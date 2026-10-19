//! 推送（Workers 形态）：P0 存根
//!
//! worker 无长连 WebSocket 服务（WS 推送在 P1；P0 用队列/轮询）。
//! 这里直接记录最近一条 payload 到内存（每次 worker 调用独立实例，跨调用持久化需 KV/Queue）。

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use vtc_core::error::AppResult;

pub struct InMemPush {
    latest: RwLock<HashMap<String, String>>,
}

impl InMemPush {
    pub fn new() -> Self {
        InMemPush { latest: RwLock::new(HashMap::new()) }
    }
}

#[async_trait(?Send)]
impl vtc_core::backend::Push for InMemPush {
    async fn publish(&self, channel: &str, payload: &str) -> AppResult<()> {
        self.latest.write().unwrap().insert(channel.to_string(), payload.to_string());
        tracing::debug!(channel, "内存推送（P0 存根，CF 形态）");
        Ok(())
    }

    fn subscriber_count(&self) -> usize {
        self.latest.read().unwrap().len()
    }
}
