//! 可插拔后端抽象：Db / Storage / Push 三个 trait。
//!
//! **双目标 async 门控**（按 vtc-core 的 cargo feature `worker`，cargo feature 按依赖边生效）：
//! - vtc-server 依赖 vtc-core（默认，不开 `worker`）→ trait 用 `#[async_trait]`（**Send**）。
//!   axum 的 `Handler` 要求 future 是 Send。
//! - vtc-worker 依赖 vtc-core 时开 `features=["worker"]` → trait 用 `#[async_trait(?Send)]`。
//!   D1/R2 绑定是 `Rc`/`Arc<D1Database>`（非 Send），wasm 单线程，future 不要求 Send。
//!
//! 每个 trait 有两个 cfg 门控的孪生定义（签名完全一致，只有 `#[async_trait]` 的 Send 开关不同）。
//! service 层代码两种 feature 下**完全相同**（泛型 `D: Db`，不持 `Arc<dyn Db>`）。

use crate::domain::*;
use crate::error::AppResult;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::sync::Arc;

// ---------- Db（Docker / 自部署：Send） ----------

#[cfg(not(feature = "worker"))]
#[async_trait]
pub trait Db {
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
    // 活动记录（C7）
    async fn list_event_logs(&self, member_id: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<EventLog>, i64)>;
    async fn insert_event_log(&self, e: &EventLog) -> AppResult<()>;
    // 排行榜（C11）
    async fn list_leaderboard(&self, take: i64, skip: i64) -> AppResult<(Vec<(String, String, Option<String>, i64)>, i64)>;
    // 审计
    async fn insert_audit(&self, a: &AuditLog) -> AppResult<()>;
    async fn list_audit(&self, action: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<AuditLog>, i64)>;
    // Team（单队）
    async fn get_team(&self) -> AppResult<Option<Team>>;
    async fn upsert_team(&self, t: &Team) -> AppResult<()>;
    // P1: 积分
    async fn get_points_rule(&self, trigger: &str, scope: &str) -> AppResult<Option<PointsRule>>;
    async fn upsert_points_rule(&self, r: &PointsRule) -> AppResult<()>;
    async fn bump_member_points(&self, member_id: &str, delta: i64) -> AppResult<()>;
    // P1: 统计
    async fn count_members(&self) -> AppResult<i64>;
    async fn count_active_members(&self) -> AppResult<i64>;
    async fn count_saves(&self) -> AppResult<i64>;
    async fn count_total_downloads(&self) -> AppResult<i64>;
    async fn count_events(&self) -> AppResult<i64>;
    async fn count_active_events(&self) -> AppResult<i64>;
    async fn count_announcements(&self) -> AppResult<i64>;
    async fn count_downloads_since(&self, days: i64) -> AppResult<i64>;
    async fn count_logins_since(&self, days: i64) -> AppResult<i64>;
    // P1: RSS
    async fn list_scs_posts(&self, source: Option<&str>, game_version: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<ScsPost>, i64)>;
    async fn upsert_scs_post(&self, p: &ScsPost) -> AppResult<()>;
}

// ---------- Db（Cloudflare worker：?Send 孪生，签名与上完全一致） ----------

