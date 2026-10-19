//! 内存推送（Docker 形态）：P0 存根（WS 推送在 P1）

use async_trait::async_trait;
use std::sync::RwLock;
use vtc_core::backend::Push;
use vtc_core::error::AppResult;

pub struct InMemPush {
    /// channel → 最近一条 payload（P0 仅内存，不真正推送）
    latest: RwLock<std::collections::HashMap<String, String>>,
}

impl InMemPush {
    pub fn new() -> Self {
        InMemPush { latest: RwLock::new(std::collections::HashMap::new()) }
    }
}

#[async_trait]
impl Push for InMemPush {
    async fn publish(&self, channel: &str, payload: &str) -> AppResult<()> {
        self.latest.write().unwrap().insert(channel.to_string(), payload.to_string());
        tracing::debug!(channel, "内存推送（P0 存根）");
        Ok(())
    }

    fn subscriber_count(&self) -> usize {
        self.latest.read().unwrap().len()
    }
}
