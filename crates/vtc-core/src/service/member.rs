//! 成员服务（S4）：CRUD + 角色 + 状态 + 软删

use crate::backend::Db;
use crate::constants::{audit, status};
use crate::domain::{Member, User};
use crate::dto::member::{MemberCreateDto, MemberDto, MemberUpdateDto};
use crate::error::{AppError, AppResult};
use std::sync::Arc;

/// 成员服务：泛型 over `Db`（只依赖 db），持有具体 `Arc<D>`。
pub struct MemberService<D: Db> {
    pub db: Arc<D>,
    pub jwt: Arc<crate::auth::JwtService>,
}

impl<D: Db> MemberService<D> {
    pub fn new(db: Arc<D>, jwt: Arc<crate::auth::JwtService>) -> Self {
        MemberService { db, jwt }
    }

    fn to_dto(m: &Member, username: &str) -> MemberDto {
        MemberDto {
            id: m.id.clone(),
            user_id: m.user_id.clone(),
            username: username.into(),
            role: m.role.clone(),
            status: m.status.clone(),
            plate: m.plate.clone(),
            vehicle: m.vehicle.clone(),
            level: m.level,
            points: m.points,
            joined_at: Some(m.joined_at),
            note: m.note.clone(),
        }
    }

    /// 列表（管理）
    pub async fn list(&self, take: i64, skip: i64, search: Option<&str>) -> AppResult<(Vec<MemberDto>, i64)> {
        let (members, total) = self.db.list_members(take, skip, search).await?;
        // 批量取 username（P0：逐个，量小）
        let mut dtos = Vec::with_capacity(members.len());
        for m in &members {
            let u = self.db.get_user_by_id(&m.user_id).await?;
            dtos.push(Self::to_dto(m, &u.map(|x| x.username).unwrap_or_default()));
        }
        Ok((dtos, total))
    }

    /// 详情
    pub async fn get(&self, id: &str) -> AppResult<Option<MemberDto>> {
        let m = self.db.get_member_by_id(id).await?;
        let Some(m) = m else { return Ok(None) };
        let u = self.db.get_user_by_id(&m.user_id).await?;
        Ok(Some(Self::to_dto(&m, &u.map(|x| x.username).unwrap_or_default())))
    }

    /// 创建（建账号 + 成员）
    pub async fn create(&self, req: &MemberCreateDto, actor: &str, ip: &str) -> AppResult<MemberDto> {
        if req.username.is_empty() || req.password.is_empty() {
            return Err(AppError::BadRequest("username/password 必填".into()));
        }
        if self.db.get_user_by_username(&req.username).await?.is_some() {
            return Err(AppError::Conflict("用户名已存在".into()));
        }
        let user_id = uuid::Uuid::new_v4().to_string();
        let user = User {
            id: user_id.clone(),
            username: req.username.clone(),
            pass_hash: crate::auth::password::hash_password(&req.password),
            role: req.role.clone(),
            status: status::ACTIVE.into(),
            tmp_player_id: None,
            steam_id: None,
            last_login: None,
            created_at: chrono::Utc::now(),
        };
        self.db.insert_user(&user).await?;
        let member = Member {
            id: uuid::Uuid::new_v4().to_string(),
            user_id,
            role: req.role.clone(),
            status: status::ACTIVE.into(),
            plate: req.plate.clone(),
            vehicle: req.vehicle.clone(),
            level: 1,
            points: 0,
            joined_at: chrono::Utc::now(),
            note: req.note.clone(),
        };
        self.db.insert_member(&member).await?;
        self.audit(actor, audit::MEMBER_CREATE, ip, &req.username).await;
        Ok(Self::to_dto(&member, &req.username))
    }

    /// 更新
    pub async fn update(&self, id: &str, req: &MemberUpdateDto, actor: &str, ip: &str) -> AppResult<MemberDto> {
        let mut m = self
            .db
            .get_member_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("成员不存在".into()))?;
        if let Some(r) = &req.role { m.role = r.clone(); }
        if let Some(s) = &req.status { m.status = s.clone(); }
        if let Some(p) = &req.plate { m.plate = Some(p.clone()); }
        if let Some(v) = &req.vehicle { m.vehicle = Some(v.clone()); }
        if let Some(n) = &req.note { m.note = Some(n.clone()); }
        if let Some(p) = req.points { m.points = p; }
        self.db.update_member(&m).await?;
        self.audit(actor, audit::MEMBER_UPDATE, ip, id).await;
        let u = self.db.get_user_by_id(&m.user_id).await?;
        Ok(Self::to_dto(&m, &u.map(|x| x.username).unwrap_or_default()))
    }

    /// 软删（退出）
    pub async fn delete(&self, id: &str, actor: &str, ip: &str) -> AppResult<()> {
        let m = self
            .db
            .get_member_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("成员不存在".into()))?;
        // 吊销该用户所有 refresh
        self.db.revoke_all_for_user(&m.user_id).await?;
        self.db.set_member_status(id, status::LEFT).await?;
        self.audit(actor, audit::MEMBER_DELETE, ip, id).await;
        Ok(())
    }

    async fn audit(&self, actor: &str, action: &str, ip: &str, target: &str) {
        if let Err(e) = self
            .db
            .insert_audit(&crate::domain::AuditLog {
                id: uuid::Uuid::new_v4().to_string(),
                actor: actor.into(),
                action: action.into(),
                target: target.into(),
                ip: ip.into(),
                detail: String::new(),
                at: chrono::Utc::now(),
            })
            .await
        {
            tracing::warn!(err = %e, "审计写入失败");
        }
    }
}
