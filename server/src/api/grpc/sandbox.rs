//! gRPC SandboxService implementation

use std::sync::Arc;

use prost_types::Timestamp;
use tonic::{Request, Response, Status};
use tracing::debug;

use crate::domain::sandbox::{
    CreateSandboxParams, SandboxState as DomainSandboxState, Sandbox as DomainSandbox,
};
use crate::error::Error;
use crate::proto::{
    sandbox_service_server::SandboxService, CreateSandboxRequest, CreateSandboxResponse,
    DeleteSandboxRequest, DeleteSandboxResponse, GetSandboxRequest, GetSandboxResponse,
    ListSandboxesRequest, ListSandboxesResponse, Sandbox as ProtoSandbox,
    SandboxState as ProtoSandboxState,
};
use crate::service::sandbox::SandboxService as SandboxServiceImpl;

/// Convert domain Error to gRPC Status
fn error_to_status(err: Error) -> Status {
    match &err {
        Error::SandboxNotFound(_) => Status::not_found(err.to_string()),
        Error::WorkspaceNotFound(_) => Status::not_found(err.to_string()),
        Error::InvalidSandboxState { .. } => Status::failed_precondition(err.to_string()),
        Error::AgentNotConnected(_) => Status::unavailable(err.to_string()),
        Error::InvalidParameter(_) | Error::InvalidRequest(_) => {
            Status::invalid_argument(err.to_string())
        }
        _ => Status::internal(err.to_string()),
    }
}

/// Convert domain SandboxState to proto SandboxState
fn state_to_proto(state: DomainSandboxState) -> i32 {
    match state {
        DomainSandboxState::Starting => ProtoSandboxState::Starting as i32,
        DomainSandboxState::Running => ProtoSandboxState::Running as i32,
        DomainSandboxState::Stopping => ProtoSandboxState::Stopping as i32,
        DomainSandboxState::Stopped => ProtoSandboxState::Stopped as i32,
        DomainSandboxState::Error => ProtoSandboxState::Error as i32,
    }
}

/// Convert proto SandboxState to domain SandboxState
fn state_from_proto(state: i32) -> Option<DomainSandboxState> {
    match ProtoSandboxState::try_from(state) {
        Ok(ProtoSandboxState::Starting) => Some(DomainSandboxState::Starting),
        Ok(ProtoSandboxState::Running) => Some(DomainSandboxState::Running),
        Ok(ProtoSandboxState::Stopping) => Some(DomainSandboxState::Stopping),
        Ok(ProtoSandboxState::Stopped) => Some(DomainSandboxState::Stopped),
        Ok(ProtoSandboxState::Error) => Some(DomainSandboxState::Error),
        _ => None,
    }
}

/// Convert domain Sandbox to proto Sandbox
fn sandbox_to_proto(sb: DomainSandbox) -> ProtoSandbox {
    ProtoSandbox {
        id: sb.id,
        workspace_id: sb.workspace_id,
        name: sb.name,
        template: sb.template,
        state: state_to_proto(sb.state),
        env: sb.env,
        metadata: sb.metadata,
        created_at: Some(Timestamp {
            seconds: sb.created_at.timestamp(),
            nanos: sb.created_at.timestamp_subsec_nanos() as i32,
        }),
        updated_at: Some(Timestamp {
            seconds: sb.updated_at.timestamp(),
            nanos: sb.updated_at.timestamp_subsec_nanos() as i32,
        }),
        timeout: sb.timeout,
        error_message: sb.error_message,
    }
}

/// gRPC SandboxService implementation
pub struct GrpcSandboxService {
    service: Arc<SandboxServiceImpl>,
}

impl GrpcSandboxService {
    pub fn new(service: Arc<SandboxServiceImpl>) -> Self {
        Self { service }
    }
}

#[tonic::async_trait]
impl SandboxService for GrpcSandboxService {
    async fn create_sandbox(
        &self,
        request: Request<CreateSandboxRequest>,
    ) -> Result<Response<CreateSandboxResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, template = ?req.template, "create_sandbox");

        let params = CreateSandboxParams {
            workspace_id: req.workspace_id,
            template: req.template,
            name: req.name,
            env: if req.env.is_empty() {
                None
            } else {
                Some(req.env)
            },
            metadata: if req.metadata.is_empty() {
                None
            } else {
                Some(req.metadata)
            },
            timeout: req.timeout,
        };

        let sandbox = self.service.create(params).await.map_err(error_to_status)?;

        Ok(Response::new(CreateSandboxResponse {
            sandbox: Some(sandbox_to_proto(sandbox)),
        }))
    }

    async fn get_sandbox(
        &self,
        request: Request<GetSandboxRequest>,
    ) -> Result<Response<GetSandboxResponse>, Status> {
        let req = request.into_inner();
        debug!(id = %req.id, "get_sandbox");

        let sandbox = self.service.get(&req.id).await.map_err(error_to_status)?;

        Ok(Response::new(GetSandboxResponse {
            sandbox: Some(sandbox_to_proto(sandbox)),
        }))
    }

    async fn list_sandboxes(
        &self,
        request: Request<ListSandboxesRequest>,
    ) -> Result<Response<ListSandboxesResponse>, Status> {
        let req = request.into_inner();
        debug!(state = ?req.state, "list_sandboxes");

        let state_filter = req.state.and_then(state_from_proto);
        let sandboxes = self
            .service
            .list(state_filter)
            .await
            .map_err(error_to_status)?;
        let total = sandboxes.len() as i32;

        Ok(Response::new(ListSandboxesResponse {
            sandboxes: sandboxes.into_iter().map(sandbox_to_proto).collect(),
            next_page_token: String::new(),
            total,
        }))
    }

    async fn delete_sandbox(
        &self,
        request: Request<DeleteSandboxRequest>,
    ) -> Result<Response<DeleteSandboxResponse>, Status> {
        let req = request.into_inner();
        debug!(id = %req.id, force = req.force, "delete_sandbox");

        self.service
            .delete(&req.id, req.force)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(DeleteSandboxResponse { success: true }))
    }
}
