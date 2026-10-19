//! REST 路由（/api/v1 全量）+ 鉴权 + 本地存储端点
//!
//! 鉴权用 axum 的 FromRequestParts 提取器：`AuthUser`。

use axum::body::{Body, Bytes};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use std::sync::Arc;
use vtc_core::auth::jwt::Claims;
use vtc_core::backend::Backends;
use vtc_core::constants::{game, role, status};
use vtc_core::error::AppError;
use vtc_core::service::{
    AuthService, ConfigService, ContentService, DlcService, MemberService, SaveService,
};
use crate::bootstrap::ServerConfig;

/// 应用状态
#[derive(Clone)]
pub struct AppState {
    pub backends: Backends,
    pub jwt: Arc<vtc_core::auth::JwtService>,
    pub cfg: ServerConfig,
    // 服务（共享）
    pub auth: Arc<AuthService>,
    pub members: Arc<MemberService>,
    pub saves: Arc<SaveService>,
    pub content: Arc<ContentService>,
    pub dlc: Arc<DlcService>,
}

/// 鉴权提取器：解析 Bearer token → Claims
pub struct AuthUser(pub Claims);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let header = parts.headers.get("authorization").and_then(|h| h.to_str().ok());
        let token = match header.and_then(|h| h.strip_prefix("Bearer ").or_else(|| h.strip_prefix("bearer "))) {
            Some(t) => t,
            None => return Err(err_response(&AppError::Unauthorized("缺少 Bearer token".into()))),
        };
        match state.jwt.validate(token) {
            Ok(c) => Ok(AuthUser(c)),
            Err(e) => return Err(err_response(&e)),
        }
    }
}

