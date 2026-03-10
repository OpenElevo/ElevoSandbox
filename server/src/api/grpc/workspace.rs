//! gRPC WorkspaceService implementation

use std::sync::Arc;

use prost_types::Timestamp;
use tonic::{Request, Response, Status};
use tracing::debug;

use crate::domain::workspace::{CreateWorkspaceParams, StorageType};
use crate::error::Error;
use crate::proto::{
    workspace_service_server::WorkspaceService, CopyFileRequest, CopyFileResponse,
    CreateWorkspaceRequest, CreateWorkspaceResponse, DeleteFileRequest, DeleteFileResponse,
    DeleteWorkspaceRequest, DeleteWorkspaceResponse, FileInfo as ProtoFileInfo, GetFileInfoRequest,
    GetFileInfoResponse, GetWorkspaceRequest, GetWorkspaceResponse, ListFilesRequest,
    ListFilesResponse, ListWorkspacesRequest, ListWorkspacesResponse, MkdirRequest, MkdirResponse,
    MoveFileRequest, MoveFileResponse, ReadFileRequest, ReadFileResponse,
    RegisterNfsTransportRequest, RegisterNfsTransportResponse, UnregisterNfsTransportRequest,
    UnregisterNfsTransportResponse, Workspace as ProtoWorkspace, WriteFileRequest,
    WriteFileResponse,
};
use crate::service::remote_storage::RemoteStorageService;
use crate::service::workspace::WorkspaceService as WorkspaceServiceImpl;

/// Convert domain Error to gRPC Status
fn error_to_status(err: Error) -> Status {
    match &err {
        Error::WorkspaceNotFound(_)
        | Error::FileNotFound(_)
        | Error::SandboxNotFound(_)
        | Error::ProcessNotFound(_)
        | Error::PtyNotFound(_)
        | Error::TemplateNotFound(_) => Status::not_found(err.to_string()),
        Error::WorkspaceHasActiveSandboxes | Error::FileAlreadyExists(_) => {
            Status::already_exists(err.to_string())
        }
        Error::InvalidParameter(_)
        | Error::InvalidRequest(_)
        | Error::InvalidPath(_)
        | Error::IsADirectory(_)
        | Error::NotADirectory(_)
        | Error::DirectoryNotEmpty(_) => Status::invalid_argument(err.to_string()),
        Error::PermissionDenied(_) | Error::PathNotAllowed(_) => {
            Status::permission_denied(err.to_string())
        }
        _ => Status::internal(err.to_string()),
    }
}

/// Convert domain Workspace to proto Workspace
fn workspace_to_proto(ws: crate::domain::workspace::Workspace) -> ProtoWorkspace {
    let storage_config = serde_json::to_string(&ws.storage_config).unwrap_or_default();
    ProtoWorkspace {
        id: ws.id,
        name: ws.name,
        nfs_url: ws.nfs_url,
        metadata: ws.metadata,
        created_at: Some(Timestamp {
            seconds: ws.created_at.timestamp(),
            nanos: ws.created_at.timestamp_subsec_nanos() as i32,
        }),
        updated_at: Some(Timestamp {
            seconds: ws.updated_at.timestamp(),
            nanos: ws.updated_at.timestamp_subsec_nanos() as i32,
        }),
        storage_type: ws.storage_type.as_str().to_string(),
        storage_config,
    }
}

/// Convert service FileInfo to proto FileInfo
fn file_info_to_proto(info: crate::service::workspace::FileInfo) -> ProtoFileInfo {
    ProtoFileInfo {
        name: info.name,
        path: info.path,
        r#type: info.file_type,
        size: info.size,
        modified_at: info.modified_at.map(|dt| Timestamp {
            seconds: dt.timestamp(),
            nanos: dt.timestamp_subsec_nanos() as i32,
        }),
    }
}

/// gRPC WorkspaceService implementation
pub struct GrpcWorkspaceService {
    service: Arc<WorkspaceServiceImpl>,
    remote_storage_service: Arc<RemoteStorageService>,
}

impl GrpcWorkspaceService {
    pub fn new(
        service: Arc<WorkspaceServiceImpl>,
        remote_storage_service: Arc<RemoteStorageService>,
    ) -> Self {
        Self {
            service,
            remote_storage_service,
        }
    }
}

#[tonic::async_trait]
impl WorkspaceService for GrpcWorkspaceService {
    async fn create_workspace(
        &self,
        request: Request<CreateWorkspaceRequest>,
    ) -> Result<Response<CreateWorkspaceResponse>, Status> {
        let req = request.into_inner();
        debug!(name = ?req.name, "create_workspace");

        let storage_type = match req.storage_type.as_deref() {
            Some("remote") => Some(StorageType::Remote),
            Some("managed") | None => None,
            Some(other) => {
                return Err(Status::invalid_argument(format!(
                    "invalid storage_type: '{}', must be 'managed' or 'remote'",
                    other
                )));
            }
        };

        let params = CreateWorkspaceParams {
            name: req.name,
            storage_type,
            metadata: if req.metadata.is_empty() {
                None
            } else {
                Some(req.metadata)
            },
        };

        let workspace = self.service.create(params).await.map_err(error_to_status)?;

        Ok(Response::new(CreateWorkspaceResponse {
            workspace: Some(workspace_to_proto(workspace)),
        }))
    }

