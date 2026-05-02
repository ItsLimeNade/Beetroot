use crate::error::CoreError;
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose};
use std::sync::OnceLock;

struct TokenCrypto {
    cipher: Aes256Gcm,
}

impl TokenCrypto {
    fn new() -> Self {
        let salt = dotenvy::var("ENCRYPTION_SALT")
            .unwrap_or_else(|_| "beetroot_default_salt_change_in_production".to_string());

        let key_material = format!("beetroot_token_encryption_v1_{}", salt);
        let hash = blake3::hash(key_material.as_bytes());
        let key = Key::<Aes256Gcm>::from_slice(hash.as_bytes());

        Self {
            cipher: Aes256Gcm::new(key),
        }
    }

    fn encrypt(&self, plaintext: &str) -> Result<String, CoreError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| CoreError::Crypto(format!("encryption failed: {e}")))?;

        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);

        Ok(general_purpose::STANDARD.encode(combined))
    }

    fn decrypt(&self, encrypted: &str) -> Result<String, CoreError> {
        let combined = general_purpose::STANDARD
            .decode(encrypted)
            .map_err(|e| CoreError::Crypto(format!("base64 decode failed: {e}")))?;

        if combined.len() < 12 {
            return Err(CoreError::Crypto("ciphertext too short".to_string()));
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| CoreError::Crypto(format!("decryption failed: {e}")))?;

        String::from_utf8(plaintext)
            .map_err(|e| CoreError::Crypto(format!("invalid utf-8 in plaintext: {e}")))
    }
}

static CRYPTO: OnceLock<TokenCrypto> = OnceLock::new();

/// Encrypt a Nightscout token (or any short secret) for storage in the DB.
pub fn encrypt_token(token: &str) -> Result<String, CoreError> {
    CRYPTO.get_or_init(TokenCrypto::new).encrypt(token)
}

/// Decrypt a token previously produced by [`encrypt_token`].
pub fn decrypt_token(encrypted: &str) -> Result<String, CoreError> {
    CRYPTO.get_or_init(TokenCrypto::new).decrypt(encrypted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        // Force a known salt so the test is deterministic regardless of env.
        // SAFETY: tests run in a single thread per default in this crate.
        unsafe { std::env::set_var("ENCRYPTION_SALT", "test_salt_for_roundtrip") };

        let plaintext = "ns-bearer-eyJhbGciOiJIUzI1NiJ9.deadbeef";
        let ciphertext = encrypt_token(plaintext).expect("encrypt");
        let decrypted = decrypt_token(&ciphertext).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn ciphertexts_differ_each_call() {
        // Same plaintext + same key MUST produce different outputs because
        // the nonce is randomized. This is what makes AES-GCM safe to reuse.
        let a = encrypt_token("hello").unwrap();
        let b = encrypt_token("hello").unwrap();
        assert_ne!(a, b);
    }
}
