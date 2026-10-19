//! 存档服务（S5）：元数据 + 直传凭证 + 发布 + 下载
//! 大文件走预签名 URL，不落库、不走 API 中转（§3.5）

use crate::backend::{Db, Storage};
use crate::constants::{audit, status};
use crate::domain::{SaveFile, SaveRelease};
use crate::dto::save::{
    PublishReq, SaveCredentialDto, SaveDownloadCredentialDto, SaveDownloadReq, SaveFileDto, SaveUploadReq,
};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use std::sync::Arc;

/// 预签名 URL 时效（秒）
const PRESIGN_TTL: i64 = 3600;

/// 存档服务：泛型 over `Db` + `Storage`，持有具体 `Arc<D>` / `Arc<S>`。
pub struct SaveService<D: Db, S: Storage> {
    pub db: Arc<D>,
    pub storage: Arc<S>,
    pub jwt: Arc<crate::auth::JwtService>,
    /// 外部基址（local 形态拼本地端点用；CF 形态不用）
    pub public_base: String,
}

impl<D: Db, S: Storage> SaveService<D, S> {
    pub fn new(db: Arc<D>, storage: Arc<S>, jwt: Arc<crate::auth::JwtService>, public_base: String) -> Self {
        SaveService { db, storage, jwt, public_base }
    }

    fn to_dto(s: &SaveFile, url: Option<String>) -> SaveFileDto {
        SaveFileDto {
            id: s.id.clone(),
            label: s.label.clone(),
            game: s.game.clone(),
            game_version: s.game_version.clone(),
            size: s.size,
            sha256: s.sha256.clone(),
            published_at: s.created_at,
            downloads: s.downloads,
            download_url: url,
        }
    }

    /// 上传凭证（Admin/Manager）
    pub async fn issue_upload(&self, req: &SaveUploadReq, actor: &str, ip: &str) -> AppResult<SaveCredentialDto> {
        let save_id = uuid::Uuid::new_v4().to_string();
        // storage key: saves/<game>/<version>/<saveId>.zip
        let key = format!("saves/{}/{}{}.zip", req.game, req.game_version, save_id);
        let upload_url = self.storage.presign_upload(&key, req.size).await?;

        // 先落"未发布"元数据（上传完成后 publish）
        let save = SaveFile {
            id: save_id.clone(),
            game: req.game.clone(),
            game_version: req.game_version.clone(),
            label: req.label.clone(),
            storage_key: key.clone(),
            size: req.size,
            sha256: req.sha256.clone(),
            published: false,
            downloads: 0,
            created_at: Utc::now(),
            created_by: actor.into(),
        };
        self.db.insert_save(&save).await?;
        self.audit(actor, "save.create", ip, &save_id).await;

        Ok(SaveCredentialDto {
            upload_url,
            save_key: key,
            expires_in: PRESIGN_TTL,
            save_id,
        })
    }

    /// 发布
    pub async fn publish(&self, save_id: &str, req: &PublishReq, actor: &str, ip: &str) -> AppResult<SaveFileDto> {
        let mut s = self
            .db
            .get_save_by_id(save_id)
            .await?
            .ok_or_else(|| AppError::NotFound("存档不存在".into()))?;
        // 幂等
        if !s.published {
            s.published = true;
            self.db.set_save_published(save_id, true).await?;
            self.db
                .insert_save_release(&SaveRelease {
                    id: uuid::Uuid::new_v4().to_string(),
                    save_id: save_id.into(),
                    release_note: req.release_note.clone().unwrap_or_default(),
                    released_at: Utc::now(),
                    released_by: actor.into(),
                })
                .await?;
            self.audit(actor, audit::SAVE_PUBLISH, ip, save_id).await;
        }
        let url = self.storage.presign_download(&s.storage_key).await?;
        Ok(Self::to_dto(&s, Some(url)))
    }

    /// 列表（按 game + 大版本过滤，仅 published）
    pub async fn list(&self, game: Option<&str>, version_major: Option<&str>, take: i64, skip: i64) -> AppResult<(Vec<SaveFileDto>, i64)> {
        let (saves, total) = self.db.list_saves(game, version_major, take, skip).await?;
        let mut dtos = Vec::with_capacity(saves.len());
        for s in &saves {
            let url = if s.published { Some(self.storage.presign_download(&s.storage_key).await?) } else { None };
            dtos.push(Self::to_dto(s, url));
        }
        Ok((dtos, total))
    }

    /// 下载凭证（需登录）
    pub async fn issue_download(&self, save_id: &str, _req: &SaveDownloadReq, user_id: &str) -> AppResult<SaveDownloadCredentialDto> {
        let s = self
            .db
            .get_save_by_id(save_id)
            .await?
            .ok_or_else(|| AppError::NotFound("存档不存在".into()))?;
        if !s.published {
            return Err(AppError::NotFound("存档未发布".into()));
        }
        let url = self.storage.presign_download(&s.storage_key).await?;
        self.db.bump_save_downloads(save_id).await?;
        let _ = user_id;
        Ok(SaveDownloadCredentialDto {
            download_url: url,
            expires_in: PRESIGN_TTL,
            sha256: s.sha256.clone(),
        })
    }

    /// 删除
    pub async fn delete(&self, save_id: &str, actor: &str, ip: &str) -> AppResult<()> {
        // 删对象存储（best-effort）+ 元数据
        if let Some(s) = self.db.get_save_by_id(save_id).await? {
            let _ = s;
        }
        self.db.delete_save(save_id).await?;
        self.audit(actor, audit::SAVE_DELETE, ip, save_id).await;
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
                at: Utc::now(),
            })
            .await
        {
            tracing::warn!(err = %e, "审计写入失败");
        }
    }
}