/// 构造路由器
pub fn build_app(backends: Backends, cfg: ServerConfig) -> axum::Router {
    let jwt = Arc::new(vtc_core::auth::JwtService::new(&cfg.jwt_secret));
    let rate = Arc::new(vtc_core::service::rate::RateLimiter::new());

    let auth = Arc::new(AuthService::new(backends.clone(), jwt.clone(), rate.clone(), cfg.require_tmp, cfg.require_steam));
    let members = Arc::new(MemberService::new(backends.clone(), jwt.clone()));
    let saves = Arc::new(SaveService::new(backends.clone(), jwt.clone(), cfg.public_base.clone()));
    let content = Arc::new(ContentService::new(backends.clone(), jwt.clone()));
    let dlc = Arc::new(DlcService::new(backends.clone(), cfg.map_dlc_dir.clone()));

    let state = AppState {
        backends,
        jwt,
        cfg,
        auth,
        members,
        saves,
        content,
        dlc,
    };

    let v1 = axum::Router::new()
        .route("/health", get(health))
        .route("/config/public", get(cfg_public))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/members", get(list_members))
        .route("/members/{id}", get(get_member))
        .route("/saves", get(list_saves))
        .route("/events", get(list_events))
        .route("/announcements", get(list_announcements))
        .route("/dlc/map-template", get(dlc_map_template));

    let storage = axum::Router::new()
        .route("/storage/upload", post(storage_upload))
        .route("/storage/download", get(storage_download));

    axum::Router::new()
        .nest("/api/v1", v1)
        .merge(storage)
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

// ---------- 处理器 ----------

async fn health() -> &'static str { "ok" }

async fn cfg_public(State(s): State<AppState>) -> impl IntoResponse {
    let svc = ConfigService::new(s.backends.clone(), s.cfg.update_channel.clone());
    match svc.get_public().await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn login(State(s): State<AppState>, Json(body): Json<vtc_core::dto::auth::AuthLoginDto>) -> impl IntoResponse {
    match s.auth.login(&body, "127.0.0.1").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn refresh(State(s): State<AppState>, Json(body): Json<vtc_core::dto::auth::AuthRefreshDto>) -> impl IntoResponse {
    match s.auth.refresh(&body.refresh_token, "127.0.0.1").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn list_members(State(s): State<AppState>, AuthUser(c): AuthUser, Query(q): Query<Qs>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    let (items, total) = s.members.list(q.take, q.skip, q.search.as_deref()).await
        .unwrap_or_else(|e| (Vec::new(), 0));
    Json(vtc_core::dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

async fn get_member(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.members.get(&id).await {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => err_response(&AppError::NotFound("成员不存在".into())),
        Err(e) => err_response(&e),
    }
}

async fn list_saves(State(s): State<AppState>, AuthUser(_c): AuthUser, Query(q): Query<Qs>) -> impl IntoResponse {
    let (items, total) = s.saves.list(None, None, q.take, q.skip).await
        .unwrap_or_else(|e| (Vec::new(), 0));
    Json(vtc_core::dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

async fn list_events(State(s): State<AppState>, AuthUser(_c): AuthUser, Query(q): Query<Qs>) -> impl IntoResponse {
    let (items, total) = s.content.list_events(None, q.take, q.skip).await
        .unwrap_or_else(|e| (Vec::new(), 0));
    Json(vtc_core::dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

async fn list_announcements(State(s): State<AppState>, AuthUser(_c): AuthUser, Query(q): Query<Qs>) -> impl IntoResponse {
    let (items, total) = s.content.list_announcements(Some(status::PUBLISHED), q.take, q.skip).await
        .unwrap_or_else(|e| (Vec::new(), 0));
    Json(vtc_core::dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

async fn dlc_map_template(State(s): State<AppState>, AuthUser(_c): AuthUser, Query(q): Query<DlcQs>) -> impl IntoResponse {
    let gamename = if q.game == "ets2" || q.game.is_empty() { "ets2" } else { "ats" };
    let major = if q.major.is_empty() { game::DEFAULT_MAJOR } else { q.major.as_str() };
    match s.dlc.map_template(gamename, major) {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn storage_upload(State(s): State<AppState>, Query(q): Query<StorageQs>, body: Bytes) -> impl IntoResponse {
    match s.backends.storage.local_access("PUT", &q.key, Some(body.to_vec())).await {
        Ok((_, ct, st)) => (StatusCode::from_u16(st).unwrap_or(StatusCode::OK), ct).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn storage_download(State(s): State<AppState>, Query(q): Query<StorageQs>) -> impl IntoResponse {
    match s.backends.storage.local_access("GET", &q.key, None).await {
        Ok((b, ct, st)) => {
            Response::builder()
                .status(StatusCode::from_u16(st).unwrap_or(StatusCode::OK))
                .header("content-type", ct)
                .body(Body::from(b))
                .expect("build storage response")
        }
        Err(e) => err_response(&e),
    }
}

// ---------- 查询参数 ----------

#[derive(serde::Deserialize)]
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
fn def_take() -> i64 { 20 }
fn def_page() -> i64 { 1 }

#[derive(serde::Deserialize)]
struct DlcQs {
    #[serde(default)]
    game: String,
    #[serde(default)]
    major: String,
}

#[derive(serde::Deserialize)]
struct StorageQs {
    key: String,
    #[serde(default)]
    sig: String,
}

fn require_admin(claims: &Claims) -> Result<(), AppError> {
    if claims.role == role::ADMIN || claims.role == role::MANAGER {
        Ok(())
    } else {
        Err(AppError::Forbidden("需要 Admin/Manager 角色".into()))
    }
}

// AppError → Response
// 注意：AppError 来自 vtc-core（外部 crate），axum 的 IntoResponse 是外部 trait，
// 无法在外层 crate 实现（orphan rule），故所有处理器用本地函数 `err_response` 构造响应。
fn err_response(e: &AppError) -> Response {
    Response::builder()
        .status(e.status())
        .header("content-type", "application/json")
        .body(Body::from(e.to_json().to_string()))
        .unwrap_or_else(|_| Response::builder().status(e.status()).body(Body::empty()).expect("build"))
}
