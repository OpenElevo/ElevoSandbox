//! JWT utility functions shared between HTTP and gRPC auth

/// Extract the `alg` field from a JWT header without full decoding.
/// Returns None if the token is malformed or not a valid JWT.
pub fn extract_jwt_alg(token: &str) -> Option<String> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    // Decode the header (first part) from base64url
    use base64::Engine;
    let header_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0])
        .ok()?;

    let header: serde_json::Value = serde_json::from_slice(&header_bytes).ok()?;
    header.get("alg")?.as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_alg_hs256() {
        // Manually construct a JWT-like header with HS256
        use base64::Engine;
        let header = serde_json::json!({"alg": "HS256", "typ": "JWT"});
        let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).unwrap());
        // Actually use the real header
        let token = format!("{}.eyBzdWIiOiIxMjM0NTY3ODkwIn0.sig", header_b64);
        assert_eq!(extract_jwt_alg(&token), Some("HS256".to_string()));
    }

    #[test]
    fn test_extract_alg_rs256() {
        use base64::Engine;
        let header = serde_json::json!({"alg": "RS256", "typ": "JWT", "kid": "key-1"});
        let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).unwrap());
        let token = format!("{}.payload.signature", header_b64);
        assert_eq!(extract_jwt_alg(&token), Some("RS256".to_string()));
    }

    #[test]
    fn test_extract_alg_malformed() {
        assert_eq!(extract_jwt_alg("not-a-jwt"), None);
        assert_eq!(extract_jwt_alg(""), None);
        assert_eq!(extract_jwt_alg("only.two"), None);
    }

    #[test]
    fn test_extract_alg_no_alg_field() {
        use base64::Engine;
        let header = serde_json::json!({"typ": "JWT"});
        let header_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(serde_json::to_vec(&header).unwrap());
        let token = format!("{}.payload.signature", header_b64);
        assert_eq!(extract_jwt_alg(&token), None);
    }
}
