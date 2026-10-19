//! 公共 DTO（ApiError 等）

use serde::Serialize;

/// 统一错误响应（p0-contract §1，对齐 VTC.Shared.ApiError）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