#[cfg(feature = "worker")]
#[async_trait(?Send)]
pub trait Db {
    async fn get_user_by_username(&self, username: &str) -> AppResult<Option<User>>;
    async fn insert_user(&self, u: &User) -> AppResult<()>;
    async fn update_user_last_login(&self, id: &str, at: DateTime<Utc>) -> AppResult<()>;
    async fn get_user_by_id(&self, id: &str) -> AppResult<Option<User>>;
    async fn insert_refresh_token(&self, t: &RefreshToken) -> AppResult<()>;
    async fn get_refresh_token(&self, token_hash: &str) -> AppResult<Option<RefreshToken>>;
    async fn revoke_refresh_token(&self, token_hash: &str, replaced_by: Option<&str>) -> AppResult<()>;
    async fn revoke_all_for_user(&self, user_id: &str) -> AppResult<()>;
    async fn list_members(&self, take: i64, skip: i64, search: Option<&str>) -> AppResult<(Vec<Member>, i64)>;
    async fn get_member_by_user(&self, user_id: &str) -> AppResult<Option<Member>>;
    async fn get_member_by_id(&self, id: &str) -> AppResult<Option<Member>>;
    async fn insert_member(&self, m: &Member) -> AppResult<()>;
    async fn update_member(&self, m: &Member) -> AppResult<()>;
    async fn set_member_status(&self, id: &str, status: &str) -> AppResult<()>;
    async fn insert_save(&self, s: &SaveFile) -> AppResult<()>;
    async fn get_save_by_id(&self, id: &str) -> AppResult<Option<SaveFile>>;
    async fn list_saves(&self, game: Option<&str>, version_major: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<SaveFile>, i64)>;
    async fn set_save_published(&self, id: &str, published: bool) -> AppResult<()>;
    async fn bump_save_downloads(&self, id: &str) -> AppResult<()>;
    async fn delete_save(&self, id: &str) -> AppResult<()>;
    async fn insert_save_release(&self, r: &SaveRelease) -> AppResult<()>;
    async fn upsert_dlc_entry(&self, e: &DlcEntry) -> AppResult<()>;
    async fn list_dlc_required(&self, game: &str, major: &str) -> AppResult<Vec<DlcEntry>>;
    async fn insert_dlc_report(&self, r: &DlcReport) -> AppResult<()>;
    async fn upsert_dlc_state(&self, s: &DlcState) -> AppResult<()>;
    async fn get_dlc_state_by_member(&self, member_id: &str) -> AppResult<Option<DlcState>>;
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
    async fn list_event_logs(&self, member_id: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<EventLog>, i64)>;
    async fn insert_event_log(&self, e: &EventLog) -> AppResult<()>;
    async fn list_leaderboard(&self, take: i64, skip: i64) -> AppResult<(Vec<(String, String, Option<String>, i64)>, i64)>;
    async fn insert_audit(&self, a: &AuditLog) -> AppResult<()>;
    async fn list_audit(&self, action: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<AuditLog>, i64)>;
    async fn get_team(&self) -> AppResult<Option<Team>>;
    async fn upsert_team(&self, t: &Team) -> AppResult<()>;
    // P1: 积分
    async fn get_points_rule(&self, trigger: &str, scope: &str) -> AppResult<Option<PointsRule>>;
    async fn upsert_points_rule(&self, r: &PointsRule) -> AppResult<()>;
    async fn bump_member_points(&self, member_id: &str, delta: i64) -> AppResult<()>;
    // P1: 统计
    async fn count_members(&self) -> AppResult<i64>;
    async fn count_active_members(&self) -> AppResult<i64>;
    async fn count_saves(&self) -> AppResult<i64>;
    async fn count_total_downloads(&self) -> AppResult<i64>;
    async fn count_events(&self) -> AppResult<i64>;
    async fn count_active_events(&self) -> AppResult<i64>;
    async fn count_announcements(&self) -> AppResult<i64>;
    async fn count_downloads_since(&self, days: i64) -> AppResult<i64>;
    async fn count_logins_since(&self, days: i64) -> AppResult<i64>;
    // P1: RSS
    async fn list_scs_posts(&self, source: Option<&str>, game_version: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<ScsPost>, i64)>;
    async fn upsert_scs_post(&self, p: &ScsPost) -> AppResult<()>;
}

/// 共享 Db 句柄（service 层已泛型，此处保留兼容）
pub type DbRef = Arc<dyn Db>;

// ---------- Storage ----------

#[cfg(not(feature = "worker"))]
#[async_trait]
pub trait Storage {
    async fn presign_upload(&self, key: &str, size: i64) -> AppResult<String>;
    async fn presign_download(&self, key: &str) -> AppResult<String>;
    async fn local_access(&self, _op: &str, _key: &str, _body: Option<Vec<u8>>) -> AppResult<(Vec<u8>, String, u16)> {
        Err(crate::error::AppError::BadRequest("local access 由具体 Storage 实现（local 磁盘 / R2）".into()))
    }
}

#[cfg(feature = "worker")]
#[async_trait(?Send)]
pub trait Storage {
    async fn presign_upload(&self, key: &str, size: i64) -> AppResult<String>;
    async fn presign_download(&self, key: &str) -> AppResult<String>;
    async fn local_access(&self, _op: &str, _key: &str, _body: Option<Vec<u8>>) -> AppResult<(Vec<u8>, String, u16)> {
        Err(crate::error::AppError::BadRequest("local access 由具体 Storage 实现（local 磁盘 / R2）".into()))
    }
}

pub type StorageRef = Arc<dyn Storage>;

// ---------- Push ----------

#[cfg(not(feature = "worker"))]
#[async_trait]
pub trait Push {
    async fn publish(&self, channel: &str, payload: &str) -> AppResult<()>;
    fn subscriber_count(&self) -> usize { 0 }
}

#[cfg(feature = "worker")]
#[async_trait(?Send)]
pub trait Push {
    async fn publish(&self, channel: &str, payload: &str) -> AppResult<()>;
    fn subscriber_count(&self) -> usize { 0 }
}

pub type PushRef = Arc<dyn Push>;

/// 三个后端的统一句柄（service 层已直接泛型持有具体 `Arc<D>`/`Arc<S>`/`Arc<P>`）。
#[derive(Clone)]
pub struct Backends<D, S, P> {
    pub db: Arc<D>,
    pub storage: Arc<S>,
    pub push: Arc<P>,
}
