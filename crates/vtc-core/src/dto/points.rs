//! 积分 / 活动记录 DTO（S10）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 排行榜条目
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEntry {
    pub rank: i64,
    pub member_id: String,
    pub username: String,
    pub points: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<String>,
}

/// 活动记录条目（C7：个人时间线）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EventLogDto {
    pub id: String,
    pub member_id: String,
    pub event_id: String,
    pub event_name: String,
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mileage: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub income: Option<i64>,
    pub at: DateTime<Utc>,
}

/// 上报活动记录请求
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct EventLogReportDto {
    pub event_id: String,
    #[serde(default = "default_result")]
    pub result: String,
    pub mileage: Option<i64>,
    pub income: Option<i64>,
}
fn default_result() -> String { "completed".into() }

/// 积分规则
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PointsRuleDto {
    pub id: String,
    pub trigger: String,
    pub points: i64,
    pub scope: String,
}

/// 设置积分规则请求（Admin）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PointsRuleSetDto {
    /// 触发器：event_complete
    #[serde(default = "default_trigger")]
    pub trigger: String,
    pub points: i64,
    /// "global" 或具体活动 id
    #[serde(default = "default_scope")]
    pub scope: String,
}
fn default_trigger() -> String { "event_complete".into() }
fn default_scope() -> String { "global".into() }

/// 统计摘要（S11：管理端看板）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub total_members: i64,
    pub active_members: i64,
    pub total_saves: i64,
    pub total_downloads: i64,
    pub total_events: i64,
    pub active_events: i64,
    pub total_announcements: i64,
    /// 近 7 天存档下载总数
    pub downloads_last_7d: i64,
    /// 近 7 天登录人数
    pub logins_last_7d: i64,
}
