//! Audit log service — fire-and-forget async logging

use std::net::IpAddr;
use uuid::Uuid;

use crate::domain::audit::CreateAuditLogParams;
use crate::domain::auth::AuthContext;
use crate::infra::audit_repository::AuditRepository;

#[derive(Clone)]
pub struct AuditService {
    repo: AuditRepository,
}

impl AuditService {
    pub fn new(repo: AuditRepository) -> Self {
        Self { repo }
    }

    /// Log an audit event asynchronously (fire-and-forget)
    pub fn log(
        &self,
        auth: &AuthContext,
        action: &str,
        resource_type: &str,
        resource_id: Uuid,
        resource_name: &str,
        detail: serde_json::Value,
    ) {
        let (actor_type, actor_id) = match &auth.identity {
            crate::domain::auth::Identity::Admin { .. } => ("admin".to_string(), None),
            crate::domain::auth::Identity::Tenant { id, .. } => ("tenant".to_string(), Some(*id)),
        };

        let ip_address = auth.ip_address.map(|ip| ip.to_string());

        let params = CreateAuditLogParams {
            actor_type,
            actor_id,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            resource_name: resource_name.to_string(),
            detail,
            ip_address,
        };

        let repo = self.repo.clone();
        tokio::spawn(async move {
            if let Err(e) = repo.create(params).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        });
    }

    /// Log an anonymous audit event (no AuthContext required, e.g. OIDC login failures)
    pub fn log_anonymous(
        &self,
        action: &str,
        resource_type: &str,
        resource_id: Uuid,
        resource_name: &str,
        detail: serde_json::Value,
        ip_address: Option<IpAddr>,
    ) {
        let params = CreateAuditLogParams {
            actor_type: "anonymous".to_string(),
            actor_id: None,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id,
            resource_name: resource_name.to_string(),
            detail,
            ip_address: ip_address.map(|ip| ip.to_string()),
        };

        let repo = self.repo.clone();
        tokio::spawn(async move {
            if let Err(e) = repo.create(params).await {
                tracing::warn!("Failed to write audit log: {}", e);
            }
        });
    }
}
