//! gRPC FileSystemService implementation
//!
//! Provides POSIX-like filesystem operations for FUSE clients via gRPC.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use futures::Stream;
use prost_types::Timestamp;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};
use tonic_types::{ErrorDetails, StatusExt};
use tracing::debug;

use crate::infra::storage::{FileStat, FileType, StorageBackend, StorageError};
use crate::proto::{
    file_system_service_server::FileSystemService, fs_write_file_request::Payload, FsCreateRequest,
    FsCreateResponse, FsDirEntry, FsFileAttr, FsFileType, FsListDirRequest, FsListDirResponse,
    FsMkdirRequest, FsMkdirResponse, FsReadAtRequest, FsReadAtResponse, FsReadFileRequest,
    FsReadFileResponse, FsReadLinkRequest, FsReadLinkResponse, FsRemoveDirRequest,
    FsRemoveDirResponse, FsRemoveFileRequest, FsRemoveFileResponse, FsRenameFlags, FsRenameRequest,
    FsRenameResponse, FsSetAttrRequest, FsSetAttrResponse, FsStatFsRequest, FsStatFsResponse,
    FsStatRequest, FsStatResponse, FsSymlinkRequest, FsSymlinkResponse, FsWriteAtRequest,
    FsWriteAtResponse, FsWriteFileRequest, FsWriteFileResponse,
};

/// Default chunk size for streaming file reads (64KB)
const DEFAULT_CHUNK_SIZE: usize = 64 * 1024;

/// FileSystemService implementation
pub struct FileSystemServiceImpl {
    storage: Arc<dyn StorageBackend>,
}

impl FileSystemServiceImpl {
    /// Create a new FileSystemService instance
    pub fn new(storage: Arc<dyn StorageBackend>) -> Self {
        Self { storage }
    }
}

/// Convert StorageError to gRPC Status with structured error details
///
/// Uses tonic-types ErrorDetails to pass errno in a structured way,
/// allowing clients to extract the exact POSIX error code.
fn storage_error_to_status(err: StorageError) -> Status {
    let (code, errno, message) = match &err {
        StorageError::NotFound(path) => (
            tonic::Code::NotFound,
            libc::ENOENT,
            format!("not found: {}", path),
        ),
        StorageError::AlreadyExists(path) => (
            tonic::Code::AlreadyExists,
            libc::EEXIST,
            format!("already exists: {}", path),
        ),
        StorageError::IsADirectory(path) => (
            tonic::Code::FailedPrecondition,
            libc::EISDIR,
            format!("is a directory: {}", path),
        ),
        StorageError::NotADirectory(path) => (
            tonic::Code::FailedPrecondition,
            libc::ENOTDIR,
            format!("not a directory: {}", path),
        ),
        StorageError::NotAFile(path) => (
            tonic::Code::FailedPrecondition,
            libc::EISDIR,
            format!("not a file: {}", path),
        ),
        StorageError::DirectoryNotEmpty(path) => (
            tonic::Code::FailedPrecondition,
            libc::ENOTEMPTY,
            format!("directory not empty: {}", path),
        ),
        StorageError::PermissionDenied(path) => (
            tonic::Code::PermissionDenied,
            libc::EACCES,
            format!("permission denied: {}", path),
        ),
        StorageError::PathTraversalDenied(path) => (
            tonic::Code::PermissionDenied,
            libc::EACCES,
            format!("path traversal denied: {}", path),
        ),
        StorageError::NotSupported(op) => (
            tonic::Code::Unimplemented,
            libc::ENOSYS,
            format!("operation not supported: {}", op),
        ),
        StorageError::Io { path, source } => {
            // Try to extract errno from the IO error
            let errno = source.raw_os_error().unwrap_or(libc::EIO);
            (
                tonic::Code::Internal,
                errno,
                format!("I/O error on {}: {}", path, source),
            )
        }
        StorageError::Internal(msg) => (tonic::Code::Internal, libc::EIO, msg.clone()),
    };

    // Build structured error details with errno
    let mut metadata = HashMap::new();
    metadata.insert("errno".to_string(), errno.to_string());

    let error_details = ErrorDetails::with_error_info(
        format!("ERRNO_{}", errno),
        "workspace.v1.FileSystemService",
        metadata,
    );

    Status::with_error_details(code, message, error_details)
}

