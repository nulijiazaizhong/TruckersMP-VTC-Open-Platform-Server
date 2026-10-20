//! VTC 服务端 · Cloudflare Workers 形态（P0 全量 API）
//!
//! 入口是 `#[worker::event(fetch)] fn fetch(req, env, ctx)`（**3 参数**：Request, Env, Context），
//! 解析 URL → 路由 → 调 vtc-core 服务 → 返回 `worker::Response`。
//! worker 形态**不复刻 axum 中间件**，直接 `match path + method` 分发，复用 vtc-core 的 service。
//!
//! 后端装配（从 `worker::Env` 拿 D1 / R2 binding）：
//! - D1  → `db::D1Db`（本地 wrapper 实现 `Db`；内部 `Arc<D1Database>`）
//! - R2  → `storage::R2Storage`（实现 `Storage`；`env.get_binding::<Bucket>(name)`）
//!
//! **首次访问自愈（免手动建表 / 免 scheduled）**：`fetch` 第一件事是 `ensure_ready`——
//! 若 D1 里还没有 `team` 表就自动跑幂等 `db::migrate`（13 张表），缺 admin 就 seed
//! admin（密码取绑定 `VTC_ADMIN_PASSWORD`，缺省 `ChangeMe123!`）+ 默认车队。判据是
//! 「`DB` / `BUCKET` 绑定存在与否」而非探测 `SELECT`，幂等且冷启动只跑一次（约 +0.5s）。
//! 这样网页端部署**既不手动粘贴 SQL、也不用配 scheduled cron**：建好 D1/R2、绑名、
//! 上传 wasm，第一次访问即自动把库建好并种进 admin。P1 可换回 `scheduled` 事件预热。
//!
//! **DLC 模板源（worker 无文件系统）**：local 形态 `DlcService::load_template` 读本地
//! `map-dlc/<major>.<game>.txt`；worker 形态没有本地磁盘，模板改存 **R2**
//! （`dlc-templates/<major>.<game>.txt`），路由层读出文本后用 `DlcService::parse_template`
//! / `plan_disable_from` 走核心逻辑（不碰文件系统）。部署时需把 `map-dlc/` 的模板文件
//! 上传到 R2 的 `dlc-templates/` 前缀（见 README 的「Cloudflare Workers 部署」）。
//!
//! worker 0.8.6 导出速查：
//! - `r2`/`d1` 是模块（d1 需 feature），`Bucket`/`D1Database`/`Method`/`Headers` 在**顶层** re-export。
//! - `JsValue` 走 `worker::wasm_bindgen::JsValue`。
//! - `Response::builder()` → `ResponseBuilder`；`.from_json(v)` / `.ok(text)` / `.from_bytes(b)`
//!   直接返回 `Result<Response>`（默认 200）；改状态码用**消费型** `builder.with_status(n).xxx()`。
//! - `#[worker::event(fetch)]` 要求**恰好 3 个参数**：`(req: Request, env: Env, ctx: Context)`。

mod db;
mod storage;

use std::sync::Arc;

use serde::Deserialize;
use vtc_core::backend::{Db as _Db, Storage};
use vtc_core::auth::jwt::Claims;
use vtc_core::constants::role;
use vtc_core::dto;
use vtc_core::error::AppError;
use vtc_core::service::{
    AuditService, AuthService, ConfigService, ContentService, DlcService, MemberService, RateLimiter,
    SaveService,
};
// worker 0.8.6 顶层 re-export（d1 模块需 feature="d1"）
use worker::{console_log, Bucket, Context, D1Database, Env, Error, Headers, Method, Request, Response};

// 绑定名（与 wrangler.jsonc 的 bindings 对应）
const D1_BINDING: &str = "DB";
const R2_BINDING: &str = "BUCKET";
/// DLC 模板在 R2 里的前缀（部署时把 map-dlc/ 模板上传到此）
const DLC_TPL_PREFIX: &str = "dlc-templates/";
/// 种子 admin 的初始密码绑定名（缺省 `ChangeMe123!`，生产建议改为随机强口令）
const ADMIN_PWD_BINDING: &str = "VTC_ADMIN_PASSWORD";
const DEFAULT_ADMIN_PWD: &str = "ChangeMe123!";

