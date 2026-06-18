pub mod refresh;

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, OsRng, rand_core::RngCore},
};
use anyhow::{Context, anyhow};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub id: String,
    #[serde(default)]
    pub workos_user_id: Option<String>,
    pub google_account_email: String,
    #[serde(default)]
    pub gmail_account_email: Option<String>,
    pub access_token_encrypted: String,
    pub refresh_token_encrypted: Option<String>,
    #[serde(default)]
    pub gmail_access_token_encrypted: Option<String>,
    #[serde(default)]
    pub gmail_refresh_token_encrypted: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GoogleTokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

pub fn encrypt_token(plaintext: &str, key_material: &str) -> anyhow::Result<String> {
    let key = Sha256::digest(key_material.as_bytes());
    let cipher = Aes256Gcm::new_from_slice(&key).context("invalid encryption key")?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| anyhow!("failed to encrypt token"))?;
    Ok(format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(nonce_bytes),
        URL_SAFE_NO_PAD.encode(ciphertext)
    ))
}

pub fn decrypt_token(encrypted: &str, key_material: &str) -> anyhow::Result<String> {
    let (nonce, ciphertext) = encrypted
        .split_once('.')
        .ok_or_else(|| anyhow!("invalid encrypted token format"))?;
    let nonce = URL_SAFE_NO_PAD.decode(nonce).context("invalid nonce")?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(ciphertext)
        .context("invalid ciphertext")?;
    let key = Sha256::digest(key_material.as_bytes());
    let cipher = Aes256Gcm::new_from_slice(&key).context("invalid encryption key")?;
    let plaintext = cipher
        .decrypt(Nonce::from_slice(&nonce), ciphertext.as_slice())
        .map_err(|_| anyhow!("failed to decrypt token"))?;
    String::from_utf8(plaintext).context("token is not valid utf-8")
}

pub fn sign_session_id(session_id: &str, secret: &str) -> anyhow::Result<String> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(secret.as_bytes()).context("invalid session secret")?;
    mac.update(session_id.as_bytes());
    let sig = mac.finalize().into_bytes();
    Ok(format!("{}.{}", session_id, URL_SAFE_NO_PAD.encode(sig)))
}

pub fn verify_session_cookie(cookie_value: &str, secret: &str) -> Option<String> {
    let (session_id, sig) = cookie_value.split_once('.')?;
    let expected = sign_session_id(session_id, secret).ok()?;
    if expected == cookie_value {
        Some(session_id.to_string())
    } else {
        let _ = sig;
        None
    }
}

pub fn session_cookie(value: &str, same_site: &str, secure: bool) -> String {
    cookie("ghmi_session", value, 2_592_000, same_site, secure)
}

pub fn oauth_cookie(value: &str, same_site: &str, secure: bool) -> String {
    cookie("ghmi_oauth", value, 600, same_site, secure)
}

pub fn clear_session_cookie(same_site: &str, secure: bool) -> String {
    cookie("ghmi_session", "", 0, same_site, secure)
}

pub fn clear_oauth_cookie(same_site: &str, secure: bool) -> String {
    cookie("ghmi_oauth", "", 0, same_site, secure)
}

fn cookie(name: &str, value: &str, max_age: i64, same_site: &str, secure: bool) -> String {
    let secure_attr = if secure { "; Secure" } else { "" };
    format!(
        "{name}={value}; Path=/; HttpOnly; SameSite={same_site}; Max-Age={max_age}{secure_attr}"
    )
}
