//! gRPC client wrapper for FileSystemService
//!
//! Provides a high-level async API for filesystem operations.

use std::time::Duration;

use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Request, Status};
use tracing::debug;

use workspace_proto::file_system_service_client::FileSystemServiceClient;
use workspace_proto::{
    FsCreateRequest, FsFileAttr, FsListDirRequest, FsMkdirRequest, FsReadAtRequest,
    FsReadLinkRequest, FsRemoveDirRequest, FsRemoveFileRequest, FsRenameFlags, FsRenameRequest,
    FsSetAttrRequest, FsStatFsRequest, FsStatFsResponse, FsStatRequest, FsSymlinkRequest,
    FsWriteAtRequest,
};

use fuse_core::backend::FuseDirEntry;

/// gRPC client for FileSystemService
#[derive(Clone)]
pub struct FileSystemRpcClient {
    client: FileSystemServiceClient<Channel>,
    token: String,
    workspace_id: String,
}

impl FileSystemRpcClient {
    /// Connect to the FileSystemService
    pub async fn connect(
        server: &str,
        workspace_id: String,
        token: String,
    ) -> Result<Self, tonic::transport::Error> {
        let endpoint = Endpoint::from_shared(server.to_string())?
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60));

        let channel = endpoint.connect().await?;
        let client = FileSystemServiceClient::new(channel);

        Ok(Self {
            client,
            token,
            workspace_id,
        })
    }

    /// Add authorization header to request (skipped if token is empty)
    fn authorize<T>(&self, mut request: Request<T>) -> Request<T> {
        if !self.token.is_empty() {
            let token_value = format!("Bearer {}", self.token);
            if let Ok(value) = MetadataValue::try_from(&token_value) {
                request.metadata_mut().insert("authorization", value);
            }
        }
        request
    }

    /// Get file/directory metadata
    pub async fn stat(&self, path: &str) -> Result<FsFileAttr, Status> {
        debug!(path = %path, "rpc: stat");
        let request = self.authorize(Request::new(FsStatRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
        }));

        let response = self.client.clone().stat(request).await?;
        response
            .into_inner()
            .attr
            .ok_or_else(|| Status::internal("missing attr in response"))
    }

    /// List directory contents
    pub async fn list_dir(&self, path: &str) -> Result<Vec<FuseDirEntry>, Status> {
        debug!(path = %path, "rpc: list_dir");
        let request = self.authorize(Request::new(FsListDirRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
        }));

        let mut stream = self.client.clone().list_dir(request).await?.into_inner();
        let mut entries = Vec::new();

        use tokio_stream::StreamExt;
        while let Some(response) = stream.next().await {
            let response = response?;
            for entry in response.entries {
                entries.push(FuseDirEntry {
                    name: entry.name,
                    attr: entry.attr,
                });
            }
        }

        Ok(entries)
    }

    /// Read file at specific offset
    pub async fn read_at(&self, path: &str, offset: u64, size: u32) -> Result<Vec<u8>, Status> {
        debug!(path = %path, offset = offset, size = size, "rpc: read_at");
        let request = self.authorize(Request::new(FsReadAtRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
            offset,
            size,
        }));

        let response = self.client.clone().read_at(request).await?;
        Ok(response.into_inner().data)
    }

    /// Write file at specific offset
    pub async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> Result<u64, Status> {
        debug!(path = %path, offset = offset, size = data.len(), "rpc: write_at");
        let request = self.authorize(Request::new(FsWriteAtRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
            offset,
            data: data.to_vec(),
        }));

        let response = self.client.clone().write_at(request).await?;
        Ok(response.into_inner().bytes_written)
    }

    /// Create a new file
    pub async fn create(
        &self,
        path: &str,
        mode: u32,
        exclusive: bool,
    ) -> Result<FsFileAttr, Status> {
        debug!(path = %path, mode = mode, exclusive = exclusive, "rpc: create");
        let request = self.authorize(Request::new(FsCreateRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
            mode,
            exclusive,
        }));

        let response = self.client.clone().create(request).await?;
        response
            .into_inner()
            .attr
            .ok_or_else(|| Status::internal("missing attr in response"))
    }

    /// Create a directory
    pub async fn mkdir(&self, path: &str, mode: u32) -> Result<FsFileAttr, Status> {
        debug!(path = %path, mode = mode, "rpc: mkdir");
        let request = self.authorize(Request::new(FsMkdirRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
            mode,
        }));

        let response = self.client.clone().mkdir(request).await?;
        response
            .into_inner()
            .attr
            .ok_or_else(|| Status::internal("missing attr in response"))
    }

    /// Remove a file
    pub async fn remove_file(&self, path: &str) -> Result<(), Status> {
        debug!(path = %path, "rpc: remove_file");
        let request = self.authorize(Request::new(FsRemoveFileRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
        }));

        self.client.clone().remove_file(request).await?;
        Ok(())
    }

    /// Remove a directory
    pub async fn remove_dir(&self, path: &str) -> Result<(), Status> {
        debug!(path = %path, "rpc: remove_dir");
        let request = self.authorize(Request::new(FsRemoveDirRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
            recursive: false, // FUSE rmdir semantics: directory must be empty
        }));

        self.client.clone().remove_dir(request).await?;
        Ok(())
    }

    /// Rename/move a file or directory
    pub async fn rename_with_flags(
        &self,
        old_path: &str,
        new_path: &str,
        flags: FsRenameFlags,
    ) -> Result<(), Status> {
        debug!(old_path = %old_path, new_path = %new_path, flags = ?flags, "rpc: rename");
        let request = self.authorize(Request::new(FsRenameRequest {
            workspace_id: self.workspace_id.clone(),
            old_path: old_path.to_string(),
            new_path: new_path.to_string(),
            flags: flags.into(),
        }));

        self.client.clone().rename(request).await?;
        Ok(())
    }

    /// Set file attributes
    pub async fn set_attr(
        &self,
        path: &str,
        size: Option<u64>,
        mode: Option<u32>,
        atime: Option<prost_types::Timestamp>,
        mtime: Option<prost_types::Timestamp>,
    ) -> Result<FsFileAttr, Status> {
        debug!(path = %path, size = ?size, mode = ?mode, "rpc: set_attr");
        let request = self.authorize(Request::new(FsSetAttrRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
            size,
            mode,
            // uid/gid are not used by client - remote workspace uid/gid semantics differ
            uid: None,
            gid: None,
            atime,
            mtime,
        }));

        let response = self.client.clone().set_attr(request).await?;
        response
            .into_inner()
            .attr
            .ok_or_else(|| Status::internal("missing attr in response"))
    }

    /// Create a symbolic link
    pub async fn symlink(&self, link_path: &str, target: &str) -> Result<FsFileAttr, Status> {
        debug!(link_path = %link_path, target = %target, "rpc: symlink");
        let request = self.authorize(Request::new(FsSymlinkRequest {
            workspace_id: self.workspace_id.clone(),
            link_path: link_path.to_string(),
            target: target.to_string(),
        }));

        let response = self.client.clone().symlink(request).await?;
        response
            .into_inner()
            .attr
            .ok_or_else(|| Status::internal("missing attr in response"))
    }

    /// Read symbolic link target
    pub async fn read_link(&self, path: &str) -> Result<String, Status> {
        debug!(path = %path, "rpc: read_link");
        let request = self.authorize(Request::new(FsReadLinkRequest {
            workspace_id: self.workspace_id.clone(),
            path: path.to_string(),
        }));

        let response = self.client.clone().read_link(request).await?;
        Ok(response.into_inner().target)
    }

    /// Get filesystem statistics
    pub async fn stat_fs(&self) -> Result<FsStatFsResponse, Status> {
        debug!("rpc: stat_fs");
        let request = self.authorize(Request::new(FsStatFsRequest {
            workspace_id: self.workspace_id.clone(),
        }));

        let response = self.client.clone().stat_fs(request).await?;
        Ok(response.into_inner())
    }
}
