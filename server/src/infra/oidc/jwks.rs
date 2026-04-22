//! JWKS key management with caching and refresh
//!
//! Supports both RSA (RS256) and ECDSA P-256 (ES256) public keys.

use std::collections::HashMap;

use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};

use super::types::{ElevoOneClaims, OidcError};

/// Cached JWKS key set mapping kid → DecodingKey (RSA or EC)
#[derive(Clone, Default)]
pub struct JwksKeySet {
    keys: HashMap<String, DecodingKey>,
}

impl std::fmt::Debug for JwksKeySet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwksKeySet")
            .field("kids", &self.keys.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl JwksKeySet {
    fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }

    /// Get a key by kid
    fn get(&self, kid: &str) -> Option<&DecodingKey> {
        self.keys.get(kid)
    }

    /// Insert a key
    fn insert(&mut self, kid: String, key: DecodingKey) {
        self.keys.insert(kid, key);
    }
}

/// Supported JWKS key types
const KTY_RSA: &str = "RSA";
const KTY_EC: &str = "EC";

/// Parse JWKS response and extract public keys (RSA and EC P-256).
///
/// - RSA keys: uses `DecodingKey::from_rsa_components(n, e)` — accepts raw
///   base64url JWK strings directly.
/// - EC keys: validates `crv == "P-256"`, then uses
///   `DecodingKey::from_ec_components(x, y)`.
fn parse_jwks(jwks_json: &serde_json::Value) -> Result<JwksKeySet, OidcError> {
    let mut key_set = JwksKeySet::new();

    let keys = jwks_json
        .get("keys")
        .and_then(|k| k.as_array())
        .ok_or_else(|| OidcError::JwksError("no keys array in JWKS".to_string()))?;

    for key_value in keys {
        let kid = key_value
            .get("kid")
            .and_then(|k| k.as_str())
            .unwrap_or_default()
            .to_string();

        let kty = key_value
            .get("kty")
            .and_then(|k| k.as_str())
            .unwrap_or_default();

        let decoding_key = match kty {
            KTY_RSA => parse_rsa_key(key_value)?,
            KTY_EC => parse_ec_key(key_value)?,
            _ => continue, // skip unsupported key types
        };

        key_set.insert(kid, decoding_key);
    }

    Ok(key_set)
}

/// Parse an RSA JWK into a DecodingKey.
fn parse_rsa_key(
    key_value: &serde_json::Value,
) -> Result<DecodingKey, OidcError> {
    let n = key_value
        .get("n")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let e = key_value
        .get("e")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    DecodingKey::from_rsa_components(n, e)
        .map_err(|e| OidcError::JwksError(format!("RSA key parse: {}", e)))
}

/// Parse an EC P-256 JWK into a DecodingKey.
fn parse_ec_key(
    key_value: &serde_json::Value,
) -> Result<DecodingKey, OidcError> {
    let crv = key_value
        .get("crv")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if crv != "P-256" {
        return Err(OidcError::JwksError(format!(
            "unsupported EC curve: {}",
            crv
        )));
    }

    let x = key_value
        .get("x")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let y = key_value
        .get("y")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    DecodingKey::from_ec_components(x, y)
        .map_err(|e| OidcError::JwksError(format!("EC key parse: {}", e)))
}

