//! VTC 服务端 · Cloudflare Workers 形态
//!
//! 入口是 `#[worker::event(fetch)] fn fetch(req, env, ctx)`（**3 参数**：Request, Env, Context），
//! 解析 URL → 路由 → 调 vtc-core 服务 → 返回 `worker::Response`。
//! worker 形态**不复刻 axum 中间件**，直接 `match path + method` 分发，复用 vtc-core 的 service。
//!
//! 后端装配（从 `worker::Env` 拿 D1 / R2 binding）：
//! - D1  → `db::D1Db`（本地 wrapper 实现 `Db`；内部 `Arc<D1Database>`）
//! - R2  → `storage::R2Storage`（实现 `Storage`；`env.get_binding::<Bucket>(name)`）
//! - Push→ `push::InMemPush`（push.rs，P0 存根）
//!
//! 注意：DLC 模板 / `load_template` 读本地文件，worker 无文件系统 → 该端点返回 501（P1 改 KV/R2 模板）。
//!
//! worker 0.8.6 导出速查：
//! - `r2`/`d1` 是模块（d1 需 feature），`Bucket`/`D1Database`/`Method`/`Headers` 在**顶层** re-export
//!   （`use worker::{Bucket, Method, ...}`）。
//! - `JsValue` 走 `worker::wasm_bindgen::JsValue`（`worker::js_sys::JsValue` 不可用）。
//! - `Response::builder()` → `ResponseBuilder`；`.from_json(v)` / `.ok(text)` / `.from_bytes(b)` 直接
//!   返回 `Result<Response>`（默认 200）；改状态码用**消费型** `builder.with_status(n).xxx()` 链，
//!   或已 build 的 `Response` 上的 `with_status`（消费 self 返回 Self）。
//! - `#[worker::event(fetch)]` 要求**恰好 3 个参数**：`(req: Request, env: Env, ctx: Context)`，
//!   宏内部对 ctx 执行 `worker::Context::new(ctx)`。
//! - `D1PreparedStatement::raw<T: Deserialize>` 返回**扁平** `Vec<T>`（每 cell 一项，非按行嵌套）。

mod db;
mod push;
mod storage;

use std::sync::Arc;

use serde::Deserialize;
use vtc_core::auth::jwt::Claims;
use vtc_core::dto;
use vtc_core::error::AppError;
use vtc_core::service::{
    AuthService, ConfigService, ContentService, MemberService, RateLimiter, SaveService,
};
// worker 0.8.6 顶层 re-export（d1 模块需 feature="d1"）
use worker::{Bucket, Context, D1Database, Env, Error, Headers, Method, Request, Response};

// 绑定名（与 wrangler.jsonc 的 bindings 对应）
const D1_BINDING: &str = "DB";
const R2_BINDING: &str = "BUCKET";

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

// ---------- 路由入口 ----------

/// `#[worker::event(fetch)]` 入口：0.8.6 的宏要求**恰好 3 个参数**
/// `(req: Request, env: Env, ctx: Context)`。
/// 宏内部：`worker::Context::new(ctx)`（ctx 实参是 `worker_sys::Context`，宏签名用 worker::Context 即可，
/// 因为 wrapper 内部会构造好）。
#[worker::event(fetch)]
async fn fetch(mut req: Request, env: Env, ctx: Context) -> Result<Response, Error> {
    let url = req.url()?;
    let path = url.path().trim_end_matches('/').to_string();
    // `Method` 非 Copy，`match` 会 move，故 clone 一份用于 match 条件
    let method = req.method();
    let query = url.query().unwrap_or("").to_string();
    // 去掉 /api/v1 前缀
    let path = path.strip_prefix("/api/v1").unwrap_or(&path).to_string();

    // 装配后端（D1 + R2；Push 内存存根）。
    // service 层已泛型化（不持 Backends/dyn），worker 把**具体** `Arc<DbType>` /
    // `Arc<StorageType>` 传给各 service 的 `new()`。
    let d1: D1Database = env.d1(D1_BINDING)?;
    let bucket: Bucket = env.get_binding::<Bucket>(R2_BINDING)?;
    let cfg = worker_config();
    let (jwt, rate) = worker_services(&cfg);
    let db: Arc<db::D1Db> = Arc::new(db::D1Db::new(d1));
    let storage: Arc<storage::R2Storage> =
        Arc::new(storage::R2Storage::new(bucket, &cfg.public_base));
    let _push: Arc<push::InMemPush> = Arc::new(push::InMemPush::new()); // P0 存根，未用于 service

    // 1) storage 中转（/api/v1/storage/*，HMAC sig 鉴权，无需 Bearer）
    if path == "/storage/upload" && method == Method::Put {
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
            let auth =
                AuthService::new(db, jwt, rate, cfg.require_tmp, cfg.require_steam);
            return match auth.refresh(&body.refresh_token, "127.0.0.1").await {
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
    let _ = cfg; // 占位（public_base 已在 backends.storage 里）

    if claims.role != vtc_core::constants::role::ADMIN
        && claims.role != vtc_core::constants::role::MANAGER
        && matches!(path.as_str(), "/members")
    {
        return err_json(&AppError::Forbidden("需要 Admin/Manager 角色".into()));
    }

    match (path.as_str(), &method) {
        ("/members", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let members = MemberService::new(db, jwt);
            match members.list(qs.take, qs.skip, qs.search.as_deref()).await {
                Ok((items, total)) => {
                    ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
                }
                Err(e) => err_json(&e),
            }
        }
        ("/saves", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let saves = SaveService::new(db, storage, jwt, cfg.public_base);
            match saves
                .list(qs.game.as_deref(), qs.major.as_deref(), qs.take, qs.skip)
                .await
            {
                Ok((items, total)) => {
                    ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
                }
                Err(e) => err_json(&e),
            }
        }
        ("/events", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let content = ContentService::new(db, jwt);
            match content.list_events(None, qs.take, qs.skip).await {
                Ok((items, total)) => {
                    ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
                }
                Err(e) => err_json(&e),
            }
        }
        ("/announcements", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let content = ContentService::new(db, jwt);
            match content
                .list_announcements(Some(vtc_core::constants::status::PUBLISHED), qs.take, qs.skip)
                .await
            {
                Ok((items, total)) => {
                    ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take })
                }
                Err(e) => err_json(&e),
            }
        }
        ("/dlc/map-template", Method::Get) => {
            // worker 无文件系统；DLC 模板端点 P1 改 KV/R2 模板
            err_json(&AppError::Internal(anyhow::anyhow!("map-template 需要文件/KV 模板（P1）")))
        }
        _ => err_json(&AppError::NotFound(format!("路由不存在：{path}"))),
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
            // builder 的 with_status 是消费型：with_status → with_headers → from_bytes 必须按此顺序链
            Ok(Response::builder()
                .with_status(status)
                .with_headers(h)
                .from_bytes(b)?)
        }
        Err(e) => err_json(&e),
    }
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
        public_base: std::env::var("PUBLIC_BASE").unwrap_or_else(|_| "https://vtc.example.com".into()),
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
