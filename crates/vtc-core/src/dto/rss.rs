//! RSS 资讯 DTO（S9）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// RSS 条目（客户端展示）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RssItemDto {
    pub id: String,
    pub source: String,
    pub title: String,
    pub url: String,
    pub date: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_version_tag: Option<String>,
}

/// RSS 源配置（管理端可改）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct RssSourceConfig {
    /// 标签：ets2 / ats
    pub label: String,
    pub feed_url: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}
fn default_enabled() -> bool { true }
