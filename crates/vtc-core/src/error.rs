//! 统一错误类型：业务错误（含 HTTP 状态 + 业务码）+ 通用 AppError

use thiserror::Error;

/// 业务错误：可序列化为 `{code, message, details}`（p0-contract §1）
#[derive(Error, Debug)]
pub enum AppError {
    #[error("{0}")]
    BadRequest(String),

    #[error("未认证：{0}")]
    Unauthorized(String),

    #[error("无权访问：{0}")]
    Forbidden(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    BadCredentials(String),

    #[error("{0}")]
    RateLimited(String),

    #[error("{0}")]
    Conflict(String),

    #[error("服务器内部错误")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn status(&self) -> u16 {
        match self {
            AppError::BadRequest(_) => 400,
            AppError::Unauthorized(_) => 401,
            AppError::Forbidden(_) => 403,
            AppError::NotFound(_) => 404,
            AppError::BadCredentials(_) => 401,
            AppError::RateLimited(_) => 429,
            AppError::Conflict(_) => 409,
            AppError::Internal(_) => 500,
        }
    }

    pub fn biz_code(&self) -> i32 {
        match self {
            AppError::BadRequest(_) => 1001,
            AppError::Unauthorized(_) => 1002,
            AppError::Forbidden(_) => 1003,
            AppError::NotFound(_) => 1004,
            AppError::BadCredentials(_) => 1005,
            AppError::RateLimited(_) => 1006,
            AppError::Conflict(_) => 1007,
            AppError::Internal(_) => 1008,
        }
    }

    /// 统一错误响应体（camelCase，对齐 VTC.Shared.ApiError）。
    /// `Internal` 的 anyhow 详情放进 `details`（其余错误保持 details=null），
    /// 便于线上诊断（如 D1 建表/seed 失败时看到真实错误）。
    pub fn to_json(&self) -> serde_json::Value {
        let details = match self {
            AppError::Internal(e) => serde_json::Value::String(e.to_string()),
            _ => serde_json::Value::Null,
        };
        serde_json::json!({
            "code": self.biz_code(),
            "message": self.to_string(),
            "details": details,
        })
    }
}

/// 应用结果类型
pub type AppResult<T> = Result<T, AppError>;
