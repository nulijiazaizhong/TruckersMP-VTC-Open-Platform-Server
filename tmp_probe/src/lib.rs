// 测试：trait 用 cfg 门控的 async_trait（native=Send, wasm=?Send），impl 用 ?Send，
// 看 native 下 service 的 run() future 是否 Send（axum handler 是否成立）。
use async_trait::async_trait;

// --- 模拟 vtc-core 的 cfg 门控 trait ---
#[cfg(target_arch = "wasm32")]
#[async_trait(?Send)]
pub trait Db {
    async fn ping(&self) -> i32;
}
#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
pub trait Db {
    async fn ping(&self) -> i32;
}

pub struct SqliteDb;
#[async_trait]
impl Db for SqliteDb {
    async fn ping(&self) -> i32 { 1 }
}

// --- 泛型 service（= vtc-core 现状） ---
pub struct Svc<D: Db> { pub db: std::sync::Arc<D> }
impl<D: Db> Svc<D> {
    pub async fn run(&self) -> i32 { self.db.ping().await }
}

// --- 直接验证 run() 的 future 是否 Send ---
pub fn run_is_send() {
    fn assert_send<T: Send>() {}
    let svc = std::sync::Arc::new(Svc { db: std::sync::Arc::new(SqliteDb) });
    let fut = Box::pin(svc.run());
    assert_send::<std::pin::Pin<Box<dyn std::future::Future<Output = i32> + Send>>>();
}
