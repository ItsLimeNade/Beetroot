use crate::error::CoreError;
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose};
use std::sync::OnceLock;

/// The old hard-coded fallback that used to be substituted silently when no
/// secret was configured. It is derived from public source code, so any DB
/// encrypted under it is effectively unencrypted. We now reject it explicitly
/// rather than ever booting with it.
const INSECURE_DEFAULT: &str = "beetroot_default_salt_change_in_production";

/// Length below which we warn (but still boot, to avoid orphaning tokens that
/// were already encrypted under a shorter secret).
const RECOMMENDED_MIN_LEN: usize = 32;

struct TokenCrypto {
    cipher: Aes256Gcm,
}

/// Resolve the secret that feeds key derivation.
///
/// Reads `ENCRYPTION_KEY` (preferred) and falls back to the legacy
/// `ENCRYPTION_SALT` name so existing deployments keep working unchanged.
/// Fails closed instead of silently using a public default when the value
/// is missing, blank, a template placeholder, or the old insecure default.
///
/// The returned value is the *raw* env value (not trimmed) so the derived key
/// is byte-for-byte identical to what the previous code produced for the same
/// configuration; every already-stored token still decrypts.
fn resolve_secret() -> Result<String, CoreError> {
    let (raw, var_name) = match dotenvy::var("ENCRYPTION_KEY") {
        Ok(v) => (v, "ENCRYPTION_KEY"),
        Err(_) => match dotenvy::var("ENCRYPTION_SALT") {
            Ok(v) => {
                tracing::warn!(
                    "ENCRYPTION_SALT is deprecated; rename it to ENCRYPTION_KEY \
                     (keep the same value so stored tokens stay valid)."
                );
                (v, "ENCRYPTION_SALT")
            }
            Err(_) => {
                return Err(CoreError::Crypto(
                    "ENCRYPTION_KEY is not set. Refusing to start with a key derived \
                     from public source code. Set ENCRYPTION_KEY to a long, random secret."
                        .to_string(),
                ));
            }
        },
    };

    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Err(CoreError::Crypto(format!(
            "{var_name} is empty. Set it to a long, random secret."
        )));
    }

    if trimmed == INSECURE_DEFAULT || trimmed.starts_with('<') {
        return Err(CoreError::Crypto(format!(
            "{var_name} is set to a placeholder/default value. Set it to a unique secret \
             before starting (rotating it invalidates all stored tokens)."
        )));
    }

    if trimmed.len() < RECOMMENDED_MIN_LEN {
        tracing::warn!(
            "{var_name} is only {} characters; {} or more is strongly recommended. \
             Changing it later invalidates all stored tokens.",
            trimmed.len(),
            RECOMMENDED_MIN_LEN
        );
    }

    Ok(raw)
}

impl TokenCrypto {
    fn new() -> Result<Self, CoreError> {
        let secret = resolve_secret()?;

        let key_material = format!("beetroot_token_encryption_v1_{}", secret);
        let hash = blake3::hash(key_material.as_bytes());
        let key = Key::<Aes256Gcm>::from_slice(hash.as_bytes());

        Ok(Self {
            cipher: Aes256Gcm::new(key),
        })
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

/// Validate the encryption configuration and initialize the cipher eagerly.
///
/// Call this once at startup so a misconfigured deployment fails *closed* with a
/// clear error before serving any traffic, rather than panicking lazily on the
/// first `/setup`. Safe to call more than once.
pub fn init() -> Result<(), CoreError> {
    let crypto = TokenCrypto::new()?;
    // If another caller won the race, the value is already a validated cipher
    // derived from the same env, so discarding ours is fine.
    let _ = CRYPTO.set(crypto);
    Ok(())
}

/// Get the initialized cipher, initializing on first use as a fallback.
///
/// In normal operation [`init`] has already populated this. If it hasn't and the
/// configuration is invalid, we panic rather than fall back to a default key
/// encrypting medical-data tokens under a known key is never an acceptable
/// degraded mode.
fn crypto() -> &'static TokenCrypto {
    CRYPTO.get_or_init(|| {
        TokenCrypto::new().unwrap_or_else(|e| panic!("token encryption is not configured: {e}"))
    })
}

/// Encrypt a Nightscout token (or any short secret) for storage in the DB.
pub fn encrypt_token(token: &str) -> Result<String, CoreError> {
    crypto().encrypt(token)
}

/// Decrypt a token previously produced by [`encrypt_token`].
pub fn decrypt_token(encrypted: &str) -> Result<String, CoreError> {
    crypto().decrypt(encrypted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The global `CRYPTO` is shared across all tests in this binary and is
    /// initialized once, so every test must agree on the same key. Set it before
    /// any crypto call. SAFETY: a single fixed value means the result is the same
    /// regardless of which test wins the init race.
    fn ensure_key() {
        unsafe { std::env::set_var("ENCRYPTION_KEY", "test_key_for_unit_tests_0123456789ab") };
    }

    #[test]
    fn roundtrip() {
        ensure_key();

        let plaintext = "ns-bearer-eyJhbGciOiJIUzI1NiJ9.deadbeef";
        let ciphertext = encrypt_token(plaintext).expect("encrypt");
        let decrypted = decrypt_token(&ciphertext).expect("decrypt");
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn ciphertexts_differ_each_call() {
        ensure_key();

        // Same plaintext + same key MUST produce different outputs because
        // the nonce is randomized. This is what makes AES-GCM safe to reuse.
        let a = encrypt_token("hello").unwrap();
        let b = encrypt_token("hello").unwrap();
        assert_ne!(a, b);
    }
}
