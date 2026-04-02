//! PKCE utilities for OIDC authorization code flow

use rand::Rng;
use sha2::{Digest, Sha256};

/// Generate a random code_verifier (43-128 chars per spec, we use 43 chars).
/// Uses base64url-no-pad encoding of 32 random bytes.
pub fn generate_code_verifier() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill(&mut buf);
    base64_url_encode(&buf)
}

/// Compute code_challenge from code_verifier using S256 method.
/// SHA-256(code_verifier) then base64url-no-pad.
pub fn compute_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    base64_url_encode(&hash)
}

/// Generate a random state parameter (32 bytes hex = 64 chars).
pub fn generate_state() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill(&mut buf);
    hex::encode(buf)
}

/// Generate a random nonce parameter (32 bytes hex = 64 chars).
pub fn generate_nonce() -> String {
    let mut buf = [0u8; 32];
    rand::thread_rng().fill(&mut buf);
    hex::encode(buf)
}

/// Base64url encoding without padding.
fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_verifier_length() {
        let verifier = generate_code_verifier();
        // 32 bytes base64url-no-pad = 43 chars
        assert_eq!(verifier.len(), 43);
    }

    #[test]
    fn test_code_challenge_deterministic() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = compute_code_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn test_code_challenge_different_for_different_verifiers() {
        let v1 = generate_code_verifier();
        let v2 = generate_code_verifier();
        if v1 != v2 {
            assert_ne!(compute_code_challenge(&v1), compute_code_challenge(&v2));
        }
    }

    #[test]
    fn test_state_and_nonce_length() {
        let state = generate_state();
        let nonce = generate_nonce();
        assert_eq!(state.len(), 64);
        assert_eq!(nonce.len(), 64);
    }

    #[test]
    fn test_state_uniqueness() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2);
    }
}
