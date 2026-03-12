//! Authentication and authorization domain models

use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Authenticated identity
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Identity {
    /// Admin user (authenticated via JWT)
    Admin { session_id: Uuid },
    /// Tenant (authenticated via API Key)
    Tenant { id: Uuid, name: String },
}

impl Identity {
    /// Check if this identity is an admin
    pub fn is_admin(&self) -> bool {
        matches!(self, Identity::Admin { .. })
    }

    /// Get the tenant ID if this is a tenant identity
    pub fn tenant_id(&self) -> Option<Uuid> {
        match self {
            Identity::Tenant { id, .. } => Some(*id),
            Identity::Admin { .. } => None,
        }
    }

    /// Get a display name for logging
    pub fn display_name(&self) -> String {
        match self {
            Identity::Admin { session_id } => format!("admin({})", session_id),
            Identity::Tenant { id, name } => format!("tenant({}:{})", name, id),
        }
    }
}

/// Authentication context injected into request extensions
#[derive(Debug, Clone)]
pub struct AuthContext {
    pub identity: Identity,
    pub ip_address: Option<IpAddr>,
}

impl AuthContext {
    pub fn is_admin(&self) -> bool {
        self.identity.is_admin()
    }

    pub fn tenant_id(&self) -> Option<Uuid> {
        self.identity.tenant_id()
    }

    /// Check if this context represents the owner of a given namespace
    pub fn is_namespace_owner(&self, namespace_id: &Uuid) -> bool {
        match &self.identity {
            Identity::Admin { .. } => true,
            Identity::Tenant { id, .. } => id == namespace_id,
        }
    }

    /// Require admin identity, return error if not
    pub fn require_admin(&self) -> Result<(), AuthError> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(AuthError::Forbidden(
                "admin access required".to_string(),
            ))
        }
    }

    /// Require namespace ownership or admin
    pub fn require_namespace_access(
        &self,
        namespace_id: &Uuid,
    ) -> Result<(), AuthError> {
        if self.is_namespace_owner(namespace_id) {
            Ok(())
        } else {
            Err(AuthError::Forbidden(
                "namespace access denied".to_string(),
            ))
        }
    }
}

/// JWT claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Subject (always "admin")
    pub sub: String,
    /// Session ID
    pub session_id: Uuid,
    /// Login IP address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_ip: Option<String>,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// Expiration (Unix timestamp)
    pub exp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin_ctx() -> AuthContext {
        AuthContext {
            identity: Identity::Admin {
                session_id: Uuid::nil(),
            },
            ip_address: None,
        }
    }

    fn tenant_ctx(id: Uuid) -> AuthContext {
        AuthContext {
            identity: Identity::Tenant {
                id,
                name: "test-tenant".to_string(),
            },
            ip_address: Some("127.0.0.1".parse().unwrap()),
        }
    }

    #[test]
    fn test_admin_identity() {
        let ctx = admin_ctx();
        assert!(ctx.is_admin());
        assert!(ctx.tenant_id().is_none());
        assert!(ctx.require_admin().is_ok());
    }

    #[test]
    fn test_tenant_identity() {
        let id = Uuid::new_v4();
        let ctx = tenant_ctx(id);
        assert!(!ctx.is_admin());
        assert_eq!(ctx.tenant_id(), Some(id));
        assert!(ctx.require_admin().is_err());
    }

    #[test]
    fn test_namespace_ownership_admin_always_owner() {
        let ctx = admin_ctx();
        let any_ns = Uuid::new_v4();
        assert!(ctx.is_namespace_owner(&any_ns));
        assert!(ctx.require_namespace_access(&any_ns).is_ok());
    }

    #[test]
    fn test_namespace_ownership_tenant_own_ns() {
        let tenant_id = Uuid::new_v4();
        let ctx = tenant_ctx(tenant_id);
        assert!(ctx.is_namespace_owner(&tenant_id));
        assert!(ctx.require_namespace_access(&tenant_id).is_ok());
    }

    #[test]
    fn test_namespace_ownership_tenant_other_ns() {
        let tenant_id = Uuid::new_v4();
        let other_ns = Uuid::new_v4();
        let ctx = tenant_ctx(tenant_id);
        assert!(!ctx.is_namespace_owner(&other_ns));
        assert!(ctx.require_namespace_access(&other_ns).is_err());
    }

    #[test]
    fn test_identity_display_name() {
        let admin = Identity::Admin {
            session_id: Uuid::nil(),
        };
        assert!(admin.display_name().starts_with("admin("));

        let tenant = Identity::Tenant {
            id: Uuid::nil(),
            name: "acme".to_string(),
        };
        assert!(tenant.display_name().contains("acme"));
    }
}

/// Authentication errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum AuthError {
    #[error("missing authorization header")]
    MissingToken,

    #[error("invalid token: {0}")]
    InvalidToken(String),

    #[error("token expired")]
    TokenExpired,

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("tenant is deactivated")]
    TenantDeactivated,

    #[error("api key revoked or expired")]
    ApiKeyInvalid,

    #[error("internal error: {0}")]
    Internal(String),
}
