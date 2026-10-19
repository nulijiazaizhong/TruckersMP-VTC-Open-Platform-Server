//! 成员 DTO（对齐 VTC.Shared.MemberDtos）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 成员（列表/详情）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemberDto {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub role: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<String>,
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub points: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 创建成员（同时建账号）
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemberCreateDto {
    pub username: String,
    pub password: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

fn default_role() -> String { "Member".into() }

/// 更新成员
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemberUpdateDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<i64>,
}
