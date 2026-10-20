//! 审计日志 DTO（对齐 VTC.Shared.AuditDtos）

use chrono::{DateTime, Utc};
use serde::Serialize;

/// 审计日志（列表项）
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogDto {
    pub id: String,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub ip: String,
    pub detail: String,
    pub at: DateTime<Utc>,
}