// ---------- 响应辅助（worker::Error） ----------

/// 200 + JSON body
fn ok_json<T: serde::Serialize>(v: &T) -> Result<Response, Error> {
    Ok(Response::builder().from_json(v)?)
}

/// 带状态码的 JSON body（builder 的 `with_status` 是消费型，须在 body 构造**之前**链上）
fn json_status(status: u16, v: &impl serde::Serialize) -> Result<Response, Error> {
    Ok(Response::builder().with_status(status).from_json(v)?)
}

fn err_json(e: &AppError) -> Result<Response, Error> {
    let body = e.to_json();
    json_status(e.status(), &body)
}

fn text(status: u16, s: impl Into<String>) -> Result<Response, Error> {
    Ok(Response::builder().with_status(status).ok(s.into())?)
}

fn no_content() -> Result<Response, Error> {
    Ok(Response::builder().with_status(204).ok(String::new())?)
}

fn unauthorized() -> Result<Response, Error> {
    err_json(&AppError::Unauthorized("缺少或无效的 Bearer token".into()))
}

// ---------- 查询参数 ----------

#[derive(Deserialize, Default)]
struct Qs {
    #[serde(default = "def_take")]
    take: i64,
    #[serde(default)]
    skip: i64,
    #[serde(default = "def_page")]
    page: i64,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    game: Option<String>,
    #[serde(default)]
    major: Option<String>,
    #[serde(default)]
    action: Option<String>,
}
fn def_take() -> i64 {
    20
}
fn def_page() -> i64 {
    1
}

/// 解析 query string → T（worker 无 serde_qs，用 serde_urlencoded 手搓）
/// `T: Default`：解析失败（空 query / 类型不匹配）时退回默认值
fn parse_qs<T: serde::de::DeserializeOwned + Default>(q: &str) -> T {
    serde_urlencoded::from_str::<T>(q).unwrap_or_default()
}

#[derive(Deserialize, Default)]
struct StorageQs {
    #[serde(default)]
    key: String,
    /// HMAC 签名（P1 存储中转鉴权启用后校验；P0 占位，保持 query 契约）
    #[serde(default)]
    #[allow(dead_code)]
    sig: String,
}

// ---------- 鉴权 ----------

fn bearer_token(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")
        .ok()
        .flatten()
        .and_then(|h| {
            h.strip_prefix("Bearer ")
                .or_else(|| h.strip_prefix("bearer "))
                .map(str::to_string)
        })
}

fn require_admin(claims: &Claims) -> Result<(), AppError> {
    if claims.role == role::ADMIN || claims.role == role::MANAGER {
        Ok(())
    } else {
        Err(AppError::Forbidden("需要 Admin/Manager 角色".into()))
    }
}

// ---------- 路由入口 ----------

