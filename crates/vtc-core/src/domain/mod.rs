//! Domain 实体（对齐 VTC.Server.Domain，13 个）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 车队（单队，多租户 = 独立实例）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub slogan: String,
    #[serde(default)]
    pub slug: String,
    pub logo: String,
    pub login_bg: String,
    #[serde(default = "default_layout")]
    pub login_layout: String,
    #[serde(default = "default_accent")]
    pub accent_color: String,
    #[serde(default = "default_lang")]
    pub default_lang: String,
    #[serde(default = "default_true")]
    pub feature_allow_upload: bool,
    #[serde(default = "default_true")]
    pub feature_tmp_binding: bool,
    #[serde(default = "default_true")]
    pub feature_rss_enabled: bool,
    pub created_at: DateTime<Utc>,
}
fn default_layout() -> String { "left".into() }
fn default_accent() -> String { "#1E88E5".into() }
fn default_lang() -> String { "zh-CN".into() }
fn default_true() -> bool { true }

/// 用户
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    /// PBKDF2 哈希（$pbkdf2$iters$salt$hash）
    pub pass_hash: String,
    pub role: String,
    pub status: String,
    pub tmp_player_id: Option<String>,
    pub steam_id: Option<String>,
    pub last_login: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// 刷新令牌（轮换 + 重放吊销链）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RefreshToken {
    pub id: String,
    pub token_hash: String,
    pub user_id: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub replaced_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 成员
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub id: String,
    pub user_id: String,
    pub role: String,
    pub status: String,
    pub plate: Option<String>,
    pub vehicle: Option<String>,
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub points: i64,
    pub joined_at: DateTime<Utc>,
    pub note: Option<String>,
}

/// 存档文件（版本化元数据）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveFile {
    pub id: String,
    pub game: String,
    pub game_version: String,
    pub label: String,
    pub storage_key: String,
    pub size: i64,
    pub sha256: String,
    /// 0=未发布 1=已发布
    pub published: bool,
    pub downloads: i64,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

/// 存档发布记录
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveRelease {
    pub id: String,
    pub save_id: String,
    #[serde(default)]
    pub release_note: String,
    pub released_at: DateTime<Utc>,
    pub released_by: String,
}

/// DLC 条目
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DlcEntry {
    pub id: String,
    pub game: String,
    pub major: String,
    pub dlc_id: String,
    pub display_name: String,
    pub is_map: bool,
    pub required: bool,
    pub purchase_url: Option<String>,
}

/// DLC 状态上报
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DlcReport {
    pub id: String,
    pub member_id: String,
    pub game: String,
    /// JSON 数组字符串
    pub missing: String,
    pub installed: String,
    pub reported_at: DateTime<Utc>,
}

/// DLC 禁用状态快照
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DlcState {
    pub id: String,
    pub member_id: String,
    pub event_disabled: bool,
    /// JSON 数组字符串
    pub disabled_list: String,
    /// 备份目录
    pub snapshot_path: String,
    pub snapshot_at: DateTime<Utc>,
    pub restored_at: Option<DateTime<Utc>>,
}

/// 公告
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Announcement {
    pub id: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub published_at: Option<DateTime<Utc>>,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub pinned: bool,
    pub author_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 活动
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub name: String,
    pub etype: String,
    pub activity_at: DateTime<Utc>,
    pub depart_at: Option<DateTime<Utc>>,
    pub server: String,
    pub navigation: String,
    pub image_key: Option<String>,
    pub save_id: Option<String>,
    pub status: String,
    pub requires_map_dlc: Option<String>,
    pub created_by: Option<String>,
}

/// 审计日志
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuditLog {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub ip: String,
    pub detail: String,
    pub at: DateTime<Utc>,
}
