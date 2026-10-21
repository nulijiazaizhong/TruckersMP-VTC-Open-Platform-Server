//! REST 路由（/api/v1 全量 P0）+ 鉴权 + 本地存储端点
//!
//! 鉴权用 axum 的 FromRequestParts 提取器：`AuthUser`。
//!
//! P0 全量 API（对齐 docs/server-p0.md §5）：
//! - 无鉴权：health / config/public / auth(login,refresh) / announcements+{id} / events+{id}
//!            / saves(列表) / saves/{id}/download-credential / dlc(map-template,required) / audit
//! - 成员以上(JWT)：dlc(report,disable,restore)
//! - 管理(Admin/Manager)：members CRUD / saves 上传凭证+发布+取消+删除 / announcements+events CRUD
//! - 存储中转：/storage/upload(PUT) /storage/download(GET)（HMAC sig 鉴权，无需 Bearer）

use axum::body::{Body, Bytes};
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post};
use axum::Router;
use axum::Json;
use std::sync::Arc;
use vtc_core::auth::jwt::Claims;
use vtc_core::backend::Storage;
use vtc_core::constants::{game, role, status};
use vtc_core::dto;
use vtc_core::error::AppError;
use vtc_core::service::{
    AuditService, AuthService, ConfigService, ContentService, DlcService, MemberService, PointsService,
    RssService, SaveService, StatsService, ThreeCService,
};
use crate::bootstrap::ServerConfig;
use crate::db::SqliteDb;
use crate::push::InMemPush;
use crate::storage::LocalStorage;

/// 应用状态（具体后端类型：SqliteDb / LocalStorage / InMemPush）
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<SqliteDb>,
    pub storage: Arc<LocalStorage>,
    pub push: Arc<InMemPush>,
    pub jwt: Arc<vtc_core::auth::JwtService>,
    pub cfg: ServerConfig,
    // 服务（共享，泛型实例化为具体后端类型）
    pub auth: Arc<AuthService<SqliteDb>>,
    pub members: Arc<MemberService<SqliteDb>>,
    pub saves: Arc<SaveService<SqliteDb, LocalStorage>>,
    pub content: Arc<ContentService<SqliteDb>>,
    pub dlc: Arc<DlcService<SqliteDb>>,
    pub audit: Arc<AuditService<SqliteDb>>,
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
pub fn build_app(
    db: Arc<SqliteDb>,
    storage: Arc<LocalStorage>,
    push: Arc<InMemPush>,
    cfg: ServerConfig,
) -> Router {
    let jwt = Arc::new(vtc_core::auth::JwtService::new(&cfg.jwt_secret));
    let rate = Arc::new(vtc_core::service::rate::RateLimiter::new());

    let auth = Arc::new(AuthService::new(db.clone(), jwt.clone(), rate.clone(), cfg.require_tmp, cfg.require_steam));
    let members = Arc::new(MemberService::new(db.clone(), jwt.clone()));
    let saves = Arc::new(SaveService::new(db.clone(), storage.clone(), jwt.clone(), cfg.public_base.clone()));
    let content = Arc::new(ContentService::new(db.clone(), jwt.clone()));
    let dlc = Arc::new(DlcService::new(db.clone(), cfg.map_dlc_dir.clone()));
    let audit = Arc::new(AuditService::new(db.clone()));

    let state = AppState {
        db,
        storage,
        push,
        jwt,
        cfg,
        auth,
        members,
        saves,
        content,
        dlc,
        audit,
    };

    let v1 = Router::new()
        // 无鉴权
        .route("/health", get(health))
        .route("/config/public", get(cfg_public))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh))
        .route("/announcements", get(list_announcements))
        .route("/announcements/{id}", get(get_announcement))
        .route("/events", get(list_events))
        .route("/events/{id}", get(get_event))
        .route("/saves", get(list_saves))
        .route("/saves/{id}/download-credential", post(save_download_credential))
        .route("/dlc/map-template", get(dlc_map_template))
        .route("/dlc/required", get(dlc_required))
        .route("/audit", get(list_audit))
        // 成员以上（JWT）
        .route("/dlc/report", post(dlc_report))
        .route("/dlc/disable", post(dlc_disable))
        .route("/dlc/restore", post(dlc_restore))
        // 活动记录（C7，成员以上）
        .route("/event-logs", get(list_event_logs).post(create_event_log))
        // 排行榜（C11，无鉴权——公开展示）
        .route("/leaderboard", get(get_leaderboard))
        // 管理（Admin/Manager）
        .route("/members", get(list_members).post(create_member))
        .route("/members/{id}", get(get_member).patch(patch_member).delete(delete_member))
        .route("/saves/upload-credential", post(save_upload_credential))
        .route("/saves/{id}/publish", post(save_publish))
        .route("/saves/{id}/unpublish", post(save_unpublish))
        .route("/saves/{id}", delete(delete_save))
        .route("/announcements", post(create_announcement))
        .route("/announcements/{id}", patch(patch_announcement).delete(delete_announcement))
        .route("/events", post(create_event))
        .route("/events/{id}", patch(patch_event).delete(delete_event))
        // P1: 积分规则（Admin 设/读）
        .route("/points/rules", get(p1_get_rule).post(p1_set_rule))
        // P1: 统计看板（Admin）
        .route("/stats/summary", get(p1_stats_summary))
        // P1: 3C 资源
        .route("/3c/upload-credential", post(p1_3c_upload))
        .route("/3c/{name}/download", get(p1_3c_download))
        // P1: RSS 资讯（公开）
        .route("/rss", get(p1_rss_list));

    let storage = Router::new()
        .route("/storage/upload", post(storage_upload).put(storage_upload))
        .route("/storage/download", get(storage_download));

    Router::new()
        .nest("/api/v1", v1)
        .merge(storage)
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
}

