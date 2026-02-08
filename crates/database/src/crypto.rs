use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
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

    fn encrypt(&self, plaintext: &str) -> anyhow::Result<String> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let ciphertext = self.cipher.encrypt(&nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);

        Ok(general_purpose::STANDARD.encode(combined))
    }

    fn decrypt(&self, encrypted: &str) -> anyhow::Result<String> {
        let combined = general_purpose::STANDARD.decode(encrypted)?;
        if combined.len() < 12 {
            return Err(anyhow::anyhow!("Invalid encrypted data"));
        }

        let (nonce_bytes, ciphertext) = combined.split_at(12);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self.cipher.decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

        Ok(String::from_utf8(plaintext)?)
    }
}

static CRYPTO: OnceLock<TokenCrypto> = OnceLock::new();

pub fn encrypt_token(token: &str) -> anyhow::Result<String> {
    CRYPTO.get_or_init(TokenCrypto::new).encrypt(token)
}

pub fn decrypt_token(token: &str) -> anyhow::Result<String> {
    CRYPTO.get_or_init(TokenCrypto::new).decrypt(token)
}