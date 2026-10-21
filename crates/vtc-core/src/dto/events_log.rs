//! 活动记录 + 排行榜 DTO（C7 / C11，对齐 VTC.Shared.Dtos）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 活动记录（时间线视图）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventLogDto {
    pub id: String,
    pub member_id: String,
    pub event_id: String,
    /// 活动名称（JOIN 后填充）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mileage: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub income: Option<i64>,
    pub at: DateTime<Utc>,
}

/// 创建活动记录
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventLogCreateDto {
    pub event_id: String,
    #[serde(default = "default_result")]
    pub result: String,
    pub mileage: Option<i64>,
    pub income: Option<i64>,
}
fn default_result() -> String { "completed".into() }

/// 排行榜条目
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEntryDto {
    pub rank: i64,
    pub member_id: String,
    pub username: String,
    pub plate: Option<String>,
    pub points: i64,
}

/// 排行榜响应
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardDto {
    pub items: Vec<LeaderboardEntryDto>,
    pub total: i64,
}
