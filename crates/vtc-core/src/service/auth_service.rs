//! 认证服务（S1）：登录 / 刷新 / 会话 / 审计 / 限流

use crate::auth::{jwt::Claims, JwtService};
use crate::backend::Db;
use crate::constants::{audit, role, status};
use crate::domain::RefreshToken;
use crate::dto::auth::{AuthLoginDto, AuthLoginResultDto, AuthMemberDto, AuthUserDto};
use crate::error::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::time::Duration;

/// 认证服务：泛型 over `Db`（只依赖 db），持有具体 `Arc<D>`。
pub struct AuthService<D: Db> {
    pub db: Arc<D>,
    pub jwt: Arc<JwtService>,
    pub rate: Arc<crate::service::rate::RateLimiter>,
    /// 是否启用 TMP / Steam 身份绑定校验
    pub require_tmp: bool,
    pub require_steam: bool,
}

impl<D: Db> AuthService<D> {
    pub fn new(
        db: Arc<D>,
        jwt: Arc<JwtService>,
        rate: Arc<crate::service::rate::RateLimiter>,
        require_tmp: bool,
        require_steam: bool,
    ) -> Self {
        AuthService { db, jwt, rate, require_tmp, require_steam }
    }

    fn hash_token(t: &str) -> String {
        let mut h = Sha256::new();
        h.update(t.as_bytes());
        hex::encode(h.finalize())
    }

    /// 登录
    pub async fn login(&self, req: &AuthLoginDto, ip: &str) -> AppResult<AuthLoginResultDto> {
        // 限流
        if let Err(e) = self
            .rate
            .hit(ip, &req.username, 5, Duration::from_secs(60))
        {
            // 审计失败
            let _ = self
                .db
                .insert_audit(&crate::domain::AuditLog {
                    id: uuid::Uuid::new_v4().to_string(),
                    actor: req.username.clone(),
                    action: audit::LOGIN_FAIL.into(),
                    target: String::new(),
                    ip: ip.into(),
                    detail: "rate_limited".into(),
                    at: chrono::Utc::now(),
                })
                .await;
            return Err(e);
        }

        let user = self
            .db
            .get_user_by_username(&req.username)
            .await?
            .ok_or_else(|| AppError::BadCredentials("用户名或密码错误".into()))?;

        if user.status != status::ACTIVE {
            self.audit(&user.username, audit::LOGIN_FAIL, ip, "disabled").await;
            return Err(AppError::Forbidden("账号已禁用".into()));
        }

        if !crate::auth::password::verify_password(&req.password, &user.pass_hash)? {
            self.audit(&user.username, audit::LOGIN_FAIL, ip, "bad_password").await;
            return Err(AppError::BadCredentials("用户名或密码错误".into()));
        }

        // 身份绑定校验（AUTH_MODE）
        if self.require_tmp && req.tmp_player_id.is_none() {
            self.audit(&user.username, audit::LOGIN_FAIL, ip, "tmp_required").await;
            return Err(AppError::Forbidden("需要 TM:MP 身份绑定".into()));
        }
        if self.require_steam && req.steam_id.is_none() {
            self.audit(&user.username, audit::LOGIN_FAIL, ip, "steam_required").await;
            return Err(AppError::Forbidden("需要 Steam 身份绑定".into()));
        }

        // 发 access + refresh
        let access = self
            .jwt
            .issue_access(&user.id, &user.username, &user.role, &user.status)?;
        let refresh = self.jwt.issue_refresh(&user.id, &user.username)?;

        // 存 refresh（旧的同用户 refresh 全部吊销 → 轮换）
        self.db.revoke_all_for_user(&user.id).await?;
        self.db
            .insert_refresh_token(&RefreshToken {
                id: uuid::Uuid::new_v4().to_string(),
                token_hash: Self::hash_token(&refresh),
                user_id: user.id.clone(),
                expires_at: chrono::Utc::now()
                    + chrono::Duration::days(crate::constants::jwt::REFRESH_TTL_DAYS),
                revoked_at: None,
                replaced_by: None,
                created_at: chrono::Utc::now(),
            })
            .await?;

        // 更新 last_login + 审计
        self.db
            .update_user_last_login(&user.id, chrono::Utc::now())
            .await?;
        self.audit(&user.username, audit::LOGIN, ip, "ok").await;

        // 组装响应
        let member = self.db.get_member_by_user(&user.id).await?;
        let result = AuthLoginResultDto {
            access_token: access,
            refresh_token: refresh,
            expires_in: crate::auth::jwt::JwtService::access_ttl(),
            user: AuthUserDto {
                id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
                status: user.status.clone(),
            },
            member: member.map(|m| AuthMemberDto {
                plate: m.plate,
                vehicle: m.vehicle,
                joined_at: Some(m.joined_at),
                status: m.status,
            }),
        };
        Ok(result)
    }

