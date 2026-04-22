//! OIDC types — domain types for the OIDC service

use serde::{Deserialize, Serialize};

/// ElevoOne ID Token claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElevoOneClaims {
    /// `sub` claim — may be absent in client_credentials tokens (M2M flow).
    #[serde(default)]
    pub sub: Option<String>,
    /// OIDC `aud` claim — can be a single string or an array of strings.
    /// Always deserialized as a single string (first element if array).
    #[serde(deserialize_with = "deserialize_aud")]
    pub aud: String,
    pub exp: i64,
    pub iat: i64,
    #[serde(default)]
    pub iss: Option<String>,
    /// Organization identifier — ElevoOne uses `elevo_oid` in access tokens
    /// and `org_id` in ID tokens. Accept both via alias.
    #[serde(
        default,
        rename = "elevo_oid",
        alias = "org_id",
        deserialize_with = "deserialize_number_or_string"
    )]
    pub oid: Option<i64>,
    /// Organization role — ElevoOne uses `elevo_org_role` in access tokens
    /// and `org_role` in ID tokens. Accept both via alias.
    #[serde(default, rename = "elevo_org_role", alias = "org_role")]
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
    /// Helper enum that accepts either a string or an array of strings.
    /// Uses `#[serde(untagged)]` so serde tries each variant in order,
    /// avoiding `deserialize_any` which causes "trailing characters" errors
    /// when `aud` is an array inside a larger JSON object.
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum AudValue {
        Single(String),
        Multiple(Vec<String>),
    }

    match AudValue::deserialize(deserializer)? {
        AudValue::Single(s) => Ok(s),
        AudValue::Multiple(v) => Ok(v.first().cloned().unwrap_or_default()),
    }
}

/// Deserialize a value that may be a number or a numeric string (e.g. 1 or "1") as i64.
fn deserialize_number_or_string<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    /// Helper enum that accepts either a number or a string.
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Number(i64),
        Str(String),
        Null,
    }

    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Number(n) => Ok(Some(n)),
        NumOrStr::Str(s) => s.parse::<i64>().map(Some).map_err(|_| {
            serde::de::Error::invalid_value(
                serde::de::Unexpected::Str(&s),
                &"a numeric string",
            )
        }),
        NumOrStr::Null => Ok(None),
    }
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
