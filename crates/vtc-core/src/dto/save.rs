//! 存档 DTO（对齐 VTC.Shared.SaveDtos）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 已发布存档（列表项）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveFileDto {
    pub id: String,
    pub label: String,
    pub game: String,
    pub game_version: String,
    /// 字节
    pub size: i64,
    pub sha256: String,
    pub published_at: DateTime<Utc>,
    #[serde(default)]
    pub downloads: i64,
    /// 预签名下载 URL（列表里给短时效）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
}

/// 上传凭证请求（管理）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SaveUploadReq {
    #[serde(default = "default_game")]
    pub game: String,
    pub label: String,
    #[serde(default = "default_version")]
    pub game_version: String,
    pub size: i64,
    pub sha256: String,
}
fn default_game() -> String { "ets2".into() }
fn default_version() -> String { "1.61".into() }

/// 上传凭证响应
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveCredentialDto {
    pub upload_url: String,
    pub save_key: String,
    pub expires_in: i64,
    pub save_id: String,
}

/// 下载凭证请求
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveDownloadReq {
    pub game: Option<String>,
}

/// 下载凭证响应
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveDownloadCredentialDto {
    pub download_url: String,
    pub expires_in: i64,
    pub sha256: String,
}

/// 发布请求
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublishReq {
    pub release_note: Option<String>,
}