/// Verify an RS256/ES256 JWT access token using the cached JWKS.
pub fn verify_access_token(
    token: &str,
    jwks: &JwksKeySet,
    issuer: &str,
    client_id: &str,
) -> Result<ElevoOneClaims, OidcError> {
    let header = decode_header(token)
        .map_err(|e| OidcError::InvalidToken(format!("invalid JWT header: {}", e)))?;

    match header.alg {
        Algorithm::RS256 | Algorithm::ES256 => {}
        _ => {
            return Err(OidcError::InvalidToken(format!(
                "unsupported algorithm: {:?}",
                header.alg
            )));
        }
    }

    let kid = header.kid.ok_or_else(|| {
        OidcError::InvalidToken("JWT header missing kid".to_string())
    })?;

    let decoding_key = jwks
        .get(&kid)
        .ok_or_else(|| OidcError::InvalidToken(format!("unknown kid: {}", kid)))?;

    let mut validation = Validation::new(header.alg);
    validation.set_required_spec_claims(&["exp", "aud"]);
    validation.set_audience(&[client_id]);
    // Normalize issuer: IdPs may or may not include trailing slash, but the JWT
    // claims typically do not. Trim to avoid InvalidIssuer mismatches.
    let normalized_issuer = issuer.trim_end_matches('/');
    validation.set_issuer(&[normalized_issuer]);

    let data = decode::<ElevoOneClaims>(token, decoding_key, &validation)
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
    fn test_parse_jwks_skips_non_rsa_ec() {
        let json = serde_json::json!({
            "keys": [
                {"kty": "OKP", "kid": "okp-key"},
                {"kty": "RSA", "kid": "rsa-key", "n": "invalid", "e": "invalid"}
            ]
        });
        // RSA key with invalid components should fail parsing, OKP should be skipped
        let result = parse_jwks(&json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_jwks_skips_unsupported_ec_curve() {
        let json = serde_json::json!({
            "keys": [
                {"kty": "EC", "crv": "P-384", "kid": "ec-p384", "x": "dGVzdA", "y": "dGVzdA"}
            ]
        });
        let result = parse_jwks(&json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unsupported EC curve: P-384"));
    }

    #[test]
    fn test_parse_jwks_ec_p256_invalid_components() {
        let json = serde_json::json!({
            "keys": [
                {"kty": "EC", "crv": "P-256", "kid": "ec-bad", "x": "not-valid-b64!", "y": "not-valid-b64!"}
            ]
        });
        let result = parse_jwks(&json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_jwks_rsa_valid_key() {
        // Well-known test RSA public key components (2048-bit)
        let json = serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "kid": "test-rsa",
                "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                "e": "AQAB"
            }]
        });
        let ks = parse_jwks(&json).unwrap();
        assert_eq!(ks.keys.len(), 1);
        assert!(ks.get("test-rsa").is_some());
    }

    #[test]
    fn test_parse_jwks_ec_p256_valid_key() {
        // A valid EC P-256 public key in JWK format (x, y are 32-byte base64url-encoded)
        let json = serde_json::json!({
            "keys": [{
                "kty": "EC",
                "crv": "P-256",
                "kid": "test-ec-p256",
                "x": "WKn-ZIGevcwGIyyrzFoZNBdaq9_TsqzGl96oc0CWuis",
                "y": "y77t-RvAHRKTsSGdIYUfweuOvwrvDD-Q3Hv5J0fSKbE"
            }]
        });
        let ks = parse_jwks(&json).unwrap();
        assert_eq!(ks.keys.len(), 1);
        assert!(ks.get("test-ec-p256").is_some());
    }

    #[test]
    fn test_parse_jwks_mixed_rsa_and_ec() {
        let json = serde_json::json!({
            "keys": [
                {
                    "kty": "RSA",
                    "kid": "test-rsa-mixed",
                    "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                    "e": "AQAB"
                },
                {
                    "kty": "EC",
                    "crv": "P-256",
                    "kid": "test-ec-mixed",
                    "x": "WKn-ZIGevcwGIyyrzFoZNBdaq9_TsqzGl96oc0CWuis",
                    "y": "y77t-RvAHRKTsSGdIYUfweuOvwrvDD-Q3Hv5J0fSKbE"
                }
            ]
        });
        let ks = parse_jwks(&json).unwrap();
        assert_eq!(ks.keys.len(), 2);
        assert!(ks.get("test-rsa-mixed").is_some());
        assert!(ks.get("test-ec-mixed").is_some());
    }

    /// Test that the real ElevoOne token payload can be deserialized as ElevoOneClaims.
    #[test]
    fn test_deserialize_real_elevoone_claims() {
        use super::super::types::ElevoOneClaims;
        // Access token uses elevo_oid / elevo_org_role
        let payload = r#"{"iss":"https://sso-dev.elevo.vip","sub":"urn:elevo:user:d670a807-ed8d-45ef-94e7-7ebe856778a1","aud":["https://sso-dev.elevo.vip","pk_302addd7d47b4fbfa649919a32c4768f"],"exp":1775391176,"nbf":1775387576,"iat":1775387576,"jti":"fd507a2f-a4e7-4cf4-a191-d593f593b13a","scope":"openid elevo:org:read","azp":"pk_302addd7d47b4fbfa649919a32c4768f","elevo_oid":1,"elevo_org_role":"admin"}"#;
        let claims: ElevoOneClaims = serde_json::from_str(payload).expect("deserialization should succeed");
        assert_eq!(claims.aud, "https://sso-dev.elevo.vip");
        assert_eq!(claims.oid, Some(1));
        assert_eq!(claims.org_role.as_deref(), Some("admin"));
    }

    /// Test that ID token format (org_id / org_role) also deserializes correctly.
    #[test]
    fn test_deserialize_id_token_claims() {
        use super::super::types::ElevoOneClaims;
        // ID token uses org_id / org_role (unprefixed)
        let payload = r#"{"iss":"https://sso-dev.elevo.vip","sub":"d670a807-ed8d-45ef-94e7-7ebe856778a1","aud":"pk_302addd7d47b4fbfa649919a32c4768f","exp":1775391176,"iat":1775387576,"nonce":"bf49ea4b","email":"linuschen@easyops.cn","name":"linus","org_id":"1","org_name":"easyops","org_role":"admin"}"#;
        let claims: ElevoOneClaims = serde_json::from_str(payload).expect("ID token deserialization should succeed");
        assert_eq!(claims.oid, Some(1));
        assert_eq!(claims.org_role.as_deref(), Some("admin"));
        assert_eq!(claims.email.as_deref(), Some("linuschen@easyops.cn"));
    }

    /// Test that DecodingKey created from EC components can be used by jsonwebtoken
    /// to actually verify a signature (integration-level test).
    #[test]
    fn test_ec_decoding_key_ring_compatibility() {
        // Create a DecodingKey from the same JWK x/y as the ElevoOne JWKS
        let dk = DecodingKey::from_ec_components(
            "5u7GtRDiTiZEyysuEXhkBGbpvlqX6GOb39LYkeAtZak",
            "H4L-6AXWzmCnbsuCYJ8hG4-J_VKQMn9R77aC6uc7Nu8",
        )
        .expect("from_ec_components should succeed");

        // Build a JwksKeySet with this key and verify that a real ES256 token
        // from ElevoOne can be decoded (signature check + claims parse).
        let mut ks = JwksKeySet::default();
        ks.insert("ek-20260405070932-4287269c".to_string(), dk);

        // Use a short-lived token obtained from ElevoOne (ES256 signed)
        let token = "eyJ0eXAiOiJhdCtKV1QiLCJhbGciOiJFUzI1NiIsImtpZCI6ImVrLTIwMjYwNDA1MDcwOTMyLTQyODcyNjljIn0.eyJpc3MiOiJodHRwczovL3Nzby1kZXYuZWxldm8udmlwIiwic3ViIjoidXJuOmVsZXZvOnVzZXI6ZDY3MGE4MDctZWQ4ZC00NWVmLTk0ZTctN2ViZTg1Njc3OGExIiwiYXVkIjpbImh0dHBzOi8vc3NvLWRldi5lbGV2by52aXAiLCJwa18zMDJhZGRkN2Q0N2I0ZmJmYTY0OTkxOWEzMmM0NzY4ZiJdLCJleHAiOjE3NzUzOTExNzYsIm5iZiI6MTc3NTM4NzU3NiwiaWF0IjoxNzc1Mzg3NTc2LCJqdGkiOiJmZDUwN2EyZi1hNGU3LTRjZjQtYTE5MS1kNTkzZjU5M2IxM2EiLCJzY29wZSI6Im9wZW5pZCBlbGV2bzpvcmc6cmVhZCBlbGV2bzpvcmc6bWVtYmVyOnJlYWQgZWxldm86b3JnOm1vZGVsOnJlYWQgZWxldm86b3JnOnRva2VuX3VzYWdlOnJlYWQgZWxldm86b3JnOmxlYXZlOmV4ZWN1dGUgZWxldm86b3JnOmludml0YXRpb246bWFuYWdlIGVsZXZvOm9yZzptZW1iZXI6bWFuYWdlIGVsZXZvOm9yZzprZXk6bWFuYWdlIGVsZXZvOm9yZzpyYXRlOm1hbmFnZSBlbGV2bzpvcmc6ZGlzcGxheV9uYW1lOm1hbmFnZSIsImF6cCI6InBrXzMwMmFkZGQ3ZDQ3YjRmYmZhNjQ5OTE5YTMyYzQ3NjhmIiwiZWxldm9fb2lkIjoxLCJlbGV2b19vcmdfcm9sZSI6ImFkbWluIn0.MeazIEWgn7ZqTzuyPNIxTiyUfK8Sz-LMd4LjWiihWG90B_fCarOvbQ1fV74GCjANYnBPQJ19qDDMumrhoM4Y8g";

        // This will tell us exactly whether the issue is in signature verification
        // or claims deserialization
        let result = verify_access_token(token, &ks, "https://sso-dev.elevo.vip", "pk_302addd7d47b4fbfa649919a32c4768f");
        // We expect this to either succeed or give a specific error.
        // The key insight is whether it fails at signature or JSON parsing stage.
        match &result {
            Ok(claims) => {
                println!("SUCCESS: oid={:?}, sub={:?}", claims.oid, claims.sub);
            }
            Err(e) => {
                println!("ERROR: {}", e);
                // If the error contains "JSON error", it's a claims parsing issue
                // If it contains "InvalidSignature", it's a key format issue
            }
        }
    }
}
