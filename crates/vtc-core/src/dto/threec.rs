//! 3C 资源 DTO（S7）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 3C 文件版本（config.cfg / config_local.cfg / controls.sii）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThreeCVersionDto {
    pub id: String,
    pub file_name: String,
    /// 存储 key
    pub storage_key: String,
    pub size: i64,
    pub sha256: String,
    pub note: Option<String>,
    pub published_at: DateTime<Utc>,
    pub published_by: String,
}

/// 上传 3C 文件凭证请求（Admin）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThreeCUploadReq {
    /// 文件名：config.cfg / config_local.cfg / controls.sii
    pub file_name: String,
    pub size: i64,
    pub sha256: String,
    pub note: Option<String>,
}

/// 上传凭证响应
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThreeCCredentialDto {
    pub upload_url: String,
    pub storage_key: String,
    pub expires_in: i64,
}

/// 3C 下载（客户端部署时拉取）
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThreeCDownloadDto {
    pub file_name: String,
    pub download_url: String,
    pub sha256: String,
    pub size: i64,
    pub expires_in: i64,
}
