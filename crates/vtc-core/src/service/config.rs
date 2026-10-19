//! 品牌/配置服务（S3）
//!
//! 泛型化：只依赖 `Db`（具体后端类型），不持 `Arc<dyn Db>`。

use crate::backend::Db;
use crate::dto::config::PublicConfigDto;
use crate::error::AppResult;
use std::sync::Arc;

pub struct ConfigService<D: Db> {
    pub db: Arc<D>,
    /// 更新渠道（来自 UPDATE_CHANNEL 环境变量）
    pub update_channel: String,
}

impl<D: Db> ConfigService<D> {
    pub fn new(db: Arc<D>, update_channel: String) -> Self {
        ConfigService { db, update_channel }
    }

    /// GET /api/v1/config/public（无鉴权）
    pub async fn get_public(&self) -> AppResult<PublicConfigDto> {
        let team = self
            .db
            .get_team()
            .await?
            .ok_or_else(|| crate::error::AppError::NotFound("车队未初始化".into()))?;

        Ok(PublicConfigDto {
            team_name: team.name,
            team_slogan: team.slogan,
            team_logo: team.logo,
            login_bg: team.login_bg,
            login_layout: team.login_layout,
            accent_color: team.accent_color,
            default_lang: team.default_lang,
            features: crate::dto::config::PublicConfigFeatures {
                allow_upload: team.feature_allow_upload,
                steam_binding: false,
                tmp_binding: team.feature_tmp_binding,
                rss_enabled: team.feature_rss_enabled,
            },
            update_channel: self.update_channel.clone(),
        })
    }
}
