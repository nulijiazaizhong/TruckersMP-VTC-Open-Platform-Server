//! 认证 DTO（对齐 VTC.Shared.AuthDtos）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 登录请求
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthLoginDto {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub remember_me: bool,
    #[serde(default)]
    pub tmp_player_id: Option<String>,
    #[serde(default)]
    pub steam_id: Option<String>,
}

/// 刷新令牌请求
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthRefreshDto {
    pub refresh_token: String,
}

/// 用户信息（响应内嵌）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthUserDto {
    pub id: String,
    pub username: String,
    pub role: String,
    pub status: String,
}

/// 成员信息（响应内嵌）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthMemberDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vehicle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<DateTime<Utc>>,
    pub status: String,
}

/// 登录/刷新响应
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthLoginResultDto {
    pub access_token: String,
    pub refresh_token: String,
    /// 秒
    pub expires_in: i64,
    pub user: AuthUserDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<AuthMemberDto>,
}