/// Convert FileStat to proto FsFileAttr
fn file_stat_to_attr(stat: &FileStat) -> FsFileAttr {
    FsFileAttr {
        file_type: match stat.file_type {
            FileType::File => FsFileType::File.into(),
            FileType::Directory => FsFileType::Directory.into(),
            FileType::Symlink => FsFileType::Symlink.into(),
        },
        size: stat.size,
        mode: stat.mode,
        uid: stat.uid,
        gid: stat.gid,
        atime: stat.accessed_at.map(datetime_to_timestamp),
        mtime: stat.modified_at.map(datetime_to_timestamp),
        ctime: stat.created_at.map(datetime_to_timestamp),
        // Default values for nlink, blksize, blocks
        nlink: if stat.file_type == FileType::Directory {
            2
        } else {
            1
        },
        blksize: 4096,
        blocks: stat.size.div_ceil(512), // Round up to 512-byte blocks
    }
}

/// Convert DateTime<Utc> to protobuf Timestamp
fn datetime_to_timestamp(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: dt.timestamp_subsec_nanos() as i32,
    }
}

/// Convert protobuf Timestamp to DateTime<Utc>
fn timestamp_to_datetime(ts: &Timestamp) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(ts.seconds, ts.nanos as u32).single()
}

#[tonic::async_trait]
impl FileSystemService for FileSystemServiceImpl {
    /// Get file/directory metadata
    async fn stat(
        &self,
        request: Request<FsStatRequest>,
    ) -> Result<Response<FsStatResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "stat");

        let stat = self
            .storage
            .stat(&req.workspace_id, &req.path)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsStatResponse {
            attr: Some(file_stat_to_attr(&stat)),
        }))
    }

    type ReadFileStream = Pin<Box<dyn Stream<Item = Result<FsReadFileResponse, Status>> + Send>>;

    /// Stream read file contents
    async fn read_file(
        &self,
        request: Request<FsReadFileRequest>,
    ) -> Result<Response<Self::ReadFileStream>, Status> {
        let req = request.into_inner();
        let chunk_size = if req.chunk_size > 0 {
            req.chunk_size as usize
        } else {
            DEFAULT_CHUNK_SIZE
        };

        debug!(
            workspace_id = %req.workspace_id,
            path = %req.path,
            chunk_size = chunk_size,
            "read_file"
        );

        // Read the entire file (for simplicity; could be optimized for very large files)
        let data = self
            .storage
            .read_file(&req.workspace_id, &req.path)
            .await
            .map_err(storage_error_to_status)?;

        let (tx, rx) = mpsc::channel(4);

        // Stream chunks
        tokio::spawn(async move {
            let chunks: Vec<_> = data.chunks(chunk_size).collect();
            let total_chunks = chunks.len();
            for (i, chunk) in chunks.into_iter().enumerate() {
                let is_last = i == total_chunks - 1;
                if tx
                    .send(Ok(FsReadFileResponse {
                        data: chunk.to_vec(),
                        eof: is_last,
                    }))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            // If file is empty, send an empty chunk with eof=true
            if total_chunks == 0 {
                let _ = tx
                    .send(Ok(FsReadFileResponse {
                        data: Vec::new(),
                        eof: true,
                    }))
                    .await;
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    /// Stream write file contents
    async fn write_file(
        &self,
        request: Request<Streaming<FsWriteFileRequest>>,
    ) -> Result<Response<FsWriteFileResponse>, Status> {
        use futures::StreamExt;

        let mut stream = request.into_inner();
        let mut workspace_id: Option<String> = None;
        let mut path: Option<String> = None;
        let mut truncate = false;
        let mut data = Vec::new();

        // Collect all chunks
        while let Some(result) = stream.next().await {
            let chunk = result?;

            match chunk.payload {
                Some(Payload::Header(header)) => {
                    if workspace_id.is_some() {
                        return Err(Status::invalid_argument(
                            "header can only be sent once as the first message",
                        ));
                    }
                    if header.workspace_id.is_empty() {
                        return Err(Status::invalid_argument("workspace_id is required"));
                    }
                    workspace_id = Some(header.workspace_id);
                    path = Some(header.path);
                    truncate = header.truncate;
                }
                Some(Payload::Data(chunk_data)) => {
                    if workspace_id.is_none() {
                        return Err(Status::invalid_argument(
                            "first message must include header",
                        ));
                    }
                    data.extend_from_slice(&chunk_data);
                }
                None => {
                    // Empty message, skip
                }
            }
        }

        let workspace_id = workspace_id.ok_or_else(|| Status::invalid_argument("empty stream"))?;
        let path = path.ok_or_else(|| Status::invalid_argument("empty stream"))?;

        debug!(
            workspace_id = %workspace_id,
            path = %path,
            size = data.len(),
            truncate = truncate,
            "write_file"
        );

        // If truncate is requested, first truncate the file
        if truncate {
            // Write file will overwrite the content anyway
        }

        self.storage
            .write_file(&workspace_id, &path, &data)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsWriteFileResponse {
            bytes_written: data.len() as u64,
        }))
    }

    type ListDirStream = Pin<Box<dyn Stream<Item = Result<FsListDirResponse, Status>> + Send>>;

    /// Stream directory listing
    async fn list_dir(
        &self,
        request: Request<FsListDirRequest>,
    ) -> Result<Response<Self::ListDirStream>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "list_dir");

        let entries = self
            .storage
            .list_dir(&req.workspace_id, &req.path)
            .await
            .map_err(storage_error_to_status)?;

        let (tx, rx) = mpsc::channel(4);

        // Stream entries in batches of 100
        tokio::spawn(async move {
            for chunk in entries.chunks(100) {
                let proto_entries: Vec<FsDirEntry> = chunk
                    .iter()
                    .map(|stat| FsDirEntry {
                        name: stat.name.clone(),
                        attr: Some(file_stat_to_attr(stat)),
                    })
                    .collect();

                if tx
                    .send(Ok(FsListDirResponse {
                        entries: proto_entries,
                    }))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    /// Create a directory
    async fn mkdir(
        &self,
        request: Request<FsMkdirRequest>,
    ) -> Result<Response<FsMkdirResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            path = %req.path,
            mode = req.mode,
            "mkdir"
        );

        // Create directory (non-recursive for FUSE semantics)
        self.storage
            .mkdir(&req.workspace_id, &req.path, false)
            .await
            .map_err(storage_error_to_status)?;

        // Set permissions if specified
        if req.mode != 0 {
            self.storage
                .set_permissions(&req.workspace_id, &req.path, req.mode)
                .await
                .map_err(storage_error_to_status)?;
        }

        // Get the created directory's attributes
        let stat = self
            .storage
            .stat(&req.workspace_id, &req.path)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsMkdirResponse {
            attr: Some(file_stat_to_attr(&stat)),
        }))
    }

    /// Remove a file
    async fn remove_file(
        &self,
        request: Request<FsRemoveFileRequest>,
    ) -> Result<Response<FsRemoveFileResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "remove_file");

        self.storage
            .remove_file(&req.workspace_id, &req.path)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsRemoveFileResponse {}))
    }

    /// Remove a directory
    async fn remove_dir(
        &self,
        request: Request<FsRemoveDirRequest>,
    ) -> Result<Response<FsRemoveDirResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            path = %req.path,
            recursive = req.recursive,
            "remove_dir"
        );

        self.storage
            .remove_dir(&req.workspace_id, &req.path, req.recursive)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsRemoveDirResponse {}))
    }

    /// Rename/move a file or directory
    async fn rename(
        &self,
        request: Request<FsRenameRequest>,
    ) -> Result<Response<FsRenameResponse>, Status> {
        let req = request.into_inner();
        let flags = FsRenameFlags::try_from(req.flags).unwrap_or(FsRenameFlags::None);

        debug!(
            workspace_id = %req.workspace_id,
            old_path = %req.old_path,
            new_path = %req.new_path,
            flags = ?flags,
            "rename"
        );

        // Handle rename flags
        match flags {
            FsRenameFlags::None => {
                // Standard rename (replace if exists)
                self.storage
                    .rename(&req.workspace_id, &req.old_path, &req.new_path)
                    .await
                    .map_err(storage_error_to_status)?;
            }
            FsRenameFlags::Noreplace => {
                // Check if destination exists first
                if self
                    .storage
                    .stat(&req.workspace_id, &req.new_path)
                    .await
                    .is_ok()
                {
                    return Err(Status::already_exists(format!(
                        "destination already exists: {}",
                        req.new_path
                    )));
                }
                self.storage
                    .rename(&req.workspace_id, &req.old_path, &req.new_path)
                    .await
                    .map_err(storage_error_to_status)?;
            }
            FsRenameFlags::Exchange => {
                // RENAME_EXCHANGE is not supported by StorageBackend
                // Return ENOSYS (Function not implemented) for proper FUSE error mapping
                let mut details = ErrorDetails::new();
                let metadata: HashMap<String, String> =
                    [("errno".to_string(), libc::ENOSYS.to_string())]
                        .into_iter()
                        .collect();
                details.set_error_info(format!("ERRNO_{}", libc::ENOSYS), "workspace.v1", metadata);
                return Err(Status::with_error_details(
                    tonic::Code::Unimplemented,
                    "RENAME_EXCHANGE is not supported",
                    details,
                ));
            }
        }

        Ok(Response::new(FsRenameResponse {}))
    }

    /// Create a new file
    async fn create(
        &self,
        request: Request<FsCreateRequest>,
    ) -> Result<Response<FsCreateResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            path = %req.path,
            mode = req.mode,
            exclusive = req.exclusive,
            "create"
        );

        self.storage
            .create_file(&req.workspace_id, &req.path, req.exclusive)
            .await
            .map_err(storage_error_to_status)?;

        // Set permissions if specified
        if req.mode != 0 {
            self.storage
                .set_permissions(&req.workspace_id, &req.path, req.mode)
                .await
                .map_err(storage_error_to_status)?;
        }

        // Get the created file's attributes
        let stat = self
            .storage
            .stat(&req.workspace_id, &req.path)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsCreateResponse {
            attr: Some(file_stat_to_attr(&stat)),
        }))
    }

    /// Set file attributes
    async fn set_attr(
        &self,
        request: Request<FsSetAttrRequest>,
    ) -> Result<Response<FsSetAttrResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            path = %req.path,
            size = ?req.size,
            mode = ?req.mode,
            "set_attr"
        );

        // Set size (truncate) if specified
        if let Some(size) = req.size {
            self.storage
                .set_file_size(&req.workspace_id, &req.path, size)
                .await
                .map_err(storage_error_to_status)?;
        }

        // Set mode if specified
        if let Some(mode) = req.mode {
            self.storage
                .set_permissions(&req.workspace_id, &req.path, mode)
                .await
                .map_err(storage_error_to_status)?;
        }

        // Set times if specified
        let atime = req.atime.as_ref().and_then(timestamp_to_datetime);
        let mtime = req.mtime.as_ref().and_then(timestamp_to_datetime);
        if atime.is_some() || mtime.is_some() {
            self.storage
                .set_times(&req.workspace_id, &req.path, atime, mtime)
                .await
                .map_err(storage_error_to_status)?;
        }

        // Get updated attributes
        let stat = self
            .storage
            .stat(&req.workspace_id, &req.path)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsSetAttrResponse {
            attr: Some(file_stat_to_attr(&stat)),
        }))
    }

    /// Create a symbolic link
    async fn symlink(
        &self,
        request: Request<FsSymlinkRequest>,
    ) -> Result<Response<FsSymlinkResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            link_path = %req.link_path,
            target = %req.target,
            "symlink"
        );

        self.storage
            .symlink(&req.workspace_id, &req.link_path, &req.target)
            .await
            .map_err(storage_error_to_status)?;

        // Get the symlink's attributes
        let stat = self
            .storage
            .stat(&req.workspace_id, &req.link_path)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsSymlinkResponse {
            attr: Some(file_stat_to_attr(&stat)),
        }))
    }

    /// Read symbolic link target
    async fn read_link(
        &self,
        request: Request<FsReadLinkRequest>,
    ) -> Result<Response<FsReadLinkResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, path = %req.path, "read_link");

        let target = self
            .storage
            .readlink(&req.workspace_id, &req.path)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsReadLinkResponse { target }))
    }

    /// Read file at specific offset (random access)
    async fn read_at(
        &self,
        request: Request<FsReadAtRequest>,
    ) -> Result<Response<FsReadAtResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            path = %req.path,
            offset = req.offset,
            size = req.size,
            "read_at"
        );

        let data = self
            .storage
            .read_file_range(&req.workspace_id, &req.path, req.offset, req.size)
            .await
            .map_err(storage_error_to_status)?;

        // EOF is indicated when we read less than requested
        let eof = (data.len() as u32) < req.size;

        Ok(Response::new(FsReadAtResponse { data, eof }))
    }

    /// Write file at specific offset (random access)
    async fn write_at(
        &self,
        request: Request<FsWriteAtRequest>,
    ) -> Result<Response<FsWriteAtResponse>, Status> {
        let req = request.into_inner();
        debug!(
            workspace_id = %req.workspace_id,
            path = %req.path,
            offset = req.offset,
            size = req.data.len(),
            "write_at"
        );

        self.storage
            .write_file_at(&req.workspace_id, &req.path, req.offset, &req.data)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsWriteAtResponse {
            bytes_written: req.data.len() as u64,
        }))
    }

    /// Get filesystem statistics
    async fn stat_fs(
        &self,
        request: Request<FsStatFsRequest>,
    ) -> Result<Response<FsStatFsResponse>, Status> {
        let req = request.into_inner();
        debug!(workspace_id = %req.workspace_id, "stat_fs");

        let stats = self
            .storage
            .stat_fs(&req.workspace_id)
            .await
            .map_err(storage_error_to_status)?;

        Ok(Response::new(FsStatFsResponse {
            blocks: stats.blocks,
            bfree: stats.bfree,
            bavail: stats.bavail,
            files: stats.files,
            ffree: stats.ffree,
            bsize: stats.bsize,
            namelen: stats.namelen,
            frsize: stats.frsize,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_error_to_status() {
        let err = StorageError::NotFound("test.txt".to_string());
        let status = storage_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::NotFound);

        let err = StorageError::AlreadyExists("test.txt".to_string());
        let status = storage_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::AlreadyExists);

        let err = StorageError::PermissionDenied("test.txt".to_string());
        let status = storage_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::PermissionDenied);

        let err = StorageError::IsADirectory("test.txt".to_string());
        let status = storage_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert!(status.message().contains("is a directory"));

        let err = StorageError::DirectoryNotEmpty("test".to_string());
        let status = storage_error_to_status(err);
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert!(status.message().contains("directory not empty"));
    }

    #[test]
    fn test_datetime_timestamp_conversion() {
        let dt = Utc::now();
        let ts = datetime_to_timestamp(dt);
        let dt2 = timestamp_to_datetime(&ts).unwrap();

        // Should be within 1 second (accounting for nanosecond precision loss)
        assert!((dt.timestamp() - dt2.timestamp()).abs() <= 1);
    }
}
