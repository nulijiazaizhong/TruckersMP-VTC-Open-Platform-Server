//! 审计日志服务（S1/S8 横切）：列表 + 按 action 过滤

use crate::backend::Db;
use crate::dto::audit::AuditLogDto;
use crate::error::AppResult;
use std::sync::Arc;

/// 审计服务：泛型 over `Db`（只依赖 db），持有具体 `Arc<D>`。
pub struct AuditService<D: Db> {
    pub db: Arc<D>,
}

impl<D: Db> AuditService<D> {
    pub fn new(db: Arc<D>) -> Self {
        AuditService { db }
    }

    /// 审计日志列表（Admin）。`action` = 可选过滤（login / save.publish / ...）。
    pub async fn list(&self, action: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<AuditLogDto>, i64)> {
        let (logs, total) = self.db.list_audit(action, take, skip).await?;
        Ok((
            logs
                .into_iter()
                .map(|l| AuditLogDto {
                    id: l.id,
                    actor: l.actor,
                    action: l.action,
                    target: l.target,
                    ip: l.ip,
                    detail: l.detail,
                    at: l.at,
                })
                .collect(),
            total,
        ))
    }
}