    /// 刷新（轮换 + 重放吊销）
    pub async fn refresh(&self, refresh_token: &str, ip: &str) -> AppResult<AuthLoginResultDto> {
        let thash = Self::hash_token(refresh_token);
        let stored = self
            .db
            .get_refresh_token(&thash)
            .await?
            .ok_or_else(|| AppError::Unauthorized("refresh token 无效".into()))?;

        if stored.revoked_at.is_some() {
            // 重放攻击：吊销该用户所有
            self.db.revoke_all_for_user(&stored.user_id).await?;
            return Err(AppError::Unauthorized("refresh token 已被吊销（疑似重放）".into()));
        }
        if stored.expires_at < chrono::Utc::now() {
            return Err(AppError::Unauthorized("refresh token 已过期".into()));
        }

        let user = self
            .db
            .get_user_by_id(&stored.user_id)
            .await?
            .ok_or_else(|| AppError::Unauthorized("用户不存在".into()))?;

        let access = self
            .jwt
            .issue_access(&user.id, &user.username, &user.role, &user.status)?;
        let new_refresh = self.jwt.issue_refresh(&user.id, &user.username)?;
        let new_hash = Self::hash_token(&new_refresh);

        // 轮换：旧吊销 + 指向新
        self.db
            .revoke_refresh_token(&thash, Some(&new_hash))
            .await?;
        self.db
            .insert_refresh_token(&RefreshToken {
                id: uuid::Uuid::new_v4().to_string(),
                token_hash: new_hash,
                user_id: user.id.clone(),
                expires_at: chrono::Utc::now() + chrono::Duration::days(crate::constants::jwt::REFRESH_TTL_DAYS),
                revoked_at: None,
                replaced_by: None,
                created_at: chrono::Utc::now(),
            })
            .await?;
        self.audit(&user.username, audit::REFRESH, ip, "ok").await;

        let member = self.db.get_member_by_user(&user.id).await?;
        Ok(AuthLoginResultDto {
            access_token: access,
            refresh_token: new_refresh,
            expires_in: crate::auth::jwt::JwtService::access_ttl(),
            user: AuthUserDto {
                id: user.id.clone(),
                username: user.username.clone(),
                role: user.role.clone(),
                status: user.status.clone(),
            },
            member: member.map(|m| AuthMemberDto {
                plate: m.plate,
                vehicle: m.vehicle,
                joined_at: Some(m.joined_at),
                status: m.status,
            }),
        })
    }

    /// 从 Bearer token 解析身份（供路由鉴权用）
    pub fn resolve(&self, token: &str) -> AppResult<Claims> {
        self.jwt.validate(token)
    }

    async fn audit(&self, actor: &str, action: &str, ip: &str, detail: &str) {
        if let Err(e) = self
            .db
            .insert_audit(&crate::domain::AuditLog {
                id: uuid::Uuid::new_v4().to_string(),
                actor: actor.into(),
                action: action.into(),
                target: String::new(),
                ip: ip.into(),
                detail: detail.into(),
                at: chrono::Utc::now(),
            })
            .await
        {
            tracing::warn!(err = %e, "审计写入失败");
        }
    }
}

/// RBAC 辅助：判断角色是否属于管理组
pub fn is_admin(role: &str) -> bool {
    role == role::ADMIN || role == role::MANAGER
}
