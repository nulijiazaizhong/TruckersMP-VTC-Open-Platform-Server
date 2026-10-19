// VTC 服务端 · Docker 自部署形态（Axum + SQLite + 本地存储 + 内存推送）
//
// 启动：`vtc-server` 读环境变量（.env）→ 建表 → 种子 → 起 Axum。
// 详见 crates/vtc-server/src/ 各模块。

mod db;
mod storage;
mod push;
mod routes;
mod bootstrap;

use anyhow::Result;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;
use vtc_core::backend::{Backends, DbRef, PushRef, StorageRef};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = bootstrap::ServerConfig::from_env()?;
    tracing::info!("VTC 服务端启动：port={}", cfg.port);

    // 1) DB
    let sqlite = db::SqliteDb::open(&cfg.db_path)?;
    sqlite.migrate()?;
    let db: DbRef = Arc::new(sqlite);
    // 种子
    bootstrap::seed(&db, &cfg).await?;

    // 2) Storage
    let storage: StorageRef = Arc::new(storage::LocalStorage::new(&cfg.storage_dir, &cfg.public_base));

    // 3) Push
    let push: PushRef = Arc::new(push::InMemPush::new());

    let backends = Backends { db, storage, push };

    // 4) 路由
    let app = routes::build_app(backends, cfg.clone());

    // 5) 起服务
    let addr = format!("0.0.0.0:{}", cfg.port);
    tracing::info!("监听 {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
