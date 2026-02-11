"""
Sync Workspace Client - Main entry point for synchronous SDK usage with gRPC
"""

from typing import Optional

import grpc

from workspace_sdk.services.workspace import WorkspaceService
from workspace_sdk.services.sandbox import SandboxService
from workspace_sdk.services.process import ProcessService
from workspace_sdk.services.pty import PtyService
from workspace_sdk.services.nfs import NfsService
from workspace_sdk.proto.workspace.v1 import (
    workspace_pb2_grpc,
    sandbox_pb2_grpc,
    process_pb2_grpc,
    pty_pb2_grpc,
    filesystem_pb2_grpc,
)


class WorkspaceClient:
    """Synchronous client for interacting with the Workspace service via gRPC"""

    def __init__(
        self,
        server_addr: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
        nfs_host: Optional[str] = None,
        nfs_port: int = 2049,
    ):
        """
        Initialize the workspace client.

        Args:
            server_addr: gRPC server address (e.g., "localhost:9090")
            api_key: Optional API key for authentication
            timeout: Request timeout in seconds (default: 30)
            nfs_host: NFS server host for mounting workspaces (optional)
            nfs_port: NFS server port (default: 2049)
        """
        self._server_addr = server_addr
        self._api_key = api_key
        self._timeout = timeout
        self._nfs_host = nfs_host
        self._nfs_port = nfs_port
        self._channel: Optional[grpc.Channel] = None

        # Services will be initialized when context manager is entered
        self.workspace: WorkspaceService
        self.sandbox: SandboxService
        self.process: ProcessService
        self.pty: PtyService
        self.nfs: NfsService

    def __enter__(self) -> "WorkspaceClient":
        """Enter context manager"""
        # Create gRPC channel
        self._channel = grpc.insecure_channel(self._server_addr)

        # Create stubs
        workspace_stub = workspace_pb2_grpc.WorkspaceServiceStub(self._channel)
        sandbox_stub = sandbox_pb2_grpc.SandboxServiceStub(self._channel)
        process_stub = process_pb2_grpc.ProcessServiceStub(self._channel)
        pty_stub = pty_pb2_grpc.PtyServiceStub(self._channel)
        filesystem_stub = filesystem_pb2_grpc.FileSystemServiceStub(self._channel)

        # Initialize services
        self.workspace = WorkspaceService(
            workspace_stub, self._api_key, self._timeout
        )
        self.sandbox = SandboxService(sandbox_stub, self._api_key, self._timeout)
        self.process = ProcessService(process_stub, self._api_key, self._timeout)
        self.pty = PtyService(pty_stub, self._api_key, self._timeout)
        self.nfs = NfsService(self._nfs_host, self._nfs_port)

        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        """Exit context manager"""
        if self._channel:
            self._channel.close()
            self._channel = None

    @staticmethod
    def create(
        server_addr: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
        nfs_host: Optional[str] = None,
        nfs_port: int = 2049,
    ) -> "WorkspaceClient":
        """
        Factory method to create a WorkspaceClient.

        Usage:
            with WorkspaceClient.create("localhost:9090") as client:
                workspace = client.workspace.create()
                sandbox = client.sandbox.create(CreateSandboxParams(workspace_id=workspace.id))
        """
        return WorkspaceClient(server_addr, api_key, timeout, nfs_host, nfs_port)
