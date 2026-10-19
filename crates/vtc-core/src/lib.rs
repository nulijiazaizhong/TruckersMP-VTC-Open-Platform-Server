//! VTC 车队管理平台 · 核心
//!
//! 同一份代码双编译：
//! - **Docker 自部署**：`vtc-server`（Axum + SQLite + 本地/S3 + 内存推送）
//! - **Cloudflare Workers**：`vtc-worker`（worker crate + D1 + R2 + Durable Objects）
//!
//! 可插拔后端通过 `backend` 模块的三个 trait（Db / Storage / Push）抽象，
//! core 里的服务逻辑只依赖 trait，不依赖具体实现。

pub mod constants;
pub mod error;
pub mod dto;
pub mod domain;
pub mod backend;
pub mod auth;
pub mod service;

pub use error::AppResult;
