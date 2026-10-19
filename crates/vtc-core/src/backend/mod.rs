//! 可插拔后端抽象：Db / Storage / Push 三个 trait。
//!
//! core 服务逻辑只依赖 trait（`Arc<dyn ...>`），由具体 crate 注入实现：
//! - **vtc-server**（Docker）：`SqliteDb`（rusqlite）· `LocalStorage`/`S3Storage` · `InMemPush`
//! - **vtc-worker**（Cloudflare）：`D1Db`（worker::d1）· `R2Storage` · `DoPush`
//!
//! 全部方法 async，返回 `AppResult`。

use crate::domain::*;
use crate::dto::*;
use crate::error::AppResult;
use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use std::sync::Arc;

// ---------- Db ----------
// Send 超 trait（默认 #[async_trait]）：Docker/axum 形态要求 Handler future 是 Send，
// 故 trait 对象必须 Send。CF worker 形态（Rc<D1/R2> 非 Send）共享 vtc-core 全部业务逻辑，
// 仅后端装配不同 —— 其 ?Send/generic 适配是独立 P1 适配层，不强行穿过 axum-Send 层。
#[async_trait]
pub trait Db: Send + Sync {
    // 用户
    async fn get_user_by_username(&self, username: &str) -> AppResult<Option<User>>;
    async fn insert_user(&self, u: &User) -> AppResult<()>;
    async fn update_user_last_login(&self, id: &str, at: DateTime<Utc>) -> AppResult<()>;
    async fn get_user_by_id(&self, id: &str) -> AppResult<Option<User>>;

    // 刷新令牌
    async fn insert_refresh_token(&self, t: &RefreshToken) -> AppResult<()>;
    async fn get_refresh_token(&self, token_hash: &str) -> AppResult<Option<RefreshToken>>;
    async fn revoke_refresh_token(&self, token_hash: &str, replaced_by: Option<&str>) -> AppResult<()>;
    async fn revoke_all_for_user(&self, user_id: &str) -> AppResult<()>;

    // 成员
    async fn list_members(&self, take: i64, skip: i64, search: Option<&str>) -> AppResult<(Vec<Member>, i64)>;
    async fn get_member_by_user(&self, user_id: &str) -> AppResult<Option<Member>>;
    async fn get_member_by_id(&self, id: &str) -> AppResult<Option<Member>>;
    async fn insert_member(&self, m: &Member) -> AppResult<()>;
    async fn update_member(&self, m: &Member) -> AppResult<()>;
    async fn set_member_status(&self, id: &str, status: &str) -> AppResult<()>;

    // 存档
    async fn insert_save(&self, s: &SaveFile) -> AppResult<()>;
    async fn get_save_by_id(&self, id: &str) -> AppResult<Option<SaveFile>>;
    async fn list_saves(&self, game: Option<&str>, version_major: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<SaveFile>, i64)>;
    async fn set_save_published(&self, id: &str, published: bool) -> AppResult<()>;
    async fn bump_save_downloads(&self, id: &str) -> AppResult<()>;
    async fn delete_save(&self, id: &str) -> AppResult<()>;
    async fn insert_save_release(&self, r: &SaveRelease) -> AppResult<()>;

    // DLC
    async fn upsert_dlc_entry(&self, e: &DlcEntry) -> AppResult<()>;
    async fn list_dlc_required(&self, game: &str, major: &str) -> AppResult<Vec<DlcEntry>>;
    async fn insert_dlc_report(&self, r: &DlcReport) -> AppResult<()>;
    async fn upsert_dlc_state(&self, s: &DlcState) -> AppResult<()>;
    async fn get_dlc_state_by_member(&self, member_id: &str) -> AppResult<Option<DlcState>>;

    // 公告 / 活动
    async fn insert_announcement(&self, a: &Announcement) -> AppResult<()>;
    async fn get_announcement(&self, id: &str) -> AppResult<Option<Announcement>>;
    async fn list_announcements(&self, status: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<Announcement>, i64)>;
    async fn update_announcement(&self, a: &Announcement) -> AppResult<()>;
    async fn delete_announcement(&self, id: &str) -> AppResult<()>;

    async fn insert_event(&self, e: &Event) -> AppResult<()>;
    async fn get_event(&self, id: &str) -> AppResult<Option<Event>>;
    async fn list_events(&self, status: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<Event>, i64)>;
    async fn update_event(&self, e: &Event) -> AppResult<()>;
    async fn delete_event(&self, id: &str) -> AppResult<()>;

    // 审计
    async fn insert_audit(&self, a: &AuditLog) -> AppResult<()>;
    async fn list_audit(&self, action: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<AuditLog>, i64)>;

    // Team（单队）
    async fn get_team(&self) -> AppResult<Option<Team>>;
    async fn upsert_team(&self, t: &Team) -> AppResult<()>;
}

/// 共享 Db 句柄
pub type DbRef = Arc<dyn Db>;

// ---------- Storage ----------

#[async_trait]
pub trait Storage: Send + Sync {
    /// 生成上传（PUT）直传 URL/凭证。local 形态返回本地端点 + HMAC 签名。
    async fn presign_upload(&self, key: &str, size: i64) -> AppResult<String>;
    /// 生成下载（GET）直传 URL。
    async fn presign_download(&self, key: &str) -> AppResult<String>;
    /// 本地 / R2 中转：处理 PUT/GET（Docker 形态由 Axum 路由调用，CF 形态由 worker /storage/* 路由调用）。
    /// 返回 (body, content_type, status)。status 在 Docker 形态用于本地 404；CF 形态 R2 命中与否
    /// 由 `local_access` 自己判（R2 无独立状态码），故 content_type 也在此返回。
    async fn local_access(&self, _op: &str, _key: &str, _body: Option<Vec<u8>>) -> AppResult<(Vec<u8>, String, u16)> {
        Err(crate::error::AppError::BadRequest("local access 由具体 Storage 实现（local 磁盘 / R2）".into()))
    }
}

pub type StorageRef = Arc<dyn Storage>;

// ---------- Push（WebSocket / 事件推送） ----------

#[async_trait]
pub trait Push: Send + Sync {
    /// 推送一条事件到所有订阅者（公告/活动/任务/VTC 事件）。P0 存根即可。
    async fn publish(&self, channel: &str, payload: &str) -> AppResult<()>;
    /// 订阅数（用于诊断）。
    fn subscriber_count(&self) -> usize { 0 }
}

pub type PushRef = Arc<dyn Push>;

/// 三个后端的统一句柄（服务依赖此结构而非各自 trait）
#[derive(Clone)]
pub struct Backends {
    pub db: DbRef,
    pub storage: StorageRef,
    pub push: PushRef,
}
