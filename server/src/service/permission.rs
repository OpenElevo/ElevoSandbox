//! Centralized permission checking service

use uuid::Uuid;

use crate::domain::auth::AuthContext;
use crate::domain::permission::PermissionLevel;
use crate::error::Error;
use crate::infra::share_permission_repository::SharePermissionRepository;
use crate::infra::share_repository::ShareRepository;

#[derive(Clone)]
pub struct PermissionService {
    share_repo: ShareRepository,
    permission_repo: SharePermissionRepository,
}

impl PermissionService {
    pub fn new(
        share_repo: ShareRepository,
        permission_repo: SharePermissionRepository,
    ) -> Self {
        Self {
            share_repo,
            permission_repo,
        }
    }

    /// Check if the auth context has the required permission on a share.
    ///
    /// Rules:
    /// 1. Admin has unlimited access
    /// 2. Owner automatically has admin permission (not stored in DB)
    /// 3. Public shares: all active tenants have implicit read
    /// 4. Private shares with no permission → NOT_FOUND (hide existence)
    /// 5. Public shares with insufficient permission → FORBIDDEN
    pub async fn check_share_permission(
        &self,
        auth: &AuthContext,
        share_id: &str,
        required: PermissionLevel,
    ) -> Result<(), Error> {
        // Admin bypasses all checks
        if auth.is_admin() {
            return Ok(());
        }

        let tenant_id = auth.tenant_id().ok_or_else(|| {
            Error::InvalidRequest("Authentication required".into())
        })?;

        let share = self.share_repo.get_share(share_id).await?;

        // Owner has implicit admin
        if share.owner_tenant_id == tenant_id.to_string() {
            return Ok(());
        }

        // Check explicit permission
        let explicit_perm = self
            .permission_repo
            .get_permission(share_id, &tenant_id.to_string())
            .await?;

        if let Some(perm) = explicit_perm {
            if perm.includes(&required) {
                return Ok(());
            }
            // Has permission but insufficient level
            return Err(Error::InvalidRequest(
                "Insufficient permission".into(),
            ));
        }

        // No explicit permission — check visibility
        let is_public = share.visibility
            == crate::domain::share::Visibility::Public;

        if is_public && required == PermissionLevel::Read {
            // Public shares grant implicit read to all tenants
            return Ok(());
        }

        if is_public {
            // Public share but needs more than read
            Err(Error::InvalidRequest(
                "Insufficient permission".into(),
            ))
        } else {
            // Private share with no permission — hide existence
            Err(Error::WorkspaceNotFound(format!(
                "Share {} not found",
                share_id
            )))
        }
    }

    /// Check if the auth context is the namespace owner or admin
    pub fn check_namespace_ownership(
        auth: &AuthContext,
        namespace_id: &str,
    ) -> Result<(), Error> {
        if auth.is_admin() {
            return Ok(());
        }

        let uuid = Uuid::parse_str(namespace_id).map_err(|_| {
            Error::InvalidParameter("Invalid namespace ID".into())
        })?;

        if auth.is_namespace_owner(&uuid) {
            Ok(())
        } else {
            Err(Error::InvalidRequest("Namespace access denied".into()))
        }
    }
}
