//! 登录限流（同 IP + username 5 次/分钟 → 429，对齐 VTC.Server.LoginRateLimiter）
//! 内存版（单实例）；多实例需 Redis（P1）。

use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct RateLimiter {
    /// key → (窗口起始, 计数)
    map: RwLock<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new() -> Self { RateLimiter { map: RwLock::new(HashMap::new()) } }

    /// 检查并计数；超限返回 Err(RateLimited)
    pub fn hit(&self, ip: &str, username: &str, limit: u32, window: Duration) -> AppResult<()> {
        let key = format!("{ip}|{username}");
        let now = Instant::now();
        // ★ 用 unwrap_or_else 兜住 PoisonError：**不** unwrap。
        // 之前某请求 panic（如 console_log 触发的 CF hang）会**毒化**这个 RwLock，
        // 之后每个 login 的 write().unwrap() 都二次 panic → CF 报 __wbg_new 异常。
        // 限流 map 是纯内存无共享不变量，毒化后继续用（into_inner）即可，不必 panic。
        let mut map = self.map.write().unwrap_or_else(|e| e.into_inner());
        // 惰性清理
        if map.len() > 10_000 {
            map.retain(|_, (t, _)| now.duration_since(*t) < window);
        }
        let e = map.entry(key).or_insert((now, 0));
        if now.duration_since(e.0) > window {
            *e = (now, 0);
        }
        e.1 += 1;
        if e.1 > limit {
            return Err(AppError::RateLimited("登录过于频繁，请稍后再试".into()));
        }
        Ok(())
    }
}

#[allow(dead_code)]
fn _lock() -> &'static Mutex<()> { static M: Mutex<()> = Mutex::new(()); &M }
