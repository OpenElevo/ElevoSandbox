"""
Sync Workspace Client - Main entry point for synchronous SDK usage
"""

from typing import Optional
import httpx

from workspace_sdk.services.workspace import WorkspaceService
from workspace_sdk.services.sandbox import SandboxService
from workspace_sdk.services.process import ProcessService
from workspace_sdk.services.pty import PtyService
from workspace_sdk.services.nfs import NfsService
from workspace_sdk.errors import parse_error_response


class WorkspaceClient:
    """Synchronous client for interacting with the Workspace service"""

    def __init__(
        self,
        api_url: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
        nfs_host: Optional[str] = None,
        nfs_port: int = 2049,
    ):
        """
        Initialize the workspace client.

        Args:
            api_url: Base URL of the workspace server
            api_key: Optional API key for authentication
            timeout: Request timeout in seconds (default: 30)
            nfs_host: NFS server host for mounting workspaces (optional)
            nfs_port: NFS server port (default: 2049)
        """
        self._api_url = api_url
        self._api_key = api_key
        self._timeout = timeout
        self._nfs_host = nfs_host
        self._nfs_port = nfs_port
        self._client: Optional[httpx.Client] = None

        # Services will be initialized when context manager is entered
        self.workspace: WorkspaceService
        self.sandbox: SandboxService
        self.process: ProcessService
        self.pty: PtyService
        self.nfs: NfsService

    def __enter__(self) -> "WorkspaceClient":
        """Enter context manager"""
        headers = {"Content-Type": "application/json"}
        if self._api_key:
            headers["Authorization"] = f"Bearer {self._api_key}"

        self._client = httpx.Client(
            base_url=f"{self._api_url}/api/v1",
            headers=headers,
            timeout=self._timeout,
        )

        # Initialize services
        self.workspace = WorkspaceService(self._client, self._api_url)
        self.sandbox = SandboxService(self._client, self._api_url)
        self.process = ProcessService(self._client, self._api_url)
        self.pty = PtyService(self._client, self._api_url)
        self.nfs = NfsService(self._nfs_host, self._nfs_port)

        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        """Exit context manager"""
        if self._client:
            self._client.close()
            self._client = None

    def health(self) -> dict:
        """Check if the server is healthy"""
        if not self._client:
            raise RuntimeError("Client not initialized. Use 'with' context manager.")
        response = self._client.get("/health")
        response.raise_for_status()
        return response.json()

    @staticmethod
    def create(
        api_url: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
        nfs_host: Optional[str] = None,
        nfs_port: int = 2049,
    ) -> "WorkspaceClient":
        """
        Factory method to create a WorkspaceClient.

        Usage:
            with WorkspaceClient.create("http://localhost:8080") as client:
                workspace = client.workspace.create()
                sandbox = client.sandbox.create(CreateSandboxParams(workspace_id=workspace.id))
        """
        return WorkspaceClient(api_url, api_key, timeout, nfs_host, nfs_port)
