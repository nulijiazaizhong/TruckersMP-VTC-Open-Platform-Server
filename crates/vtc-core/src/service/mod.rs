//! 服务层：依赖 `Backends`（Db/Storage/Push trait），无具体后端依赖。
//! 全部服务逻辑在此，Docker 与 CF 两个 crate 共享。

pub mod config;
pub mod auth_service;
pub mod member;
pub mod save;
pub mod dlc;
pub mod content;
pub mod audit;
pub mod rate;

pub use config::ConfigService;
pub use auth_service::AuthService;
pub use member::MemberService;
pub use save::SaveService;
pub use dlc::DlcService;
pub use content::ContentService;
pub use audit::AuditService;
pub use rate::RateLimiter;
