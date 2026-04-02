//! OIDC types — domain types for the OIDC service

use serde::{Deserialize, Serialize};

/// ElevoOne ID Token claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevoOneClaims {
    pub sub: String,
    /// OIDC `aud` claim — can be a single string or an array of strings.
    /// Always deserialized as a single string (first element if array).
    #[serde(deserialize_with = "deserialize_aud")]
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(default)]
    pub iss: Option<String>,
    #[serde(default)]
    pub org_id: Option<i64>,
    #[serde(default)]
    pub org_role: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub picture: Option<String>,
    #[serde(default)]
    pub nonce: Option<String>,
}

fn deserialize_aud<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct AudVisitor;

    impl<'de> Visitor<'de> for AudVisitor {
        type Value = String;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a string or array of strings")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<String, A::Error> {
            let first: Option<String> = seq.next_element()?;
            Ok(first.unwrap_or_default())
        }
    }

    deserializer.deserialize_any(AudVisitor)
}

/// Token response from OIDC token endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub id_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub token_type: Option<String>,
}

/// OIDC configuration domain type
#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub jwks_refresh_interval_secs: u64,
    pub disable_password_login: bool,
    pub auto_create_tenant: bool,
}

/// OIDC errors
#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    #[error("OIDC not configured")]
    NotConfigured,

    #[error("OIDC disabled")]
    Disabled,

    #[error("circuit breaker tripped: too many failures")]
    CircuitBreakerTripped,

    #[error("invalid or expired token: {0}")]
    InvalidToken(String),

    #[error("token exchange failed: {0}")]
    TokenExchangeFailed(String),

    #[error("ID token verification failed: {0}")]
    IdTokenVerificationFailed(String),

    #[error("JWKS error: {0}")]
    JwksError(String),

    #[error("crypto error: {0}")]
    CryptoError(String),

    #[error("connection test failed: {0}")]
    ConnectionTestFailed(String),

    #[error("tenant not found for org_id: {0}")]
    TenantNotFound(i64),

    #[error("non-admin user (org_role={0})")]
    NotAdmin(String),

    #[error("{0}")]
    Internal(String),
}

impl From<base64::DecodeError> for OidcError {
    fn from(e: base64::DecodeError) -> Self {
        OidcError::CryptoError(format!("base64 decode: {}", e))
    }
}
