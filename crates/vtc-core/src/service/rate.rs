//! 登录限流（同 IP + username 5 次/分钟 → 429，对齐 VTC.Server.LoginRateLimiter）
//! 内存版（单实例）；多实例需 Redis（P1）。
//!
//! ★ wasm 安全（CF Workers / `wasm32-unknown-unknown`，单线程、无 multi-thread）：
//! - **时钟**：用 `std::time::Instant`（经 workspace `chrono` 的 `wasmbind` 特性，在
//!   wasm 上由 JS `performance.now()` 驱动）。**绝不**用 `SystemTime`/`UNIX_EPOCH`——
//!   在 `wasm32-unknown-unknown` 上它们是 `std::sys::pal::wasm::unsupported::time`，
//!   `SystemTime::now()` **直接 panic**「time not implemented on this platform」→
//!   CF `__wbg_new` Critical（本地 wasm 复现已实锤：这是 login 1101 的真正根因）。
//!   `wasmbind` 同时把 `Instant` 也改走 JS 时钟，故此处安全。
//! - **容器**：用 `std::cell::RefCell`（零线程/锁；`lock()`/`write()` 在无 multi-thread
//!   的 wasm 下会走平台锁/线程启动路径 → worker 立即 abort）。`RefCell` 是编译期 `Sync`
//!   （std 对 `RefCell<T:Unsize>` 有 `unsafe impl Sync`），`&self` + `Arc` 签名不用改。
//!
//! 多协程（Docker axum 多线程）下 `RefCell` 若被真正跨线程借用会 panic，但本服务
//! 原本就是「单实例内存限流」，与多 worker 一致性同样依赖 Redis（P1）；单进程内
//! 每个请求各自持有 `Arc<RateLimiter>`，`RefCell` 借用是请求内瞬时的，CF 单线程天然安全。

use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::time::Duration;

// 双目标门控：
// - native（Docker axum 多线程）：`RwLock`，`Send + Sync`，满足 axum state 要求。
// - wasm（CF Workers 单线程）：`RefCell`（零锁，`wasm32-unknown-unknown` 无 multi-thread，
//   `RwLock`/`Mutex` 会走平台锁路径 → abort）。`RefCell` 编译期 `Sync`（std 有
//   `unsafe impl<T: ?Sized> Sync for RefCell<T>`），`&self` 签名不用改。
#[cfg(not(feature = "worker"))]
type MapLock = std::sync::RwLock<HashMap<String, (u64, u32)>>;
#[cfg(feature = "worker")]
type MapLock = std::cell::RefCell<HashMap<String, (u64, u32)>>;

#[cfg(not(feature = "worker"))]
fn map_new() -> MapLock { std::sync::RwLock::new(HashMap::new()) }
#[cfg(feature = "worker")]
fn map_new() -> MapLock { std::cell::RefCell::new(HashMap::new()) }

pub struct RateLimiter {
    /// key → (窗口起始毫秒, 计数)
    map: MapLock,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        RateLimiter { map: map_new() }
    }

    /// ★ 时钟源：**`chrono::Utc::now().timestamp_millis()`**（wasm 下经 workspace `chrono`
    /// 的 `wasmbind` 特性走 JS 时钟，native 下走平台钟，跨目标一致）。
    /// **绝不**用 `std::time::{Instant,SystemTime}`：在 `wasm32-unknown-unknown` 上它们走
    /// `std::sys::pal::wasm::unsupported::time`，`Instant::now()`/`SystemTime::now()` **直接
    /// panic**「time not implemented on this platform」→ CF `__wbg_new` Critical（login 1101
    /// 根因，本地 wasm 复现实锤：`wasmbind` 只改道 `chrono` 内部时钟，**不**改道直接调
    /// `std::time::Instant` 的路径，故限流器必须改走 `chrono`）。
    fn now_ms() -> u64 {
        chrono::Utc::now().timestamp_millis().max(0) as u64
    }

    /// 检查并计数；超限返回 Err(RateLimited)
    pub fn hit(&self, ip: &str, username: &str, limit: u32, window: Duration) -> AppResult<()> {
        let key = format!("{ip}|{username}");
        let now = Self::now_ms();
        let window_ms = window.as_millis() as u64;
        self.hit_locked(&key, now, window_ms, limit)
    }

    #[cfg(not(feature = "worker"))]
    fn hit_locked(&self, key: &str, now: u64, window_ms: u64, limit: u32) -> AppResult<()> {
        let mut map = self.map.write().map_err(|_| anyhow::anyhow!("限流锁不可用"))?;
        Self::count(&mut map, key, now, window_ms, limit)
    }

    #[cfg(feature = "worker")]
    fn hit_locked(&self, key: &str, now: u64, window_ms: u64, limit: u32) -> AppResult<()> {
        let mut map = self.map.borrow_mut();
        Self::count(map, key, now, window_ms, limit)
    }

    fn count(map: &mut HashMap<String, (u64, u32)>, key: &str, now: u64, window_ms: u64, limit: u32) -> AppResult<()> {
        // 惰性清理
        if map.len() > 10_000 {
            map.retain(|_, (t, _)| now.saturating_sub(*t) < window_ms);
        }
        let e = map.entry(key.to_string()).or_insert((now, 0));
        if now.saturating_sub(e.0) > window_ms {
            *e = (now, 0);
        }
        e.1 += 1;
        if e.1 > limit {
            return Err(AppError::RateLimited("登录过于频繁，请稍后再试".into()));
        }
        Ok(())
    }
}
