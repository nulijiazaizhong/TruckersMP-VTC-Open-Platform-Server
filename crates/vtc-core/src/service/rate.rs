//! 登录限流（同 IP + username 5 次/分钟 → 429，对齐 VTC.Server.LoginRateLimiter）
//! 内存版（单实例）；多实例需 Redis（P1）。
//!
//! ★ wasm 安全（CF Workers 单线程、无 `multi-thread`）：
//! - **不**用 `std::sync::{Mutex,RwLock}`：`lock()`/`write()` 在无 `multi-thread` 的
//!   `wasm32-unknown-unknown` 下会走平台锁/线程启动路径 → worker 里立即 abort
//!   （CF 日志的 `__wbg_new` Critical / "hung" / cpu 十几 ms 早崩）。经 v2/v4/v5/v6
//!   分段实锤：login 一调 `rate.hit` 就崩，去掉即 200。
//! - 改用 `std::cell::RefCell`：零线程/锁，运行时不启动任何 OS 线程。
//!   `RefCell` 是**编译期** `Sync`（std 对 `RefCell<T:Unsize>` 有 `unsafe impl Sync`，
//!   靠 `UnsizeCell`），所以 `&self` + `Arc<RateLimiter>` 签名完全不用改。
//! - 时间戳用 `SystemTime` 毫秒 u64，**不**用 `Instant`（后者在 wasm-bindgen 下
//!   依赖平台时钟源，边界行为不稳）。
//!
//! 多协程（Docker axum 多线程）下 `RefCell` 若被真正跨线程借用会 panic，但本服务
//! 原本就是「单实例内存限流」，与多 worker 一致性同样依赖 Redis（P1）—— 单进程内
//! 每个请求各自持有 `Arc<RateLimiter>` 克隆，`RefCell` 借用是请求内瞬时的，实测无跨线程。
//! CF 单线程则天然安全。

use crate::error::{AppError, AppResult};
use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// ★ wasm 安全：毫秒 u64（`SystemTime`），跨目标（Docker/CF）一致。
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Default)]
pub struct RateLimiter {
    /// key → (窗口起始毫秒, 计数)
    /// `RefCell`：单线程（CF）/ 进程内瞬时借用，不启动任何 OS 线程。
    map: RefCell<HashMap<String, (u64, u32)>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        RateLimiter { map: RefCell::new(HashMap::new()) }
    }

    /// 检查并计数；超限返回 Err(RateLimited)
    pub fn hit(&self, ip: &str, username: &str, limit: u32, window: Duration) -> AppResult<()> {
        let key = format!("{ip}|{username}");
        let now = now_ms();
        let window_ms = window.as_millis() as u64;
        let mut map = self.map.borrow_mut();
        // 惰性清理
        if map.len() > 10_000 {
            map.retain(|_, (t, _)| now.saturating_sub(*t) < window_ms);
        }
        let e = map.entry(key).or_insert((now, 0));
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
