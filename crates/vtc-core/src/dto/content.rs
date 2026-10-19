//! 公告 / 活动 DTO（对齐 VTC.Shared.EventAnnouncementDtos）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------- 公告 ----------

/// 公告
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementDto {
    pub id: String,
    pub title: String,
    pub body: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 创建公告
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementCreateDto {
    pub title: String,
    pub body: String,
    #[serde(default = "default_published")]
    pub status: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<DateTime<Utc>>,
}
fn default_published() -> String { "published".into() }

/// 更新公告
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnnouncementUpdateDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<DateTime<Utc>>,
}

// ---------- 活动 ----------

/// 活动
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventDto {
    pub id: String,
    pub name: String,
    pub etype: String,
    pub activity_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depart_at: Option<DateTime<Utc>>,
    pub server: String,
    pub navigation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_key: Option<String>,
    /// 关联存档 id
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_id: Option<String>,
    pub status: String,
    /// 需要地图类 DLC 的豁免包（逗号分隔）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_map_dlc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
}

/// 创建活动
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventCreateDto {
    pub name: String,
    #[serde(default = "default_etype")]
    pub etype: String,
    pub activity_at: DateTime<Utc>,
    pub depart_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub server: String,
    #[serde(default = "default_nav")]
    pub navigation: String,
    pub image_key: Option<String>,
    pub save_id: Option<String>,
    #[serde(default = "default_ended")]
    pub status: String,
    pub requires_map_dlc: Option<String>,
}
fn default_etype() -> String { "activity".into() }
fn default_nav() -> String { "跟随头车".into() }
fn default_ended() -> String { "scheduled".into() }

/// 更新活动
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventUpdateDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etype: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub depart_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub navigation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_map_dlc: Option<String>,
}
