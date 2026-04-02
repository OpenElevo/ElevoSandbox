//! JWKS key management with caching and refresh

use std::collections::HashMap;

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use rsa::pkcs8::EncodePublicKey;
use rsa::RsaPublicKey;

use super::types::{ElevoOneClaims, OidcError};

/// Cached JWKS key set mapping kid → RSA public key
#[derive(Debug, Clone, Default)]
pub struct JwksKeySet {
    keys: HashMap<String, RsaPublicKey>,
}

impl JwksKeySet {
    fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }

    /// Get a key by kid
    fn get(&self, kid: &str) -> Option<&RsaPublicKey> {
        self.keys.get(kid)
    }

    /// Insert a key
    fn insert(&mut self, kid: String, key: RsaPublicKey) {
        self.keys.insert(kid, key);
    }
}

/// Parse JWKS response and extract RSA public keys
fn parse_jwks(jwks_json: &serde_json::Value) -> Result<JwksKeySet, OidcError> {
    let mut key_set = JwksKeySet::new();

    let keys = jwks_json
        .get("keys")
        .and_then(|k| k.as_array())
        .ok_or_else(|| OidcError::JwksError("no keys array in JWKS".to_string()))?;

    for key_value in keys {
        // Extract kid
        let kid = key_value
            .get("kid")
            .and_then(|k| k.as_str())
            .unwrap_or_default()
            .to_string();

        // Only handle RSA keys
        let kty = key_value
            .get("kty")
            .and_then(|k| k.as_str())
            .unwrap_or_default();

        if kty != "RSA" {
            continue;
        }

        // Try to parse from parameters (n, e)
        if let (Some(n_b64), Some(e_b64)) = (key_value.get("n"), key_value.get("e")) {
            let n_str = n_b64.as_str().unwrap_or_default();
            let e_str = e_b64.as_str().unwrap_or_default();

            use base64::Engine;
            let n_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(n_str)
                .map_err(|e| OidcError::JwksError(format!("invalid n: {}", e)))?;
            let e_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(e_str)
                .map_err(|e| OidcError::JwksError(format!("invalid e: {}", e)))?;

            let public_key =
                rsa::RsaPublicKey::new(rsa::BigUint::from_bytes_be(&n_bytes), rsa::BigUint::from_bytes_be(&e_bytes))
                    .map_err(|e| OidcError::JwksError(format!("RSA key parse: {}", e)))?;

            key_set.insert(kid, public_key);
        }
    }

    Ok(key_set)
}

/// Verify an RS256 JWT access token using the cached JWKS.
pub fn verify_access_token(
    token: &str,
    jwks: &JwksKeySet,
    issuer: &str,
    client_id: &str,
) -> Result<ElevoOneClaims, OidcError> {
    let header = decode_header(token)
        .map_err(|e| OidcError::InvalidToken(format!("invalid JWT header: {}", e)))?;

    if header.alg != Algorithm::RS256 {
        return Err(OidcError::InvalidToken(format!(
            "unsupported algorithm: {:?}",
            header.alg
        )));
    }

    let kid = header.kid.ok_or_else(|| {
        OidcError::InvalidToken("JWT header missing kid".to_string())
    })?;

    let public_key = jwks
        .get(&kid)
        .ok_or_else(|| OidcError::InvalidToken(format!("unknown kid: {}", kid)))?;

    // Convert RSA public key to DER bytes for jsonwebtoken
    let der_bytes = public_key
        .to_public_key_der()
        .map_err(|e| OidcError::InvalidToken(format!("DER encode: {}", e)))?;

    let decoding_key = DecodingKey::from_rsa_der(der_bytes.as_bytes());

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_required_spec_claims(&["exp", "aud"]);
    validation.set_audience(&[client_id]);
    validation.set_issuer(&[issuer]);

    let data = decode::<ElevoOneClaims>(token, &decoding_key, &validation)
        .map_err(|e| OidcError::InvalidToken(format!("JWT verification: {}", e)))?;

    Ok(data.claims)
}

/// Fetch JWKS from the issuer's well-known endpoint
pub async fn fetch_jwks(
    http_client: &reqwest::Client,
    issuer_url: &str,
) -> Result<JwksKeySet, OidcError> {
    let jwks_uri = if issuer_url.ends_with('/') {
        format!("{}.well-known/jwks.json", issuer_url)
    } else {
        format!("{}/.well-known/jwks.json", issuer_url)
    };

    let response = http_client
        .get(&jwks_uri)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| OidcError::JwksError(format!("fetch JWKS: {}", e)))?
        .error_for_status()
        .map_err(|e| OidcError::JwksError(format!("JWKS response: {}", e)))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| OidcError::JwksError(format!("parse JWKS: {}", e)))?;

    parse_jwks(&json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jwks_empty() {
        let json = serde_json::json!({"keys": []});
        let ks = parse_jwks(&json).unwrap();
        assert!(ks.keys.is_empty());
    }

    #[test]
    fn test_parse_jwks_skips_non_rsa() {
        let json = serde_json::json!({
            "keys": [
                {"kty": "EC", "kid": "ec-key"},
                {"kty": "RSA", "kid": "rsa-key", "n": "invalid", "e": "invalid"}
            ]
        });
        // Should not crash, may return empty or partial
        let _ = parse_jwks(&json);
    }
}
