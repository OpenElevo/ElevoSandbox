"""
Workspace service for managing workspaces and file operations via gRPC
"""

from typing import Optional, List
from datetime import datetime

import grpc

from workspace_sdk.types import Workspace, CreateWorkspaceParams, FileInfo
from workspace_sdk.errors import convert_grpc_error, NotFoundError
from workspace_sdk.proto.workspace.v1 import workspace_pb2, workspace_pb2_grpc


def _create_metadata(api_key: Optional[str]) -> List[tuple]:
    """Create gRPC metadata with auth token"""
    if api_key:
        return [("authorization", f"Bearer {api_key}")]
    return []


class WorkspaceService:
    """Sync service for managing workspaces and file operations via gRPC"""

    def __init__(
        self,
        stub: workspace_pb2_grpc.WorkspaceServiceStub,
        api_key: Optional[str],
        timeout: float,
    ):
        self._stub = stub
        self._api_key = api_key
        self._timeout = timeout

    def _metadata(self) -> List[tuple]:
        return _create_metadata(self._api_key)

    # ==================== Workspace CRUD ====================

    def create(self, params: Optional[CreateWorkspaceParams] = None) -> Workspace:
        """Create a new workspace"""
        req = workspace_pb2.CreateWorkspaceRequest()
        if params:
            if params.name:
                req.name = params.name
            if params.storage_type:
                req.storage_type = params.storage_type
            if params.metadata:
                req.metadata.update(params.metadata)

        try:
            resp = self._stub.CreateWorkspace(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_workspace(resp.workspace)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def get(self, workspace_id: str) -> Workspace:
        """Get a workspace by ID"""
        req = workspace_pb2.GetWorkspaceRequest(id=workspace_id)
        try:
            resp = self._stub.GetWorkspace(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_workspace(resp.workspace)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def list(self) -> List[Workspace]:
        """List all workspaces"""
        req = workspace_pb2.ListWorkspacesRequest()
        try:
            resp = self._stub.ListWorkspaces(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return [self._transform_workspace(w) for w in resp.workspaces]
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def delete(self, workspace_id: str) -> None:
        """Delete a workspace"""
        req = workspace_pb2.DeleteWorkspaceRequest(id=workspace_id)
        try:
            self._stub.DeleteWorkspace(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    # ==================== File Operations ====================

    def read_file(self, workspace_id: str, path: str) -> str:
        """Read a file from workspace"""
        req = workspace_pb2.ReadFileRequest(workspace_id=workspace_id, path=path)
        try:
            resp = self._stub.ReadFile(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return resp.content.decode("utf-8") if isinstance(resp.content, bytes) else resp.content
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def read_file_bytes(self, workspace_id: str, path: str) -> bytes:
        """Read a file as bytes from workspace"""
        req = workspace_pb2.ReadFileRequest(workspace_id=workspace_id, path=path)
        try:
            resp = self._stub.ReadFile(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return resp.content if isinstance(resp.content, bytes) else resp.content.encode("utf-8")
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def write_file(self, workspace_id: str, path: str, content: str | bytes) -> None:
        """Write a file to workspace"""
        if isinstance(content, str):
            content = content.encode("utf-8")
        req = workspace_pb2.WriteFileRequest(
            workspace_id=workspace_id, path=path, content=content
        )
        try:
            self._stub.WriteFile(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def mkdir(self, workspace_id: str, path: str) -> None:
        """Create a directory in workspace"""
        req = workspace_pb2.MkdirRequest(
            workspace_id=workspace_id, path=path
        )
        try:
            self._stub.Mkdir(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def list_files(self, workspace_id: str, path: str) -> List[FileInfo]:
        """List directory contents in workspace"""
        req = workspace_pb2.ListFilesRequest(workspace_id=workspace_id, path=path)
        try:
            resp = self._stub.ListFiles(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return [self._transform_file_info(f) for f in resp.files]
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def delete_file(self, workspace_id: str, path: str, recursive: bool = False) -> None:
        """Delete a file or directory in workspace"""
        req = workspace_pb2.DeleteFileRequest(
            workspace_id=workspace_id, path=path, recursive=recursive
        )
        try:
            self._stub.DeleteFile(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def move_file(self, workspace_id: str, source: str, destination: str) -> None:
        """Move/rename a file or directory in workspace"""
        req = workspace_pb2.MoveFileRequest(
            workspace_id=workspace_id, source=source, destination=destination
        )
        try:
            self._stub.MoveFile(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def copy_file(self, workspace_id: str, source: str, destination: str) -> None:
        """Copy a file or directory in workspace"""
        req = workspace_pb2.CopyFileRequest(
            workspace_id=workspace_id, source=source, destination=destination
        )
        try:
            self._stub.CopyFile(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def get_file_info(self, workspace_id: str, path: str) -> FileInfo:
        """Get file information in workspace"""
        req = workspace_pb2.GetFileInfoRequest(
            workspace_id=workspace_id, path=path
        )
        try:
            resp = self._stub.GetFileInfo(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_file_info(resp.file)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def exists(self, workspace_id: str, path: str) -> bool:
        """Check if a file or directory exists in workspace"""
        try:
            self.get_file_info(workspace_id, path)
            return True
        except NotFoundError:
            return False

    # ==================== NFS Transport ====================

    def register_nfs_transport(self, workspace_id: str, nfs_url: str) -> Workspace:
        """Register NFS transport for a workspace (switch from gRPC to NFS)"""
        req = workspace_pb2.RegisterNfsTransportRequest(
            workspace_id=workspace_id, nfs_url=nfs_url
        )
        try:
            resp = self._stub.RegisterNfsTransport(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_workspace(resp.workspace)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def unregister_nfs_transport(self, workspace_id: str) -> Workspace:
        """Unregister NFS transport for a workspace (switch back to gRPC)"""
        req = workspace_pb2.UnregisterNfsTransportRequest(
            workspace_id=workspace_id
        )
        try:
            resp = self._stub.UnregisterNfsTransport(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_workspace(resp.workspace)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    # ==================== Transform Helpers ====================

    def _transform_workspace(self, ws) -> Workspace:
        """Transform proto Workspace to SDK Workspace type"""
        created_at = None
        updated_at = None
        if ws.created_at:
            created_at = ws.created_at.ToDatetime().isoformat()
        if ws.updated_at:
            updated_at = ws.updated_at.ToDatetime().isoformat()

        return Workspace(
            id=ws.id,
            name=ws.name if ws.HasField("name") else None,
            nfs_url=ws.nfs_url if ws.HasField("nfs_url") else None,
            storage_type=ws.storage_type if ws.storage_type else None,
            storage_config=ws.storage_config if ws.storage_config else None,
            metadata=dict(ws.metadata) if ws.metadata else None,
            created_at=created_at,
            updated_at=updated_at,
        )

    def _transform_file_info(self, f) -> FileInfo:
        """Transform proto FileInfo to SDK FileInfo type"""
        modified_at = None
        if f.modified_at:
            modified_at = f.modified_at.ToDatetime().isoformat()

        return FileInfo(
            name=f.name,
            path=f.path,
            type=f.type,
            size=f.size,
            modified_at=modified_at,
        )