/// `#[worker::event(fetch)]` 入口：0.8.6 的宏要求**恰好 3 个参数**
/// `(req: Request, env: Env, ctx: Context)`。
#[worker::event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response, Error> {
    let url = req.url()?;
    let path = url.path().trim_end_matches('/').to_string();
    // `Method` 非 Copy，`match` 会 move，故 clone 一份用于 match 条件
    let method = req.method();
    let query = url.query().unwrap_or("").to_string();
    // 去掉 /api/v1 前缀
    let path = path.strip_prefix("/api/v1").unwrap_or(&path).to_string();

    // 装配后端（D1 + R2）。`?` 在绑定缺失（未配 DB/BUCKET）时提前返回，避免空指针。
    let d1: D1Database = env.d1(D1_BINDING)?;
    let bucket: Bucket = env.get_binding::<Bucket>(R2_BINDING)?;
    let cfg = worker_config();
    let db: Arc<db::D1Db> = Arc::new(db::D1Db::new(d1));
    let storage: Arc<storage::R2Storage> =
        Arc::new(storage::R2Storage::new(bucket, &cfg.public_base));

    // ★ 首次访问自愈：建表 + seed（admin / 默认车队），幂等，仅冷启动一次（约 +0.5s）。
    // 网页端部署因此免「手动粘贴 SQL」与「配 scheduled cron」。
    let pwd = match env.secret(ADMIN_PWD_BINDING) {
        Ok(s) => s.to_string(),
        Err(_) => String::new(),
    };
    // ★ 捕获 ensure_ready 的**真实**错误并返回：
    //   ensure_ready 内部（db.rs）把 D1 错误包成 AppError::Internal(真实anyhow)，但
    //   AppError 的 Display（thiserror #[error]）对 Internal 是固定「服务器内部错误」，
    //   直接 format! {e} 会丢掉真实 D1 错误。这里**匹配** AppError::Internal 提取内层
    //   anyhow（真实 D1 错误：database not found / SQL 错 / 权限等），带进 response，
    //   便于线上诊断（否则只回固定「服务器内部错误」，看不到 D1 到底报什么）。
    if let Err(e) = ensure_ready(&db, &pwd).await {
        let inner = match &e {
            AppError::Internal(inner) => inner.to_string(),
            other => other.to_string(),
        };
        return err_json(&AppError::Internal(anyhow::anyhow!(
            "D1 初始化失败（建表/种子）：{inner}；请确认 `DB` 绑定指向可用 D1"
        )));
    }
    let (jwt, rate) = worker_services(&cfg);

    // 1) storage 中转（/api/v1/storage/*，HMAC sig 鉴权，无需 Bearer）
    if path == "/storage/upload" && (method == Method::Put || method == Method::Post) {
        let qs = parse_qs::<StorageQs>(&query);
        return storage_transit(&storage, &qs.key, "PUT", Some(req.bytes().await.unwrap_or_default()))
            .await;
    }
    if path == "/storage/download" && method == Method::Get {
        let qs = parse_qs::<StorageQs>(&query);
        return storage_transit(&storage, &qs.key, "GET", None).await;
    }

    // 2) 无鉴权
    match (path.as_str(), &method) {
        ("/health", Method::Get) => return text(200, "ok"),
        ("/config/public", Method::Get) => {
            let svc = ConfigService::new(db.clone(), cfg.update_channel);
            return match svc.get_public().await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        ("/auth/login", Method::Post) => {
            // ★ 用 req.text() + serde_json::from_str（不用 req.json()）：req.json() 内部走
            // serde_wasm_bindgen::from_value，在 wasm 下对非对象 JsValue 可能 panic
            // （outcome:exception，CF 1101）。text() 只取字符串，from_str 错误走 Err 不 panic。
            // ★ console.log 标记（诊断 panic 区间）：console_error_panic_hook 把 panic 打到
            // CF console 日志；这些 log 显示「最后走到哪步」→ panic 就在下一步。定位后可删。
            console_log!("VTC login: enter, path={path}");
            let body: dto::auth::AuthLoginDto =
                match req.text().await {
                    Ok(t) => {
                        console_log!("VTC login: text() ok, len={}", t.len());
                        serde_json::from_str(&t).unwrap_or_default()
                    }
                    Err(e) => {
                        console_log!("VTC login: text() ERR: {e}");
                        dto::auth::AuthLoginDto::default()
                    }
                };
            console_log!("VTC login: parsed body, username={}", body.username);
            let auth = AuthService::new(
                db.clone(),
                jwt.clone(),
                rate.clone(),
                cfg.require_tmp,
                cfg.require_steam,
            );
            console_log!("VTC login: calling auth.login()");
            return match auth.login(&body, "127.0.0.1").await {
                Ok(v) => {
                    console_log!("VTC login: SUCCESS user={}", v.user.username);
                    ok_json(&v)
                }
                Err(e) => {
                    console_log!("VTC login: ERR {e}");
                    err_json(&e)
                }
            };
        }
        ("/auth/refresh", Method::Post) => {
            let body: dto::auth::AuthRefreshDto = match req.text().await {
                Ok(t) => serde_json::from_str(&t).unwrap_or(dto::auth::AuthRefreshDto {
                    refresh_token: String::new(),
                }),
                Err(_) => dto::auth::AuthRefreshDto { refresh_token: String::new() },
            };
            let auth = AuthService::new(db, jwt, rate, cfg.require_tmp, cfg.require_steam);
            return match auth.refresh(&body.refresh_token, "127.0.0.1").await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        // 公告（列表公开 / 详情）
        ("/announcements", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let content = ContentService::new(db.clone(), jwt.clone());
            let res = content
                .list_announcements(Some(vtc_core::constants::status::PUBLISHED), qs.take, qs.skip)
                .await;
            return match res {
                Ok((items, total)) => {
                    ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
                }
                Err(e) => err_json(&e),
            };
        }
        ("/events", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let content = ContentService::new(db.clone(), jwt.clone());
            return match content.list_events(None, qs.take, qs.skip).await {
                Ok((items, total)) => {
                    ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
                }
                Err(e) => err_json(&e),
            };
        }
        // 存档列表 + 下载凭证
        ("/saves", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let saves = SaveService::new(db.clone(), storage.clone(), jwt.clone(), cfg.public_base.clone());
            return match saves
                .list(qs.game.as_deref(), qs.major.as_deref(), qs.take, qs.skip)
                .await
            {
                Ok((items, total)) => {
                    ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
                }
                Err(e) => err_json(&e),
            };
        }
        ("/saves/{id}/download-credential", Method::Post) => {
            // 下载凭证从宽（已发布即可）；body 可选
            let body: dto::save::SaveDownloadReq = req.json().await.unwrap_or_default();
            // /saves/<id>/download-credential（3 段）→ 取中间段
            let id = save_download_id(&path).unwrap_or_default();
            let saves = SaveService::new(db.clone(), storage.clone(), jwt, cfg.public_base.clone());
            return match saves.issue_download(&id, &body, "anonymous").await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        // DLC 公开端点
        ("/dlc/map-template", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let (g, major) = dlc_game_major(&qs.game, &qs.major);
            return dlc_map_template_worker(&db, &storage, g, major).await;
        }
        ("/dlc/required", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let (g, major) = dlc_game_major(&qs.game, &qs.major);
            let dlc = DlcService::new(db.clone(), String::new());
            return match dlc.required(&g, &major).await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        _ => {}
    }

    // 3) 需 Bearer
    let token = match bearer_token(&req) {
        Some(t) => t,
        None => return unauthorized(),
    };
    let claims = match jwt.validate(&token) {
        Ok(c) => c,
        Err(e) => return err_json(&e),
    };
    let actor = claims.sub.clone();
    let ip = "127.0.0.1".to_string();

    // 自助 seed：仅 Admin（重置 admin 密码 / 补建默认车队），兜底首次访问自愈未触发或改密。
    // body: { "password": "可选，缺省用绑定 VTC_ADMIN_PASSWORD / 内置默认" }
    if path == "/seed" && method == Method::Post {
        if let Err(e) = require_admin(&claims) {
            return err_json(&e);
        }
        #[derive(serde::Deserialize, Default)]
        struct SeedBody {
            #[serde(default)]
            password: Option<String>,
        }
        let body: SeedBody = req.json().await.unwrap_or_default();
        let pwd = body
            .password
            .or_else(|| match env.secret(ADMIN_PWD_BINDING) {
                Ok(s) => {
                    let t = s.to_string();
                    (!t.is_empty()).then_some(t)
                }
                Err(_) => None,
            })
            .unwrap_or_else(|| DEFAULT_ADMIN_PWD.into());
        return match seed_admin(&db, &pwd).await {
            Ok(msg) => ok_json(&serde_json::json!({ "ok": true, "message": msg })),
            Err(e) => err_json(&e),
        };
    }

    // 审计：仅 Admin
    if path == "/audit" && method == Method::Get {
        if let Err(e) = require_admin(&claims) {
            return err_json(&e);
        }
        let qs = parse_qs::<Qs>(&query);
        let audit = AuditService::new(db.clone());
        return match audit.list(qs.action.as_deref(), qs.take, qs.skip).await {
            Ok((items, total)) => {
                ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
            }
            Err(e) => err_json(&e),
        };
    }

    // DLC 成员以上
    if path == "/dlc/report" && method == Method::Post {
        let body: dto::dlc::DlcReportDto = req.json().await.unwrap_or_default();
        let dlc = DlcService::new(db.clone(), String::new());
        return match dlc.report(&actor, &body.game, &body.missing, &body.installed).await {
            Ok(()) => ok_json(&serde_json::json!({ "ok": true })),
            Err(e) => err_json(&e),
        };
    }
    if path == "/dlc/disable" && method == Method::Post {
        let body: dto::dlc::DlcDisableReq = req.json().await.unwrap_or_default();
        let (g, major) = (body.game.clone(), body.version.clone());
        return dlc_disable_worker(&db, &storage, &actor, &ip, &body, g, major).await;
    }
    if path == "/dlc/restore" && method == Method::Post {
        let body: dto::dlc::DlcRestoreDto = req.json().await.unwrap_or_default();
        let dlc = DlcService::new(db.clone(), String::new());
        return match dlc.report_restore(&actor, &ip, &body.game, &body.restored).await {
            Ok(()) => ok_json(&serde_json::json!({ "ok": true })),
            Err(e) => err_json(&e),
        };
    }

    // 管理：成员 CRUD
    if path == "/members" && method == Method::Get {
        if let Err(e) = require_admin(&claims) {
            return err_json(&e);
        }
        let qs = parse_qs::<Qs>(&query);
        let members = MemberService::new(db.clone(), jwt.clone());
        return match members.list(qs.take, qs.skip, qs.search.as_deref()).await {
            Ok((items, total)) => {
                ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
            }
            Err(e) => err_json(&e),
        };
    }
    if path == "/members" && method == Method::Post {
        if let Err(e) = require_admin(&claims) {
            return err_json(&e);
        }
        let body: dto::member::MemberCreateDto = req.json().await.unwrap_or_default();
        let members = MemberService::new(db.clone(), jwt.clone());
        return match members.create(&body, &actor, &ip).await {
            Ok(v) => json_status(201, &v),
            Err(e) => err_json(&e),
        };
    }

    // 管理：存档（上传凭证 / 发布 / 取消 / 删除）
    if path == "/saves/upload-credential" && method == Method::Post {
        if let Err(e) = require_admin(&claims) {
            return err_json(&e);
        }
        let body: dto::save::SaveUploadReq = req.json().await.unwrap_or_default();
        let saves = SaveService::new(db.clone(), storage.clone(), jwt, cfg.public_base);
        return match saves.issue_upload(&body, &actor, &ip).await {
            Ok(v) => json_status(201, &v),
            Err(e) => err_json(&e),
        };
    }
    if let Some(id) = save_id_from(&path) {
        if path.ends_with("/publish") && method == Method::Post {
            if let Err(e) = require_admin(&claims) {
                return err_json(&e);
            }
            let body: dto::save::PublishReq = req.json().await.unwrap_or_default();
            let saves = SaveService::new(db.clone(), storage.clone(), jwt, cfg.public_base);
            return match saves.publish(&id, &body, &actor, &ip).await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        if path.ends_with("/unpublish") && method == Method::Post {
            if let Err(e) = require_admin(&claims) {
                return err_json(&e);
            }
            return save_unpublish_worker(&db, &id).await;
        }
        if is_save_id_only(&path) {
            if method == Method::Delete {
                if let Err(e) = require_admin(&claims) {
                    return err_json(&e);
                }
                let saves = SaveService::new(db.clone(), storage.clone(), jwt, cfg.public_base);
                return match saves.delete(&id, &actor, &ip).await {
                    Ok(()) => no_content(),
                    Err(e) => err_json(&e),
                };
            }
        }
    }

    // 管理：公告 CRUD
    if path == "/announcements" && method == Method::Post {
        if let Err(e) = require_admin(&claims) {
            return err_json(&e);
        }
        let body: dto::content::AnnouncementCreateDto = req.json().await.unwrap_or_default();
        let content = ContentService::new(db.clone(), jwt.clone());
        return match content.create_announcement(&body, &actor, &ip).await {
            Ok(v) => json_status(201, &v),
            Err(e) => err_json(&e),
        };
    }
    if let Some(id) = ann_or_evt_id(&path, "announcements") {
        if method == Method::Get {
            let content = ContentService::new(db.clone(), jwt.clone());
            return match content.get_announcement(&id).await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        if method == Method::Patch {
            if let Err(e) = require_admin(&claims) {
                return err_json(&e);
            }
            let body: dto::content::AnnouncementUpdateDto = req.json().await.unwrap_or_default();
            let content = ContentService::new(db.clone(), jwt.clone());
            return match content.update_announcement(&id, &body, &actor, &ip).await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        if method == Method::Delete {
            if let Err(e) = require_admin(&claims) {
                return err_json(&e);
            }
            let content = ContentService::new(db.clone(), jwt.clone());
            return match content.delete_announcement(&id, &actor, &ip).await {
                Ok(()) => no_content(),
                Err(e) => err_json(&e),
            };
        }
    }

    // 管理：活动 CRUD
    if path == "/events" && method == Method::Post {
        if let Err(e) = require_admin(&claims) {
            return err_json(&e);
        }
        let body: dto::content::EventCreateDto = req.json().await.unwrap_or_default();
        let content = ContentService::new(db.clone(), jwt.clone());
        return match content.create_event(&body, &actor, &ip).await {
            Ok(v) => json_status(201, &v),
            Err(e) => err_json(&e),
        };
    }
    if let Some(id) = ann_or_evt_id(&path, "events") {
        if method == Method::Get {
            let content = ContentService::new(db.clone(), jwt.clone());
            return match content.get_event(&id).await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        if method == Method::Patch {
            if let Err(e) = require_admin(&claims) {
                return err_json(&e);
            }
            let body: dto::content::EventUpdateDto = req.json().await.unwrap_or_default();
            let content = ContentService::new(db.clone(), jwt.clone());
            return match content.update_event(&id, &body, &actor, &ip).await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        if method == Method::Delete {
            if let Err(e) = require_admin(&claims) {
                return err_json(&e);
            }
            let content = ContentService::new(db.clone(), jwt.clone());
            return match content.delete_event(&id, &actor, &ip).await {
                Ok(()) => no_content(),
                Err(e) => err_json(&e),
            };
        }
    }

    err_json(&AppError::NotFound(format!("路由不存在：{path}")))
}

// ---------- worker 专用：DLC 模板走 R2（无本地文件系统） ----------

fn dlc_game_major(game: &Option<String>, major: &Option<String>) -> (String, String) {
    let g = match game.as_deref() {
        Some("ets2") | Some("") | None => "ets2",
        _ => "ats",
    };
    let m = major.as_deref().unwrap_or(vtc_core::constants::game::DEFAULT_MAJOR);
    (g.into(), m.into())
}

/// worker 形态：从 R2 读 DLC 模板文本 → 解析 → 返回行数组。
async fn dlc_map_template_worker(
    db: &Arc<db::D1Db>,
    storage: &Arc<storage::R2Storage>,
    game: String,
    major: String,
) -> Result<Response, Error> {
    let key = format!("{}{}.{}.txt", DLC_TPL_PREFIX, major, game);
    let dlc = DlcService::new(db.clone(), String::new());
    match storage.local_access("GET", &key, None).await {
        Ok((bytes, _, st)) if st == 200 => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            match dlc.map_template_from(&text) {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            }
        }
        Ok(_) => err_json(&AppError::NotFound(format!("DLC 模板缺失：{key}（部署时上传到 R2）"))),
        Err(e) => err_json(&e),
    }
}

/// worker 形态：从 R2 读模板 → plan_disable_from。
async fn dlc_disable_worker(
    db: &Arc<db::D1Db>,
    storage: &Arc<storage::R2Storage>,
    actor: &str,
    ip: &str,
    req: &dto::dlc::DlcDisableReq,
    game: String,
    major: String,
) -> Result<Response, Error> {
    let key = format!("{}{}.{}.txt", DLC_TPL_PREFIX, major, game);
    let dlc = DlcService::new(db.clone(), String::new());
    match storage.local_access("GET", &key, None).await {
        Ok((bytes, _, st)) if st == 200 => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let template = vtc_core::service::dlc::parse_template(&text);
            match dlc.plan_disable_from(template, actor, ip, req).await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            }
        }
        Ok(_) => err_json(&AppError::NotFound(format!("DLC 模板缺失：{key}（部署时上传到 R2）"))),
        Err(e) => err_json(&e),
    }
}

/// worker 形态：取消发布（读 save → 若已发布则置 false）。
async fn save_unpublish_worker(db: &Arc<db::D1Db>, id: &str) -> Result<Response, Error> {
    use vtc_core::backend::Db;
    match db.get_save_by_id(id).await {
        Ok(Some(s)) if s.published => {
            let _ = db.set_save_published(id, false).await;
            no_content()
        }
        Ok(Some(_)) => no_content(),
        Ok(None) => err_json(&AppError::NotFound("存档不存在".into())),
        Err(e) => err_json(&e),
    }
}

// ---------- 存储中转（R2 读写，由 presign URL 触发） ----------

/// 存储中转（R2 读写）。`S: Storage` 泛型——service 层已泛型化，worker 直接传具体 `Arc<R2Storage>`。
async fn storage_transit<S: vtc_core::backend::Storage>(
    st: &Arc<S>,
    key: &str,
    op: &str,
    body: Option<Vec<u8>>,
) -> Result<Response, Error> {
    match st.local_access(op, key, body).await {
        Ok((b, ct, status)) => {
            let h = Headers::new();
            let _ = h.set("content-type", &ct);
            Ok(Response::builder()
                .with_status(status)
                .with_headers(h)
                .from_bytes(b)?)
        }
        Err(e) => err_json(&e),
    }
}

// ---------- 路径段解析（worker 不做模板匹配，按段拆） ----------

/// 是否形如 `/saves/<id>`（恰好两段，首段=saves）
fn is_save_id_only(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    parts.len() == 2 && parts[0] == "saves"
}

/// `/saves/<id>` 的 id（仅当恰好两段且首段=saves）
fn save_id_from(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    (parts.len() == 2 && parts[0] == "saves").then(|| parts[1].to_string())
}

/// `/saves/<id>/download-credential`（3 段）的中间段 id
fn save_download_id(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    (parts.len() == 3 && parts[0] == "saves" && parts[2] == "download-credential")
        .then(|| parts[1].to_string())
}

/// `/announcements/<id>` 或 `/events/<id>` 的 id（按前缀段精确匹配）
fn ann_or_evt_id<'a>(path: &'a str, prefix: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    (parts.len() == 2 && parts[0] == prefix).then(|| parts[1].to_string())
}

// ---------- 配置 / 服务装配 ----------

struct WorkerConfig {
    jwt_secret: String,
    public_base: String,
    update_channel: String,
    require_tmp: bool,
    require_steam: bool,
}

fn worker_config() -> WorkerConfig {
    WorkerConfig {
        jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| {
            tracing::warn!("JWT_SECRET 未设置，使用开发默认值（生产必须设置 ≥32 字符）");
            "dev-insecure-secret-change-me-0123456789".into()
        }),
        public_base: std::env::var("PUBLIC_BASE")
            .unwrap_or_else(|_| "https://vtc.example.com".into()),
        update_channel: std::env::var("UPDATE_CHANNEL").unwrap_or_else(|_| "stable".into()),
        require_tmp: std::env::var("AUTH_REQUIRE_TMP").ok().as_deref() == Some("true"),
        require_steam: std::env::var("AUTH_REQUIRE_STEAM").ok().as_deref() == Some("true"),
    }
}

fn worker_services(cfg: &WorkerConfig) -> (Arc<vtc_core::auth::JwtService>, Arc<RateLimiter>) {
    (
        Arc::new(vtc_core::auth::JwtService::new(&cfg.jwt_secret)),
        Arc::new(RateLimiter::new()),
    )
}

// ---------- 首次访问自愈：建表 + 种子（幂等，免手动建表 / 免 scheduled） ----------

/// 初始化 D1：缺表则跑幂等 `migrate`，缺 admin 则 seed admin + 默认车队。
/// 判据是「`DB` 绑定是否存在」（调用方已取到 `D1Database`，存在即可建表），
/// 因此网页端部署后**第一次请求即自动建库**，冷启动约 +0.5s，幂等只跑一次。
async fn ensure_ready(db: &Arc<db::D1Db>, admin_pwd: &str) -> vtc_core::error::AppResult<()> {
    db::migrate(db).await?;
    // ★ **无条件** seed（不再判 need_admin/need_team 后条件 seed）：
    //   之前「判存在→缺失才 seed」在 CF stateless 下有**读不一致**——ensure_ready 里
    //   get_team() 判 Some（跳过 seed），但随后 /config/public 的 get_team() 又判 None
    //   （报「车队未初始化」）。根因可能是 D1 读时序/或 get_team 判断与实际写入不同步。
    //   seed_admin 内部 team 用 upsert（ON CONFLICT(id) DO UPDATE）、admin 用 upsert
    //   （ON CONFLICT(username) DO UPDATE）、member 用 insert（UUID 主键），**全部幂等**：
    //   已存在则更新/无副作用，不存在则插入。**无条件**跑保证 team+admin **一定**就位，
    //   避免「判存在跳过→实际缺失」的 read-then-act 不一致。幂等性让重复 seed 无副作用
    //   （admin 密码：upsert 会**刷新**为绑定值——注意若用户改过 admin 密码，重启/新
    //   请求会用 VTC_ADMIN_PASSWORD 覆盖；如需保留用户改的密码，后续可加「仅当 pass_hash
    //   为默认才更新」的精细条件，当前先保证初始化成功）。
    seed_admin(db, admin_pwd).await?;
    Ok(())
}

/// seed（与 vtc-server `bootstrap::seed` 对齐）：默认车队 + admin 用户 + 对应 member。
/// 仅当缺失时写，返回说明（已种子 / 已就位）。
async fn seed_admin(db: &Arc<db::D1Db>, pwd: &str) -> vtc_core::error::AppResult<String> {
    use vtc_core::constants::role;
    use vtc_core::domain::{Member, Team, User};

    let mut seeded = Vec::new();
    // ★ team **无条件** upsert（不判 get_team().is_none()）：upsert 幂等
    // （ON CONFLICT(id) DO UPDATE），已存在则刷新、缺失则插入。避免「判存在跳过→实际
    // 缺失」的 read-then-act 不一致（CF 上 get_team 判 Some 但 get_public 又判 None）。
    // created_at 用「已存在则保留原值」需查旧值；当前 upsert 会刷新 created_at，
    // 对默认车队无实际影响（仅展示用），可接受。
    let team = Team {
        id: "default".into(),
        name: std::env::var("TEAM_NAME").unwrap_or_else(|_| "默认车队".into()),
        slogan: "松散，自由的行驶、玩耍".into(),
        slug: "default".into(),
        logo: "pack://app-assets/brand/logo.png".into(),
        login_bg: "pack://app-assets/brand/login_bg.jpg".into(),
        login_layout: "left".into(),
        accent_color: "#1E88E5".into(),
        default_lang: "zh-CN".into(),
        feature_allow_upload: true,
        feature_tmp_binding: true,
        feature_rss_enabled: true,
        created_at: chrono::Utc::now(),
    };
    db.upsert_team(&team).await?;
    seeded.push("默认车队".to_string());
    if db.get_user_by_username("admin").await?.is_none() {
        let admin = User {
            id: uuid::Uuid::new_v4().to_string(),
            username: "admin".into(),
            pass_hash: vtc_core::auth::password::hash_password(pwd),
            role: role::ADMIN.into(),
            status: "Active".into(),
            tmp_player_id: None,
            steam_id: None,
            last_login: None,
            created_at: chrono::Utc::now(),
        };
        // ★ upsert（不是 insert_user）：抗并发/重入的 UNIQUE 冲突。CF stateless 冷启动
        // 窗口内多请求并发 ensure_ready 都判 admin 不存在→都 INSERT→第二个撞 user.username
        // UNIQUE。upsert 让「已存在则更新密码，不存在则插入」，永不冲突。
        db.upsert_admin_user(&admin).await?;
        db.insert_member(&Member {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: admin.id.clone(),
            role: role::ADMIN.into(),
            status: "Active".into(),
            plate: None,
            vehicle: None,
            level: 1,
            points: 0,
            joined_at: chrono::Utc::now(),
            note: Some("初始管理员".into()),
        })
        .await?;
        seeded.push("admin".to_string());
    }
    if seeded.is_empty() {
        Ok("team 与 admin 均已就位，无需 seed".into())
    } else {
        Ok(format!("已 seed：{}（admin 初始密码见绑定/默认，请登录后修改）", seeded.join("、")))
    }
}
