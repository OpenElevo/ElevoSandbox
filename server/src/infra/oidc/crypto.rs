//! AES-256-GCM encryption for OIDC client_secret storage

use aes_gcm::{
    aead::{Aead, KeyInit, OsRng},
    Aes256Gcm, AeadCore, Nonce,
};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};

use super::types::OidcError;

/// Encrypt plaintext using AES-256-GCM.
/// Returns base64(iv || ciphertext || tag).
pub fn encrypt_client_secret(plaintext: &str, key: &[u8; 32]) -> Result<String, OidcError> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| OidcError::CryptoError(format!("cipher init: {}", e)))?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bit nonce

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| OidcError::CryptoError(format!("encrypt: {}", e)))?;

    // Pack: nonce (12 bytes) + ciphertext (includes tag)
    let mut packed = nonce.to_vec();
    packed.extend_from_slice(&ciphertext);

    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(&packed))
}

/// Decrypt ciphertext produced by `encrypt_client_secret`.
pub fn decrypt_client_secret(ciphertext_b64: &str, key: &[u8; 32]) -> Result<String, OidcError> {
    use base64::Engine;
    let packed = base64::engine::general_purpose::STANDARD
        .decode(ciphertext_b64)
        .map_err(|e| OidcError::CryptoError(format!("base64 decode: {}", e)))?;

    if packed.len() < 13 {
        // nonce (12) + at least 1 byte ciphertext+tag
        return Err(OidcError::CryptoError("ciphertext too short".to_string()));
    }

    let (nonce_bytes, ciphertext) = packed.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| OidcError::CryptoError(format!("cipher init: {}", e)))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| OidcError::CryptoError(format!("decrypt: {}", e)))?;

    String::from_utf8(plaintext)
        .map_err(|e| OidcError::CryptoError(format!("utf8: {}", e)))
}

/// Derive a 32-byte encryption key from a secret using HKDF-SHA256.
/// salt = SHA-256("elevo-oidc-salt")[0..16], info = "elevo-oidc-secret-encryption-v1"
pub fn derive_encryption_key(jwt_secret: &str) -> [u8; 32] {
    let salt_hash = sha2::Sha256::digest(b"elevo-oidc-salt");
    let salt = &salt_hash[..16]; // first 16 bytes

    let hk = Hkdf::<Sha256>::new(Some(salt), jwt_secret.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(b"elevo-oidc-secret-encryption-v1", &mut okm)
        .expect("HKDF expand should not fail with valid inputs");
    okm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = derive_encryption_key("test-jwt-secret-at-least-32-bytes-long!!");
        let plaintext = "my-super-secret-client-secret";

        let encrypted = encrypt_client_secret(plaintext, &key).unwrap();
        let decrypted = decrypt_client_secret(&encrypted, &key).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = derive_encryption_key("secret-one-at-least-32-bytes-long!!!");
        let key2 = derive_encryption_key("secret-two-at-least-32-bytes-long!!!");

        let encrypted = encrypt_client_secret("hello", &key1).unwrap();
        assert!(decrypt_client_secret(&encrypted, &key2).is_err());
    }

    #[test]
    fn test_decrypt_tampered_fails() {
        let key = derive_encryption_key("secret-at-least-32-bytes-long!!!");
        let encrypted = encrypt_client_secret("hello", &key).unwrap();

        // Tamper with the base64
        let tampered = format!("{}XX", &encrypted[..encrypted.len().saturating_sub(2)]);
        assert!(decrypt_client_secret(&tampered, &key).is_err());
    }

    #[test]
    fn test_derive_encryption_key_deterministic() {
        let key1 = derive_encryption_key("same-secret-input-32-bytes-min!!");
        let key2 = derive_encryption_key("same-secret-input-32-bytes-min!!");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_derive_encryption_key_different_inputs() {
        let key1 = derive_encryption_key("input-one-at-least-32-bytes-long!!");
        let key2 = derive_encryption_key("input-two-at-least-32-bytes-long!!");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_encrypted_output_is_base64() {
        use base64::Engine;
        let key = derive_encryption_key("secret-at-least-32-bytes-long!!!");
        let encrypted = encrypt_client_secret("test", &key).unwrap();
        // Should be valid base64
        assert!(base64::engine::general_purpose::STANDARD
            .decode(&encrypted)
            .is_ok());
    }

    #[test]
    fn test_empty_plaintext() {
        let key = derive_encryption_key("secret-at-least-32-bytes-long!!!");
        let encrypted = encrypt_client_secret("", &key).unwrap();
        let decrypted = decrypt_client_secret(&encrypted, &key).unwrap();
        assert_eq!(decrypted, "");
    }
}
