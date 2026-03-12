//! Audit log service — fire-and-forget async logging

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
        resource_id: &str,
        resource_name: &str,
        detail: serde_json::Value,
    ) {
        let (actor_type, actor_id) = match &auth.identity {
            crate::domain::auth::Identity::Admin { .. } => {
                ("admin".to_string(), None)
            }
            crate::domain::auth::Identity::Tenant { id, .. } => {
                ("tenant".to_string(), Some(*id))
            }
        };

        let resource_uuid = Uuid::parse_str(resource_id).unwrap_or(Uuid::nil());
        let ip_address = auth.ip_address.map(|ip| ip.to_string());

        let params = CreateAuditLogParams {
            actor_type,
            actor_id,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: resource_uuid,
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
}
