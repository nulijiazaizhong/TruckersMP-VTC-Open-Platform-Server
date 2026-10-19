//! 公开配置（GET /config/public，客户端启动第一调用）

use serde::{Deserialize, Serialize};

/// 功能开关（对齐 VTC.Shared.PublicConfigDto.Features）
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct PublicConfigFeatures {
    #[serde(default = "default_true")]
    pub allow_upload: bool,
    #[serde(default)]
    pub steam_binding: bool,
    #[serde(default = "default_true")]
    pub tmp_binding: bool,
    #[serde(default = "default_true")]
    pub rss_enabled: bool,
}

fn default_true() -> bool { true }

/// 车队品牌与功能开关
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PublicConfigDto {
    pub team_name: String,
    #[serde(default)]
    pub team_slogan: String,
    pub team_logo: String,
    pub login_bg: String,
    /// left | right | center
    pub login_layout: String,
    pub accent_color: String,
    #[serde(default = "default_lang")]
    pub default_lang: String,
    #[serde(default)]
    pub features: PublicConfigFeatures,
    #[serde(default = "default_channel")]
    pub update_channel: String,
}

fn default_lang() -> String { "zh-CN".into() }
fn default_channel() -> String { "stable".into() }
