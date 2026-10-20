//! Cloudflare R2 存储（Workers 形态）：实现 vtc_core::backend::Storage trait
//!
//! P0：R2 0.8 无 `signed_url()` API，故**不走预签名 URL**（那是 v1.3+ 的能力），
//! 改为 worker 路由中转：
//! - `presign_upload/download` 返回指向 worker `/storage/upload|download?key=..&sig=..` 的 URL
//!   （与 Docker 形态 LocalStorage 同款 HMAC 签名鉴权）。
//! - worker 的 `/storage/*` 路由校验 `sig` 后直接 `bucket.put()` / `bucket.get()` 流式读写 R2。
//! - `local_access` 在 worker 形态 = 真正的 R2 读写（Docker 形态才写本地磁盘）。
//!
//! 这样同一份 vtc-core service 逻辑在两种形态下都成立：service 拿 presign URL 给客户端，
//! 客户端 PUT/GET 该 URL，由后端（本地磁盘 / R2）落地。

use async_trait::async_trait;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::rc::Rc;
use vtc_core::error::{AppError, AppResult};
use worker::Bucket; // worker 0.8.6：r2 模块私有，Bucket 在顶层 re-export（pub use crate::r2::*）

/// R2 存储。worker 是单线程运行时（wasm），`Bucket`/`Object` 视图非 `Sync`，
/// 故用 `Rc<Bucket>` 共享（不跨线程），`Storage` 可 `Send+Sync`。
pub struct R2Storage {
    bucket: Rc<Bucket>,
    /// 对外基址（拼 /storage/ 中转 URL；worker 形态用 PUBLIC_BASE 环境变量）
    public_base: String,
    sign_key: Vec<u8>,
}

impl R2Storage {
    pub fn new(bucket: Bucket, public_base: &str) -> Self {
        let sign_key = std::env::var("VTC_STORAGE_SIGN_KEY")
            .unwrap_or_else(|_| "vtc-r2-sign-key-change-me".into())
            .into_bytes();
        R2Storage {
            bucket: Rc::new(bucket),
            public_base: public_base.trim_end_matches('/').to_string(),
            sign_key,
        }
    }

    fn sign(&self, key: &str, op: &str) -> String {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.sign_key).expect("hmac key");
        mac.update(key.as_bytes());
        mac.update(b"|");
        mac.update(op.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    fn safe_key(&self, key: &str) -> AppResult<String> {
        if key.contains("..") || key.starts_with('/') {
            return Err(AppError::BadRequest("非法存储 key".into()));
        }
        Ok(key.to_string())
    }

    /// P1 直传/列表对象用；P0 经 `local_access` 中转，此处占位
    #[allow(dead_code)]
    pub fn bucket(&self) -> &Bucket {
        &self.bucket
    }
    /// P1 跨调用共享 `Rc<Bucket>`（worker 路由中转用）；P0 占位
    #[allow(dead_code)]
    pub fn bucket_ref(&self) -> Rc<Bucket> {
        Rc::clone(&self.bucket)
    }
}

#[async_trait(?Send)]
impl vtc_core::backend::Storage for R2Storage {
    async fn presign_upload(&self, key: &str, _size: i64) -> AppResult<String> {
        let sig = self.sign(key, "PUT");
        Ok(format!("{}/storage/upload?key={}&sig={}", self.public_base, urlenc(key), urlenc(&sig)))
    }

    async fn presign_download(&self, key: &str) -> AppResult<String> {
        let sig = self.sign(key, "GET");
        Ok(format!("{}/storage/download?key={}&sig={}", self.public_base, urlenc(key), urlenc(&sig)))
    }

    /// worker 形态：真正的 R2 读写（由 /storage/* 路由调用）
    async fn local_access(&self, op: &str, key: &str, body: Option<Vec<u8>>) -> AppResult<(Vec<u8>, String, u16)> {
        let key = self.safe_key(key)?;
        match op {
            "PUT" => {
                if let Some(b) = body {
                    self.bucket.put(&key, b).execute().await.map_err(|x| AppError::Internal(anyhow::anyhow!("{x}")))?;
                }
                Ok((Vec::new(), "text/plain".into(), 200))
            }
            "GET" => {
                let obj = self
                    .bucket
                    .get(&key)
                    .execute()
                    .await
                    .map_err(|x| AppError::Internal(anyhow::anyhow!("{x}")))?
                    .ok_or_else(|| AppError::NotFound("文件不存在".into()))?;
                let bytes = obj
                    .body()
                    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("R2 对象无 body")))?
                    .bytes()
                    .await
                    .map_err(|x| AppError::Internal(anyhow::anyhow!("{x}")))?;
                Ok((bytes, "application/octet-stream".into(), 200))
            }
            _ => Err(AppError::BadRequest("未知操作".into())),
        }
    }
}

fn urlenc(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".into(),
            c => format!("%{:02X}", c as u8),
        })
        .collect()
}
