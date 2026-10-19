//! 本地存储（Docker 形态）：预签名 URL 指向本地 /storage/ 端点
//! 大文件走本地 PUT/GET（HMAC 签名鉴权），不走 S3（P0；S3 在 P1）。

use async_trait::async_trait;
use std::path::PathBuf;
use vtc_core::backend::Storage;
use vtc_core::error::{AppError, AppResult};

pub struct LocalStorage {
    root: PathBuf,
    public_base: String,
    /// HMAC 签名密钥（防篡改）
    sign_key: Vec<u8>,
}

impl LocalStorage {
    pub fn new(root: &str, public_base: &str) -> Self {
        let root = PathBuf::from(root);
        std::fs::create_dir_all(&root).ok();
        let sign_key = std::env::var("VTC_STORAGE_SIGN_KEY")
            .unwrap_or_else(|_| "vtc-local-sign-key-change-me".into())
            .into_bytes();
        LocalStorage { root, public_base: public_base.trim_end_matches('/').to_string(), sign_key }
    }

    fn sign(&self, key: &str, op: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(&self.sign_key).expect("hmac");
        mac.update(key.as_bytes());
        mac.update(b"|");
        mac.update(op.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    fn safe_key(&self, key: &str) -> AppResult<PathBuf> {
        // 防目录穿越
        if key.contains("..") {
            return Err(AppError::BadRequest("非法存储 key".into()));
        }
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn presign_upload(&self, key: &str, _size: i64) -> AppResult<String> {
        let sig = self.sign(key, "PUT");
        Ok(format!("{}/storage/upload?key={}&sig={}", self.public_base, urlenc(key), urlenc(&sig)))
    }

    async fn presign_download(&self, key: &str) -> AppResult<String> {
        let sig = self.sign(key, "GET");
        Ok(format!("{}/storage/download?key={}&sig={}", self.public_base, urlenc(key), urlenc(&sig)))
    }

    async fn local_access(&self, op: &str, key: &str, body: Option<Vec<u8>>) -> AppResult<(Vec<u8>, String, u16)> {
        let path = self.safe_key(key)?;
        match op {
            "PUT" => {
                if let Some(b) = body {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|e| AppError::Internal(e.into()))?;
                    }
                    std::fs::write(&path, &b).map_err(|e| AppError::Internal(e.into()))?;
                }
                Ok((Vec::new(), "text/plain".into(), 200))
            }
            "GET" => {
                let b = std::fs::read(&path).map_err(|_| AppError::NotFound("文件不存在".into()))?;
                Ok((b, "application/octet-stream".into(), 200))
            }
            _ => Err(AppError::BadRequest("未知操作".into())),
        }
    }
}

fn urlenc(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "%20".into(),
        c => format!("%{:02X}", c as u8),
    }).collect()
}