    async fn get_workspace(
        &self,
        request: Request<GetWorkspaceRequest>,
    ) -> Result<Response<GetWorkspaceResponse>, Status> {
        let req = request.into_inner();
        debug!(id = %req.id, "get_workspace");

        let workspace = self.service.get(&req.id).await.map_err(error_to_status)?;

        Ok(Response::new(GetWorkspaceResponse {
            workspace: Some(workspace_to_proto(workspace)),
        }))
    }

    async fn list_workspaces(
        &self,
        request: Request<ListWorkspacesRequest>,
    ) -> Result<Response<ListWorkspacesResponse>, Status> {
        let _req = request.into_inner();
        debug!("list_workspaces");

        let workspaces = self.service.list().await.map_err(error_to_status)?;
        let total = workspaces.len() as i32;

        Ok(Response::new(ListWorkspacesResponse {
            workspaces: workspaces.into_iter().map(workspace_to_proto).collect(),
            next_page_token: String::new(),
            total,
        }))
    }

    async fn delete_workspace(
        &self,
        request: Request<DeleteWorkspaceRequest>,
    ) -> Result<Response<DeleteWorkspaceResponse>, Status> {
        let req = request.into_inner();
        debug!(id = %req.id, "delete_workspace");

        self.service
            .delete(&req.id)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(DeleteWorkspaceResponse { success: true }))
    }

    // ==================== File Operations ====================

    async fn read_file(
        &self,
        request: Request<ReadFileRequest>,
    ) -> Result<Response<ReadFileResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "read_file");

        let content = self
            .service
            .read_file(&req.workspace_id, &req.path)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(ReadFileResponse { content }))
    }

    async fn write_file(
        &self,
        request: Request<WriteFileRequest>,
    ) -> Result<Response<WriteFileResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "write_file");

        self.service
            .write_file(&req.workspace_id, &req.path, &req.content)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(WriteFileResponse { success: true }))
    }

    async fn list_files(
        &self,
        request: Request<ListFilesRequest>,
    ) -> Result<Response<ListFilesResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "list_files");

        let files = self
            .service
            .list_files(&req.workspace_id, &req.path)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(ListFilesResponse {
            files: files.into_iter().map(file_info_to_proto).collect(),
        }))
    }

    async fn mkdir(
        &self,
        request: Request<MkdirRequest>,
    ) -> Result<Response<MkdirResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "mkdir");

        self.service
            .mkdir(&req.workspace_id, &req.path)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(MkdirResponse { success: true }))
    }

    async fn delete_file(
        &self,
        request: Request<DeleteFileRequest>,
    ) -> Result<Response<DeleteFileResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, recursive = req.recursive, "delete_file");

        self.service
            .delete_file(&req.workspace_id, &req.path, req.recursive)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(DeleteFileResponse { success: true }))
    }

    async fn move_file(
        &self,
        request: Request<MoveFileRequest>,
    ) -> Result<Response<MoveFileResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, source = %req.source, destination = %req.destination, "move_file");

        self.service
            .move_file(&req.workspace_id, &req.source, &req.destination)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(MoveFileResponse { success: true }))
    }

    async fn copy_file(
        &self,
        request: Request<CopyFileRequest>,
    ) -> Result<Response<CopyFileResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, source = %req.source, destination = %req.destination, "copy_file");

        self.service
            .copy_file(&req.workspace_id, &req.source, &req.destination)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(CopyFileResponse { success: true }))
    }

    async fn get_file_info(
        &self,
        request: Request<GetFileInfoRequest>,
    ) -> Result<Response<GetFileInfoResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "get_file_info");

        let file = self
            .service
            .get_file_info(&req.workspace_id, &req.path)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(GetFileInfoResponse {
            file: Some(file_info_to_proto(file)),
        }))
    }

    async fn register_nfs_transport(
        &self,
        request: Request<RegisterNfsTransportRequest>,
    ) -> Result<Response<RegisterNfsTransportResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            nfs_url = %req.nfs_url,
            "register_nfs_transport"
        );

        self.remote_storage_service
            .register_nfs_transport(&req.workspace_id, &req.nfs_url)
            .await
            .map_err(error_to_status)?;

        // Fetch updated workspace to return
        let workspace = self
            .service
            .get(&req.workspace_id)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(RegisterNfsTransportResponse {
            workspace: Some(workspace_to_proto(workspace)),
        }))
    }

    async fn unregister_nfs_transport(
        &self,
        request: Request<UnregisterNfsTransportRequest>,
    ) -> Result<Response<UnregisterNfsTransportResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            "unregister_nfs_transport"
        );

        self.remote_storage_service
            .unregister_nfs_transport(&req.workspace_id)
            .await
            .map_err(error_to_status)?;

        // Fetch updated workspace to return
        let workspace = self
            .service
            .get(&req.workspace_id)
            .await
            .map_err(error_to_status)?;

        Ok(Response::new(UnregisterNfsTransportResponse {
            workspace: Some(workspace_to_proto(workspace)),
        }))
    }
}
