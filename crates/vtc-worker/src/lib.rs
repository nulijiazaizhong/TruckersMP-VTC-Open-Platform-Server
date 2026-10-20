//! VTC 服务端 · Cloudflare Workers 形态（P0 全量 API）
//!
//! 入口是 `#[worker::event(fetch)] fn fetch(req, env, ctx)`（**3 参数**：Request, Env, Context），
//! 解析 URL → 路由 → 调 vtc-core 服务 → 返回 `worker::Response`。
//! worker 形态**不复刻 axum 中间件**，直接 `match path + method` 分发，复用 vtc-core 的 service。
//!
//! 后端装配（从 `worker::Env` 拿 D1 / R2 binding）：
//! - D1  → `db::D1Db`（本地 wrapper 实现 `Db`；内部 `Arc<D1Database>`）
//! - R2  → `storage::R2Storage`（实现 `Storage`；`env.get_binding::<Bucket>(name)`）
//! - Push→ `push::InMemPush`（push.rs，P0 存根；P1 换 Durable Objects）
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
mod push;
mod storage;

use std::sync::Arc;

use serde::Deserialize;
use vtc_core::backend::Storage;
use vtc_core::auth::jwt::Claims;
use vtc_core::constants::role;
use vtc_core::dto;
use vtc_core::error::AppError;
use vtc_core::service::{
    AuditService, AuthService, ConfigService, ContentService, DlcService, MemberService, RateLimiter,
    SaveService,
};
// worker 0.8.6 顶层 re-export（d1 模块需 feature="d1"）
use worker::{Bucket, Context, D1Database, Env, Error, Headers, Method, Request, Response};

// 绑定名（与 wrangler.jsonc 的 bindings 对应）
const D1_BINDING: &str = "DB";
const R2_BINDING: &str = "BUCKET";
/// DLC 模板在 R2 里的前缀（部署时把 map-dlc/ 模板上传到此）
const DLC_TPL_PREFIX: &str = "dlc-templates/";

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
    #[serde(default)]
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

    // 装配后端（D1 + R2；Push 内存存根）。
    let d1: D1Database = env.d1(D1_BINDING)?;
    let bucket: Bucket = env.get_binding::<Bucket>(R2_BINDING)?;
    let cfg = worker_config();
    let (jwt, rate) = worker_services(&cfg);
    let db: Arc<db::D1Db> = Arc::new(db::D1Db::new(d1));
    let storage: Arc<storage::R2Storage> =
        Arc::new(storage::R2Storage::new(bucket, &cfg.public_base));
    let _push: Arc<push::InMemPush> = Arc::new(push::InMemPush::new()); // P0 存根，P1 换 DO

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
            let body: dto::auth::AuthLoginDto = req.json().await.unwrap_or_default();
            let auth = AuthService::new(
                db.clone(),
                jwt.clone(),
                rate.clone(),
                cfg.require_tmp,
                cfg.require_steam,
            );
            return match auth.login(&body, "127.0.0.1").await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        ("/auth/refresh", Method::Post) => {
            let body: dto::auth::AuthRefreshDto = req.json().await.unwrap_or_default();
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
            let mut h = Headers::new();
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

// ---------- D1 建表（idempotent，P1 在 scheduled/start 事件里调用） ----------

/// 启动时建表（D1 方言 = SQLite；`db::MIGRATE_SQL` 与 vtc-server/src/db.rs 的 schema 一致）。
/// P0 无 `start` 事件入口（0.8.6 的 start handler 是同步的，且冷启动路径未接），
/// 此处保留公开 API 供 P1 的 `scheduled`/`start` 事件调用。
#[allow(dead_code)]
pub async fn ensure_schema(d1: &D1Database) -> vtc_core::error::AppResult<()> {
    d1
        .exec(db::MIGRATE_SQL)
        .await
        .map(|_| ())
        .map_err(|x| vtc_core::error::AppError::Internal(anyhow::anyhow!("D1 建表失败：{x}")))
}

#[allow(dead_code)]
fn _headers() -> Headers {
    Headers::new()
}
