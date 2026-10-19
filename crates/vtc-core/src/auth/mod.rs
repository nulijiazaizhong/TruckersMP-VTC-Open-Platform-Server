//! 认证原语：JWT 签发/校验 + PBKDF2 密码哈希（纯逻辑，无后端依赖）

pub mod jwt;
pub mod password;

pub use jwt::{Claims, JwtService};
pub use password::{hash_password, verify_password};
