//! VTC 服务端 · Cloudflare Workers 形态
//!
//! 入口是 `#[worker::event(fetch)] fn fetch`：解析 URL → 路由 → 调 vtc-core 服务 → 返回 `worker::Response`。
//! worker 形态**不复刻 axum 中间件**，直接 `match path + method` 分发，复用 vtc-core 的 service。
//!
//! 后端装配（从 `worker::Context` 拿 D1 / R2 binding）：
//! - D1  → `Arc<D1Database>`（db.rs 实现 Db）
//! - R2  → `R2Storage`（storage.rs 实现 Storage；`ctx.get_binding::<Bucket>(name)`）
//! - Push→ `InMemPush`（push.rs，P0 存根）
//!
//! 注意：DLC 模板 / `load_template` 读本地文件，worker 无文件系统 → 该端点返回 501（P1 改 KV/R2 模板）。
//!
//! 注意（worker 0.8.6 导出）：`r2`/`d1`/`http` 模块是私有的，`Bucket`/`D1Database`/`Method`/`Headers`
//! 都在**顶层** re-export（`use worker::{Bucket, Method, ...}`）。

mod db;
mod push;
mod storage;

use std::sync::Arc;

use serde::Deserialize;
use vtc_core::auth::jwt::Claims;
use vtc_core::backend::Backends;
use vtc_core::dto;
use vtc_core::error::AppError;
use vtc_core::service::{
    AuthService, ConfigService, ContentService, MemberService, RateLimiter, SaveService,
};
// 注意（worker 0.8.6）：
// - `#[worker::event(fetch)]` 的第三个参数是**原始** `worker_sys::Context`（宏内部再 `worker::Context::new`），
//   这里签名必须用 `worker_sys::Context` 而不是包装后的 `worker::Context`。
// - `Bucket`/`D1Database`/`Method`/`Headers` 在顶层 re-export；`JsValue` 在 `worker::js_sys`（worker 转导出 js_sys）。
use worker::js_sys::JsValue;
use worker::{
    d1::D1Database, worker_sys, Bucket, Error, Headers, Method, Response, Request,
};

// 绑定名（与 wrangler.jsonc 的 bindings 对应）
const D1_BINDING: &str = "DB";
const R2_BINDING: &str = "BUCKET";

// ---------- 响应辅助（worker::Error） ----------

fn ok_json<T: serde::Serialize>(v: &T) -> Result<Response, Error> {
    Ok(Response::builder().from_json(v)?.with_status(200).ok()?)
}

fn err_json(e: &AppError) -> Result<Response, Error> {
    let body = e.to_json();
    Ok(Response::builder().from_json(&body)?.with_status(e.status()).ok()?)
}

fn text(status: u16, s: impl Into<String>) -> Result<Response, Error> {
    Ok(Response::builder().ok(s.into())?.with_status(status))
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
fn parse_qs<T: serde::de::DeserializeOwned>(q: &str) -> T {
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

#[worker::event(fetch)]
async fn fetch(req: Request, ctx: Context) -> Result<Response, Error> {
    let url = req.url()?;
    let path = url.path().trim_end_matches('/').to_string();
    let method = req.method();
    let query = url.query().unwrap_or("").to_string();
    // 去掉 /api/v1 前缀
    let path = path.strip_prefix("/api/v1").unwrap_or(&path).to_string();

    // 装配后端（D1 + R2；Push 内存存根）
    let d1: D1Database = ctx.d1(D1_BINDING)?;
    let bucket: Bucket = ctx.get_binding::<Bucket>(R2_BINDING)?;
    let cfg = worker_config();
    let (jwt, rate) = worker_services(&cfg);
    let backends = Backends {
        db: Arc::new(d1),
        storage: Arc::new(storage::R2Storage::new(bucket, &cfg.public_base)),
        push: Arc::new(push::InMemPush::new()),
    };

    // 1) storage 中转（/api/v1/storage/*，HMAC sig 鉴权，无需 Bearer）
    if path == "/storage/upload" && method == Method::Put {
        let qs = parse_qs::<StorageQs>(&query);
        return storage_transit(&backends.storage, &qs.key, "PUT", Some(req.bytes().await.unwrap_or_default())).await;
    }
    if path == "/storage/download" && method == Method::Get {
        let qs = parse_qs::<StorageQs>(&query);
        return storage_transit(&backends.storage, &qs.key, "GET", None).await;
    }

    // 2) 无鉴权
    match (path.as_str(), method) {
        ("/health", Method::Get) => return text(200, "ok"),
        ("/config/public", Method::Get) => {
            let svc = ConfigService::new(backends, cfg.update_channel);
            return match svc.get_public().await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        ("/auth/login", Method::Post) => {
            let body: dto::auth::AuthLoginDto = req.json().await.unwrap_or_default();
            let auth = AuthService::new(backends, jwt, rate, cfg.require_tmp, cfg.require_steam);
            return match auth.login(&body, "127.0.0.1").await {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(&e),
            };
        }
        ("/auth/refresh", Method::Post) => {
            let body: dto::auth::AuthRefreshDto = req.json().await.unwrap_or_default();
            let auth = AuthService::new(backends, jwt, rate, cfg.require_tmp, cfg.require_steam);
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

    if claims.role != vtc_core::constants::role::ADMIN && claims.role != vtc_core::constants::role::MANAGER
        && matches!(path.as_str(), "/members")
    {
        return err_json(&AppError::Forbidden("需要 Admin/Manager 角色".into()));
    }

    match (path.as_str(), method) {
        ("/members", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let members = MemberService::new(backends, jwt);
            match members.list(qs.take, qs.skip, qs.search.as_deref()).await {
                Ok((items, total)) => ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take }),
                Err(e) => err_json(&e),
            }
        }
        ("/saves", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let saves = SaveService::new(backends, jwt, cfg.public_base);
            match saves.list(qs.game.as_deref(), qs.major.as_deref(), qs.take, qs.skip).await {
                Ok((items, total)) => ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take }),
                Err(e) => err_json(&e),
            }
        }
        ("/events", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let content = ContentService::new(backends, jwt);
            match content.list_events(None, qs.take, qs.skip).await {
                Ok((items, total)) => ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take }),
                Err(e) => err_json(&e),
            }
        }
        ("/announcements", Method::Get) => {
            let qs = parse_qs::<Qs>(&query);
            let content = ContentService::new(backends, jwt);
            match content
                .list_announcements(Some(vtc_core::constants::status::PUBLISHED), qs.take, qs.skip)
                .await
            {
                Ok((items, total)) => ok_json(&dto::PageResult { items, total, page: qs.page, page_size: qs.take }),
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

async fn storage_transit(
    st: &Arc<dyn vtc_core::backend::Storage>,
    key: &str,
    op: &str,
    body: Option<Vec<u8>>,
) -> Result<Response, Error> {
    match st.local_access(op, key, body).await {
        Ok((b, ct, status)) => {
            let mut h = Headers::new();
            let _ = h.set("content-type", &ct);
            Ok(Response::builder().from_bytes(b)?.with_status(status).with_headers(h))
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
    (Arc::new(vtc_core::auth::JwtService::new(&cfg.jwt_secret)), Arc::new(RateLimiter::new()))
}

// ---------- D1 建表（idempotent，可在 worker 冷启动时跑） ----------

pub async fn ensure_schema(d1: &D1Database) -> vtc_core::error::AppResult<()> {
    db::migrate(d1).await
}

#[allow(dead_code)]
fn _headers() -> Headers {
    Headers::new()
}