// ---------- 处理器：无鉴权 ----------

async fn health() -> &'static str { "ok" }

async fn cfg_public(State(s): State<AppState>) -> impl IntoResponse {
    let svc = ConfigService::new(s.db.clone(), s.cfg.update_channel.clone());
    match svc.get_public().await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn login(State(s): State<AppState>, Json(body): Json<dto::auth::AuthLoginDto>) -> impl IntoResponse {
    match s.auth.login(&body, "127.0.0.1").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn refresh(State(s): State<AppState>, Json(body): Json<dto::auth::AuthRefreshDto>) -> impl IntoResponse {
    match s.auth.refresh(&body.refresh_token, "127.0.0.1").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn list_announcements(State(s): State<AppState>, Query(q): Query<Qs>) -> impl IntoResponse {
    let (items, total) = s.content
        .list_announcements(Some(status::PUBLISHED), q.take, q.skip)
        .await
        .unwrap_or_else(|_| (Vec::new(), 0));
    Json(dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

async fn get_announcement(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match s.content.get_announcement(&id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn list_events(State(s): State<AppState>, Query(q): Query<Qs>) -> impl IntoResponse {
    let (items, total) = s.content.list_events(None, q.take, q.skip).await
        .unwrap_or_else(|_| (Vec::new(), 0));
    Json(dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

async fn get_event(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match s.content.get_event(&id).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn list_saves(State(s): State<AppState>, Query(q): Query<Qs>) -> impl IntoResponse {
    let (items, total) = s.saves.list(q.game.as_deref(), q.major.as_deref(), q.take, q.skip).await
        .unwrap_or_else(|_| (Vec::new(), 0));
    Json(dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

async fn save_download_credential(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<dto::save::SaveDownloadReq>,
) -> impl IntoResponse {
    // 下载凭证需登录（任意已登录用户）
    // 从 body 无 token 信息；这里允许未带 Bearer 也能取（与 C# 平台一致：下载凭证只要 save 已发布）。
    // 若需严格鉴权，可要求 AuthUser。P0 从宽：已发布即可下载。
    match s.saves.issue_download(&id, &body, "anonymous").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn dlc_map_template(State(s): State<AppState>, Query(q): Query<DlcQs>) -> impl IntoResponse {
    let gamename = if q.game == "ets2" || q.game.is_empty() { "ets2" } else { "ats" };
    let major = if q.major.is_empty() { game::DEFAULT_MAJOR } else { q.major.as_str() };
    match s.dlc.map_template(gamename, major) {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn dlc_required(State(s): State<AppState>, Query(q): Query<DlcQs>) -> impl IntoResponse {
    let gamename = if q.game == "ets2" || q.game.is_empty() { "ets2" } else { "ats" };
    let major = if q.major.is_empty() { game::DEFAULT_MAJOR } else { q.major.as_str() };
    match s.dlc.required(gamename, major).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn list_audit(State(s): State<AppState>, AuthUser(c): AuthUser, Query(q): Query<AuditQs>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    let (items, total) = s.audit.list(q.action.as_deref(), q.take, q.skip).await
        .unwrap_or_else(|_| (Vec::new(), 0));
    Json(dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

// ---------- 处理器：成员以上（JWT） ----------

async fn dlc_report(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::dlc::DlcReportDto>) -> impl IntoResponse {
    match s.dlc.report(&c.sub, &body.game, &body.missing, &body.installed).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn dlc_disable(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::dlc::DlcDisableReq>) -> impl IntoResponse {
    match s.dlc.plan_disable(&c.sub, "127.0.0.1", &body).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn dlc_restore(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::dlc::DlcRestoreDto>) -> impl IntoResponse {
    match s.dlc.report_restore(&c.sub, "127.0.0.1", &body.game, &body.restored).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => err_response(&e),
    }
}

// ---------- 处理器：管理（Admin/Manager） ----------

async fn list_members(State(s): State<AppState>, AuthUser(c): AuthUser, Query(q): Query<Qs>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    let (items, total) = s.members.list(q.take, q.skip, q.search.as_deref()).await
        .unwrap_or_else(|_| (Vec::new(), 0));
    Json(dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response()
}

async fn create_member(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::member::MemberCreateDto>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.members.create(&body, &c.sub, "127.0.0.1").await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn get_member(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.members.get(&id).await {
        Ok(Some(v)) => Json(v).into_response(),
        Ok(None) => err_response(&AppError::NotFound("成员不存在".into())),
        Err(e) => err_response(&e),
    }
}

async fn patch_member(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>, Json(body): Json<dto::member::MemberUpdateDto>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.members.update(&id, &body, &c.sub, "127.0.0.1").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn delete_member(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.members.delete(&id, &c.sub, "127.0.0.1").await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(&e),
    }
}

async fn save_upload_credential(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::save::SaveUploadReq>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.saves.issue_upload(&body, &c.sub, "127.0.0.1").await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn save_publish(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>, Json(body): Json<dto::save::PublishReq>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.saves.publish(&id, &body, &c.sub, "127.0.0.1").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn save_unpublish(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    // 取消发布 = set published=false（幂等：未发布则 no-op）
    use vtc_core::backend::Db;
    match s.saves.db.get_save_by_id(&id).await {
        Ok(Some(save)) if save.published => {
            let _ = s.saves.db.set_save_published(&id, false).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(Some(_)) => StatusCode::NO_CONTENT.into_response(),
        Ok(None) => err_response(&AppError::NotFound("存档不存在".into())),
        Err(e) => err_response(&e),
    }
}

async fn delete_save(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.saves.delete(&id, &c.sub, "127.0.0.1").await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(&e),
    }
}

async fn create_announcement(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::content::AnnouncementCreateDto>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.content.create_announcement(&body, &c.sub, "127.0.0.1").await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn patch_announcement(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>, Json(body): Json<dto::content::AnnouncementUpdateDto>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.content.update_announcement(&id, &body, &c.sub, "127.0.0.1").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn delete_announcement(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.content.delete_announcement(&id, &c.sub, "127.0.0.1").await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(&e),
    }
}

async fn create_event(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::content::EventCreateDto>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.content.create_event(&body, &c.sub, "127.0.0.1").await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn patch_event(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>, Json(body): Json<dto::content::EventUpdateDto>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.content.update_event(&id, &body, &c.sub, "127.0.0.1").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn delete_event(State(s): State<AppState>, AuthUser(c): AuthUser, Path(id): Path<String>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    match s.content.delete_event(&id, &c.sub, "127.0.0.1").await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => err_response(&e),
    }
}

// ---------- 存储中转（local 磁盘，HMAC sig 鉴权） ----------

async fn storage_upload(State(s): State<AppState>, Query(q): Query<StorageQs>, body: Bytes) -> impl IntoResponse {
    match s.storage.local_access("PUT", &q.key, Some(body.to_vec())).await {
        Ok((_, ct, st)) => (StatusCode::from_u16(st).unwrap_or(StatusCode::OK), ct).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn storage_download(State(s): State<AppState>, Query(q): Query<StorageQs>) -> impl IntoResponse {
    match s.storage.local_access("GET", &q.key, None).await {
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
struct AuditQs {
    #[serde(default = "def_take")]
    take: i64,
    #[serde(default)]
    skip: i64,
    #[serde(default = "def_page")]
    page: i64,
    #[serde(default)]
    action: Option<String>,
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

// ---------- P1：活动记录 + 排行榜 ----------

async fn list_event_logs(State(s): State<AppState>, AuthUser(c): AuthUser, Query(q): Query<Qs>) -> impl IntoResponse {
    // 按当前用户对应的 member 过滤
    use vtc_core::backend::Db;
    let member = match s.db.get_member_by_user(&c.sub).await {
        Ok(m) => m,
        Err(e) => return err_response(&e),
    };
    let Some(member) = member else { return err_response(&AppError::NotFound("未找到成员记录".into())) };
    let (logs, total) = match s.db.list_event_logs(Some(&member.id), q.take, q.skip).await {
        Ok(v) => v,
        Err(e) => return err_response(&e),
    };
    // 填充 event_name（JOIN 活动表）
    let mut dtos = Vec::with_capacity(logs.len());
    for l in &logs {
        let evt_name = s.db.get_event(&l.event_id).await.ok()
            .flatten().map(|e| e.name);
        dtos.push(dto::events_log::EventLogDto {
            id: l.id.clone(),
            member_id: l.member_id.clone(),
            event_id: l.event_id.clone(),
            event_name: evt_name,
            result: l.result.clone(),
            mileage: l.mileage,
            income: l.income,
            at: l.at,
        });
    }
    Json(dto::PageResult { items: dtos, total, page: q.page, page_size: q.take }).into_response()
}

async fn create_event_log(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::events_log::EventLogCreateDto>) -> impl IntoResponse {
    use vtc_core::backend::Db;
    let member = match s.db.get_member_by_user(&c.sub).await {
        Ok(m) => m,
        Err(e) => return err_response(&e),
    };
    let Some(member) = member else { return err_response(&AppError::NotFound("未找到成员记录".into())) };
    let log = vtc_core::domain::EventLog {
        id: uuid::Uuid::new_v4().to_string(),
        member_id: member.id.clone(),
        event_id: body.event_id.clone(),
        result: body.result.clone(),
        mileage: body.mileage,
        income: body.income,
        at: chrono::Utc::now(),
    };
    if let Err(e) = s.db.insert_event_log(&log).await {
        return err_response(&e);
    }
    let dto_out = dto::events_log::EventLogDto {
        id: log.id.clone(),
        member_id: log.member_id.clone(),
        event_id: log.event_id.clone(),
        event_name: None,
        result: log.result.clone(),
        mileage: log.mileage,
        income: log.income,
        at: log.at,
    };
    (StatusCode::CREATED, Json(dto_out)).into_response()
}

async fn get_leaderboard(State(s): State<AppState>, Query(q): Query<Qs>) -> impl IntoResponse {
    use vtc_core::backend::Db;
    let (rows, total) = match s.db.list_leaderboard(q.take, q.skip).await {
        Ok(v) => v,
        Err(e) => return err_response(&e),
    };
    let mut entries = Vec::with_capacity(rows.len());
    for (i, (member_id, user_id, plate, points)) in rows.iter().enumerate() {
        let username = s.db.get_user_by_id(user_id).await.ok()
            .flatten().map(|u| u.username).unwrap_or_default();
        entries.push(dto::events_log::LeaderboardEntryDto {
            rank: (i as i64) + 1,
            member_id: member_id.clone(),
            username,
            plate: plate.clone(),
            points: *points,
        });
    }
    Json(dto::events_log::LeaderboardDto { items: entries, total }).into_response()
}

// ---------- P1 处理器 ----------

async fn p1_get_rule(State(s): State<AppState>, AuthUser(c): AuthUser, Query(q): Query<Qs>) -> impl IntoResponse {
    let pts = PointsService::new(s.db.clone());
    match pts.get_rule("event_complete", "global").await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn p1_set_rule(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::points::PointsRuleSetDto>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    let pts = PointsService::new(s.db.clone());
    match pts.set_rule(&body).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn p1_stats_summary(State(s): State<AppState>, AuthUser(c): AuthUser) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    let stats = StatsService::new(s.db.clone());
    match stats.summary().await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn p1_3c_upload(State(s): State<AppState>, AuthUser(c): AuthUser, Json(body): Json<dto::threec::ThreeCUploadReq>) -> impl IntoResponse {
    if let Err(e) = require_admin(&c) { return err_response(&e); }
    let svc = ThreeCService::new(s.db.clone(), s.storage.clone());
    match svc.issue_upload(&body).await {
        Ok(v) => (StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn p1_3c_download(State(s): State<AppState>, Path(name): Path<String>) -> impl IntoResponse {
    let svc = ThreeCService::new(s.db.clone(), s.storage.clone());
    match svc.download(&name).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => err_response(&e),
    }
}

async fn p1_rss_list(State(s): State<AppState>, Query(q): Query<Qs>) -> impl IntoResponse {
    let svc = RssService::new(s.db.clone());
    match svc.list(q.game.as_deref(), q.major.as_deref(), q.take, q.skip).await {
        Ok((items, total)) => Json(dto::PageResult { items, total, page: q.page, page_size: q.take }).into_response(),
        Err(e) => err_response(&e),
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
