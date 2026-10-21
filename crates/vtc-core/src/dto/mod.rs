//! DTO：对齐 VTC.Shared.Dtos（camelCase 序列化，与 C# 客户端兼容）

pub mod auth;
pub mod common;
pub mod config;
pub mod member;
pub mod save;
pub mod dlc;
pub mod content;
pub mod audit;
pub mod events_log;
// P1
pub mod points;
pub mod rss;
pub mod threec;

use serde::Serialize;

/// 分页结果（对齐 VTC.Shared.PageResult<T>）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResult<T: Serialize> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

/// 分页查询参数
#[derive(Debug, Clone, Copy)]
pub struct PageQuery {
    pub take: i64,
    pub skip: i64,
}

impl Default for PageQuery {
    fn default() -> Self {
        PageQuery { take: 20, skip: 0 }
    }
}
