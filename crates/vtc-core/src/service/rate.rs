//! 登录限流（同 IP + username 5 次/分钟 → 429，对齐 VTC.Server.LoginRateLimiter）
//! 内存版（单实例）；多实例需 Redis（P1）。

use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// ★ wasm 安全：用**毫秒 u64**（`SystemTime`）做时间戳，**不**用 `Instant`。
/// `Instant` 在 wasm-bindgen 下依赖平台时钟源，worker 环境可能不可靠/触发边界异常；
/// `SystemTime` → 毫秒 u64 是纯整数运算，跨目标（Docker/CF）一致。
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Default)]
pub struct RateLimiter {
    /// key → (窗口起始毫秒, 计数)
    /// ★ wasm 安全：用 **Mutex**（单线程下最稳），**不**用 `RwLock`
    ///（`RwLock::write()` 在 wasm-bindgen 运行时行为异常 → CF `__wbg_new`/hang，
    ///  经 v4/v5 二分探针实锤：login 一调 `rate.hit` 就崩，去掉即好）。
    map: Mutex<HashMap<String, (u64, u32)>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        RateLimiter { map: Mutex::new(HashMap::new()) }
    }

    /// 检查并计数；超限返回 Err(RateLimited)
    pub fn hit(&self, ip: &str, username: &str, limit: u32, window: Duration) -> AppResult<()> {
        let key = format!("{ip}|{username}");
        let now = now_ms();
        let window_ms = window.as_millis() as u64;
        // ★ 不 unwrap：PoisonError 用 into_inner 兜住（限流 map 无共享不变量，毒化后照用）。
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
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
