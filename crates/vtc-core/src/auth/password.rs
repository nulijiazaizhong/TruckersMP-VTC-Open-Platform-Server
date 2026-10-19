//! PBKDF2-HMAC-SHA256 密码哈希（210000 轮，对齐 VTC.Server.PasswordHasher）
//! 格式：`$pbkdf2$<iters>$<salt_hex>$<hash_hex>`
//!
//! 手写 PBKDF2（基于 hmac + sha2），不依赖 pbkdf2 crate（避免拉入 ring C 编译）。

use crate::error::{AppError, AppResult};
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub const ITERS: u32 = 210_000;
pub const SALT_LEN: usize = 16;
pub const DK_LEN: usize = 32;

/// PBKDF2-HMAC-SHA256 核心
fn pbkdf2(password: &[u8], salt: &[u8], iters: u32, dk_len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(dk_len);
    let mut block_index: u32 = 1;
    let mut prev_block: Option<Vec<u8>> = None;
    while out.len() < dk_len {
        // U1 = H(S || INT(block_index))
        let mut mac = HmacSha256::new_from_slice(password).expect("hmac key");
        mac.update(salt);
        mac.update(&block_index.to_be_bytes());
        let mut u = mac.finalize().into_bytes().to_vec();
        let mut block = u.clone();
        // U2..Uc
        for _ in 2..=iters {
            let mut m2 = HmacSha256::new_from_slice(password).expect("hmac key");
            m2.update(&u);
            u = m2.finalize().into_bytes().to_vec();
            for (i, b) in block.iter_mut().enumerate() {
                *b ^= u[i];
            }
        }
        out.extend_from_slice(&block);
        block_index += 1;
        let _ = prev_block.as_mut();
    }
    out.truncate(dk_len);
    out
}

/// 生成密码哈希
pub fn hash_password(password: &str) -> String {
    let mut rng = rand::rng();
    let mut salt = [0u8; SALT_LEN];
    rng.fill(&mut salt);
    let dk = pbkdf2(password.as_bytes(), &salt, ITERS, DK_LEN);
    format!("$pbkdf2${iters}${salt_hex}${dk_hex}",
        iters = ITERS,
        salt_hex = hex::encode(salt),
        dk_hex = hex::encode(dk))
}

/// 校验密码
pub fn verify_password(password: &str, stored: &str) -> AppResult<bool> {
    let parts: Vec<&str> = stored.split('$').collect();
    if parts.len() != 5 || parts[1] != "pbkdf2" {
        return Ok(false);
    }
    let iters: u32 = parts[2].parse().map_err(|_| AppError::BadRequest("无效哈希".into()))?;
    let salt = hex::decode(parts[3]).map_err(|_| AppError::BadRequest("无效 salt".into()))?;
    let expected = hex::decode(parts[4]).map_err(|_| AppError::BadRequest("无效 hash".into()))?;
    let dk = pbkdf2(password.as_bytes(), &salt, iters, expected.len());
    Ok(dk == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let h = hash_password("ChangeMe123!");
        assert!(verify_password("ChangeMe123!", &h).unwrap());
        assert!(!verify_password("wrong", &h).unwrap());
    }
}
