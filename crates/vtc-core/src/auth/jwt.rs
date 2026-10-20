//! JWT HS256 签发 / 校验（手写，避免 jsonwebtoken 拉入 ring C 编译）
//! 对齐 VTC.Server.TokenService

use crate::constants::jwt;
use crate::error::{AppError, AppResult};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Claims {
    pub sub: String,
    pub username: String,
    pub role: String,
    pub status: String,
    pub exp: i64,
    pub iat: i64,
    pub iss: String,
    pub aud: String,
    #[serde(rename = "type", default)]
    pub typ: String,
}

/// JWT Header
#[derive(Serialize, Deserialize)]
struct JwsHeader {
    alg: String,
    typ: String,
}

fn b64url_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(b64url_char(((n >> 18) & 0x3f) as usize));
        out.push(b64url_char(((n >> 12) & 0x3f) as usize));
        if chunk.len() > 1 {
            out.push(b64url_char(((n >> 6) & 0x3f) as usize));
        }
        if chunk.len() > 2 {
            out.push(b64url_char((n & 0x3f) as usize));
        }
    }
    out
}

/// base64url 字符表（`+`→`-`、`/`→`_`，JWT 规范）
const B64URL_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64url_char(i: usize) -> char {
    B64URL_CHARS[i] as char
}

fn b64url_decode(s: &str) -> AppResult<Vec<u8>> {
    let s = s.trim_end_matches('=');
    let mut buf: Vec<u8> = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for c in s.chars() {
        // 防御：跳过空白（合法 token 绝不应因一个多余空格 400；真实客户端偶尔夹带）
        if c.is_whitespace() {
            continue;
        }
        let v = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            'a'..='z' => c as u32 - 'a' as u32 + 26,
            '0'..='9' => c as u32 - '0' as u32 + 52,
            '+' => 62,
            '/' => 63, // 标准 base64（兼容我方编码器未做 url-safe 转换的输出）
            '-' => 62, // url-safe
            '_' => 63, // url-safe
            '=' => continue,
            _ => return Err(AppError::BadRequest("无效 base64url 字符".into())),
        };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            buf.push(((acc >> bits) & 0xff) as u8);
        }
    }
    Ok(buf)
}

fn b64url_decode_json<T: serde::de::DeserializeOwned>(s: &str) -> AppResult<T> {
    let bytes = b64url_decode(s)?;
    serde_json::from_slice(&bytes).map_err(|e| AppError::BadRequest(format!("JSON 解析失败：{e}")))
}

pub struct JwtService {
    secret: Vec<u8>,
}

impl JwtService {
    pub fn new(secret: &str) -> Self {
        JwtService { secret: secret.as_bytes().to_vec() }
    }

    fn sign(&self, claims: &Claims) -> AppResult<String> {
        let header = JwsHeader { alg: "HS256".into(), typ: "JWT".into() };
        let header_b64 = b64url_encode(
            &serde_json::to_vec(&header).map_err(|e| AppError::Internal(e.into()))?,
        );
        let payload_b64 = b64url_encode(
            &serde_json::to_vec(claims).map_err(|e| AppError::Internal(e.into()))?,
        );
        let signing_input = format!("{header_b64}.{payload_b64}");
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|e| AppError::Internal(e.into()))?;
        mac.update(signing_input.as_bytes());
        let sig = mac.finalize().into_bytes();
        Ok(format!("{signing_input}.{}", b64url_encode(&sig)))
    }

    fn verify(&self, token: &str) -> AppResult<(JwsHeader, Claims)> {
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(AppError::Unauthorized("token 格式错误".into()));
        }
        let header: JwsHeader = b64url_decode_json(parts[0])?;
        if header.alg != "HS256" {
            return Err(AppError::Unauthorized("不支持的算法".into()));
        }
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let mut mac =
            HmacSha256::new_from_slice(&self.secret).map_err(|e| AppError::Internal(e.into()))?;
        mac.update(signing_input.as_bytes());
        let expected = mac.finalize().into_bytes();
        let actual = b64url_decode(parts[2])?;
        // 常量时间比较
        if expected.len() != actual.len() || expected.iter().zip(actual.iter()).any(|(a, b)| a != b) {
            return Err(AppError::Unauthorized("签名无效".into()));
        }
        let claims: Claims = b64url_decode_json(parts[1])?;
        // exp 校验
        if claims.exp < chrono::Utc::now().timestamp() {
            return Err(AppError::Unauthorized("token 已过期".into()));
        }
        // iss / aud 校验
        if claims.iss != jwt::ISSUER || claims.aud != jwt::AUDIENCE {
            return Err(AppError::Unauthorized("issuer/audience 不匹配".into()));
        }
        Ok((header, claims))
    }

    pub fn issue_access(&self, user_id: &str, username: &str, role: &str, status: &str) -> AppResult<String> {
        let now = chrono::Utc::now().timestamp();
        self.sign(&Claims {
            sub: user_id.into(),
            username: username.into(),
            role: role.into(),
            status: status.into(),
            exp: now + jwt::ACCESS_TTL as i64,
            iat: now,
            iss: jwt::ISSUER.into(),
            aud: jwt::AUDIENCE.into(),
            typ: "access".into(),
        })
    }

    pub fn issue_refresh(&self, user_id: &str, username: &str) -> AppResult<String> {
        let now = chrono::Utc::now().timestamp();
        self.sign(&Claims {
            sub: user_id.into(),
            username: username.into(),
            role: String::new(),
            status: String::new(),
            exp: now + jwt::REFRESH_TTL_DAYS * 86400,
            iat: now,
            iss: jwt::ISSUER.into(),
            aud: jwt::AUDIENCE.into(),
            typ: "refresh".into(),
        })
    }

    pub fn validate(&self, token: &str) -> AppResult<Claims> {
        let (_, claims) = self.verify(token)?;
        Ok(claims)
    }

    pub fn access_ttl() -> i64 { jwt::ACCESS_TTL as i64 }
}
