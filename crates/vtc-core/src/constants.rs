//! 常量：角色 / 游戏 / 状态 / JWT / 审计动作 / 错误码（对齐 VTC.Shared.Constants）

/// 角色（RBAC）
pub mod role {
    pub const ADMIN: &str = "Admin";
    pub const MANAGER: &str = "Manager";
    pub const DRIVER: &str = "Driver";
    pub const MEMBER: &str = "Member";
}

/// 游戏
pub mod game {
    pub const ETS2: &str = "ets2";
    pub const ATS: &str = "ats";
    /// ETS2 1.61 地图类 dlc 默认大版本
    pub const DEFAULT_MAJOR: &str = "1.61";
}

/// 实体状态
pub mod status {
    pub const ACTIVE: &str = "Active";
    pub const DISABLED: &str = "Disabled";
    pub const LEFT: &str = "Left";
    pub const PUBLISHED: &str = "published";
    pub const DRAFT: &str = "draft";
    pub const SCHEDULED: &str = "scheduled";
    pub const ENDED: &str = "ended";
    pub const PENDING: &str = "pending";
}

/// JWT
pub mod jwt {
    pub const ISSUER: &str = "vtc-server";
    pub const AUDIENCE: &str = "vtc-client";
    /// access token 寿命（秒）= 2h
    pub const ACCESS_TTL: u64 = 2 * 3600;
    /// refresh token 寿命（天）= 7d
    pub const REFRESH_TTL_DAYS: i64 = 7;
}

/// 审计动作
pub mod audit {
    pub const LOGIN: &str = "auth.login";
    pub const LOGIN_FAIL: &str = "auth.login.fail";
    pub const REFRESH: &str = "auth.refresh";
    pub const SAVE_PUBLISH: &str = "save.publish";
    pub const SAVE_UNPUBLISH: &str = "save.unpublish";
    pub const SAVE_DELETE: &str = "save.delete";
    pub const MEMBER_CREATE: &str = "member.create";
    pub const MEMBER_UPDATE: &str = "member.update";
    pub const MEMBER_DELETE: &str = "member.delete";
    pub const ANNOUNCE_CREATE: &str = "announcement.create";
    pub const EVENT_CREATE: &str = "event.create";
    pub const DLC_DISABLE: &str = "dlc.disable";
    pub const DLC_RESTORE: &str = "dlc.restore";
    pub const STARTUP: &str = "startup.probe";
}

/// 业务错误码（p0-contract 第 1 节，对齐 VTC.Shared.ErrorCodes 1001-1008）
pub mod err {
    pub const BAD_REQUEST: i32 = 1001;
    pub const UNAUTHORIZED: i32 = 1002;
    pub const FORBIDDEN: i32 = 1003;
    pub const NOT_FOUND: i32 = 1004;
    pub const BAD_CREDENTIALS: i32 = 1005;
    pub const RATE_LIMITED: i32 = 1006;
    pub const CONFLICT: i32 = 1007;
    pub const INTERNAL: i32 = 1008;
}
