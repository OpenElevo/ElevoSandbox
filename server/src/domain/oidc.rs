//! OIDC domain types

use serde::{Deserialize, Serialize};

/// OIDC public configuration (exposed to login page)
#[derive(Debug, Clone, Serialize)]
pub struct OidcPublicConfig {
    pub enabled: bool,
    pub disable_password_login: bool,
}

/// OIDC authorize response
#[derive(Debug, Clone, Serialize)]
pub struct OidcAuthorizeResponse {
    pub authorize_url: String,
}

/// OIDC user info extracted from ID token
#[derive(Debug, Clone, Serialize)]
pub struct OidcUserInfo {
    pub name: String,
    pub email: Option<String>,
    pub picture: Option<String>,
    pub is_admin: bool,
}

/// OIDC session response (returned after session_code exchange)
#[derive(Debug, Clone, Serialize)]
pub struct OidcSessionResponse {
    pub token: String,
    pub user: OidcUserInfo,
}

/// OIDC refresh response
#[derive(Debug, Clone, Serialize)]
pub struct OidcRefreshResponse {
    pub success: bool,
}

/// OIDC logout response
#[derive(Debug, Clone, Serialize)]
pub struct OidcLogoutResponse {
    pub idp_logout_url: Option<String>,
}

/// OIDC config update params (from admin UI)
#[derive(Debug, Clone, Deserialize)]
pub struct OidcConfigUpdateParams {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    pub redirect_uri: String,
    #[serde(default = "default_jwks_interval")]
    pub jwks_refresh_interval_secs: i32,
    pub disable_password_login: bool,
    pub auto_create_tenant: bool,
}

fn default_jwks_interval() -> i32 {
    3600
}

/// OIDC config display (for admin settings page)
#[derive(Debug, Clone, Serialize)]
pub struct OidcConfigDisplay {
    pub enabled: bool,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String, // masked as "••••••••" or empty
    pub redirect_uri: String,
    pub jwks_refresh_interval_secs: i32,
    pub disable_password_login: bool,
    pub auto_create_tenant: bool,
}

/// OIDC connection test response
#[derive(Debug, Clone, Serialize)]
pub struct OidcTestResponse {
    pub success: bool,
    pub message: String,
}
