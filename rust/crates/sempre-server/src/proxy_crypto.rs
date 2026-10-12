use crate::error::ApiError;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit as _, Nonce,
    aead::{Aead as _, Payload},
};
use rand::RngCore as _;
use uuid::Uuid;

pub(crate) fn key(value: &str) -> Result<[u8; 32], String> {
    STANDARD
        .decode(value)
        .map_err(|_| "SEMPRE_PROXY_KEY must be base64".to_owned())?
        .try_into()
        .map_err(|_| "SEMPRE_PROXY_KEY must encode exactly 32 bytes".into())
}

pub(crate) fn encrypt(key: &[u8; 32], id: Uuid, content: &[u8]) -> Result<String, ApiError> {
    let mut nonce = [0; 12];
    rand::rng().fill_bytes(&mut nonce);
    let encrypted = ChaCha20Poly1305::new(key.into())
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: content,
                aad: id.as_bytes(),
            },
        )
        .map_err(|_| ApiError::unavailable("proxy encryption failed"))?;
    let mut bytes = nonce.to_vec();
    bytes.extend(encrypted);
    Ok(STANDARD.encode(bytes))
}

pub(crate) fn decrypt(key: &[u8; 32], id: Uuid, content: &str) -> Result<Vec<u8>, ApiError> {
    let bytes = STANDARD
        .decode(content)
        .map_err(|_| ApiError::unavailable("proxy configuration is unreadable"))?;
    if bytes.len() < 28 {
        return Err(ApiError::unavailable("proxy configuration is unreadable"));
    }
    ChaCha20Poly1305::new(key.into())
        .decrypt(
            Nonce::from_slice(&bytes[..12]),
            Payload {
                msg: &bytes[12..],
                aad: id.as_bytes(),
            },
        )
        .map_err(|_| ApiError::unavailable("proxy key does not match the stored configuration"))
}
