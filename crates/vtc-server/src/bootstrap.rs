//! 启动配置 + 种子（Team / admin / 默认车队）

use anyhow::Result;
use std::sync::Arc;
use vtc_core::backend::Db;
use vtc_core::constants::role;
use vtc_core::domain::{Team, User};
use vtc_core::error::AppError;

#[derive(Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub db_path: String,
    pub storage_dir: String,
    pub public_base: String,
    pub jwt_secret: String,
    pub admin_username: String,
    pub admin_password: String,
    pub team_name: String,
    pub update_channel: String,
    pub map_dlc_dir: String,
    pub require_tmp: bool,
    pub require_steam: bool,
}

impl ServerConfig {
    pub fn from_env() -> Result<Self> {
        Ok(ServerConfig {
            port: std::env::var("PORT").ok().and_then(|s| s.parse().ok()).unwrap_or(8731),
            db_path: std::env::var("DB_PATH").unwrap_or_else(|_| "vtc.db".into()),
            storage_dir: std::env::var("STORAGE_DIR").unwrap_or_else(|_| "./storage".into()),
            public_base: std::env::var("PUBLIC_BASE").unwrap_or_else(|_| "http://127.0.0.1:8731".into()),
            jwt_secret: std::env::var("JWT_SECRET").unwrap_or_else(|_| {
                tracing::warn!("JWT_SECRET 未设置，使用开发默认值（生产必须设置 ≥32 字符）");
                "dev-insecure-secret-change-me-0123456789".into()
            }),
            admin_username: std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".into()),
            admin_password: std::env::var("ADMIN_INITIAL_PASSWORD").unwrap_or_else(|_| "ChangeMe123!".into()),
            team_name: std::env::var("TEAM_NAME").unwrap_or_else(|_| "默认车队".into()),
            update_channel: std::env::var("UPDATE_CHANNEL").unwrap_or_else(|_| "stable".into()),
            map_dlc_dir: std::env::var("MAP_DLC_DIR").unwrap_or_else(|_| "../../map-dlc".into()),
            require_tmp: std::env::var("AUTH_REQUIRE_TMP").ok().as_deref() == Some("true"),
            require_steam: std::env::var("AUTH_REQUIRE_STEAM").ok().as_deref() == Some("true"),
        })
    }
}

/// 种子：Team（若不存在）+ admin 用户（若不存在）
///
/// 参数是**具体后端** `Arc<SqliteDb>`（不是 `Arc<dyn Db>`）：直接对 `SqliteDb` 调 `Db` 方法，
/// 让编译器用 `SqliteDb` 的 `?Send` impl 方法（Send future）。**不**走 `Arc<dyn Db>`——因为
/// `?Send` trait 的 `dyn Db` 是非 Send 的，在 `async fn` 里 `.await` 它的 future 会触发
/// async-trait 的"全局 Send 规则"反向污染 `SqliteDb: Db` 的 future（去掉 Send bound），
/// 让 axum handler 判非 Send。直接调具体类型绕开这个 trap。
pub async fn seed(db: &Arc<crate::db::SqliteDb>, cfg: &ServerConfig) -> Result<()> {
    use vtc_core::backend::Db as _; // 让 `SqliteDb` 的 `Db` trait 方法可见

    // Team
    if db.get_team().await?.is_none() {
        let team = Team {
            id: "default".into(),
            name: cfg.team_name.clone(),
            slogan: "松散，自由的行驶、玩耍".into(),
            slug: "default".into(),
            logo: "pack://app-assets/brand/logo.png".into(),
            login_bg: "pack://app-assets/brand/login_bg.jpg".into(),
            login_layout: "left".into(),
            accent_color: "#1E88E5".into(),
            default_lang: "zh-CN".into(),
            feature_allow_upload: true,
            feature_tmp_binding: true,
            feature_rss_enabled: true,
            created_at: chrono::Utc::now(),
        };
        db.upsert_team(&team).await?;
        tracing::info!("已种子 Team：{}", team.name);
    }

    // admin
    if db.get_user_by_username(&cfg.admin_username).await?.is_none() {
        let admin = User {
            id: uuid::Uuid::new_v4().to_string(),
            username: cfg.admin_username.clone(),
            pass_hash: vtc_core::auth::password::hash_password(&cfg.admin_password),
            role: role::ADMIN.into(),
            status: "Active".into(),
            tmp_player_id: None,
            steam_id: None,
            last_login: None,
            created_at: chrono::Utc::now(),
        };
        db.insert_user(&admin).await?;
        // 建对应 member
        db.insert_member(&vtc_core::domain::Member {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: admin.id.clone(),
            role: role::ADMIN.into(),
            status: "Active".into(),
            plate: None,
            vehicle: None,
            level: 1,
            points: 0,
            joined_at: chrono::Utc::now(),
            note: Some("初始管理员".into()),
        }).await?;
        tracing::info!("已种子 admin：{}（密码 ADMIN_INITIAL_PASSWORD）", admin.username);
    }

    Ok(())
}

#[allow(dead_code)]
fn _err() -> AppError { AppError::NotFound("x".into()) }
