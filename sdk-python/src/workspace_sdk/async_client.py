"""
Async Workspace Client - Main entry point for async SDK usage
"""

from typing import Optional
import httpx

from workspace_sdk.services.workspace import AsyncWorkspaceService
from workspace_sdk.services.sandbox import AsyncSandboxService
from workspace_sdk.services.process import AsyncProcessService
from workspace_sdk.services.pty import AsyncPtyService
from workspace_sdk.errors import parse_error_response


class AsyncWorkspaceClient:
    """Async client for interacting with the Workspace service"""

    def __init__(
        self,
        api_url: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ):
        """
        Initialize the async workspace client.

        Args:
            api_url: Base URL of the workspace server
            api_key: Optional API key for authentication
            timeout: Request timeout in seconds (default: 30)
        """
        self._api_url = api_url
        self._api_key = api_key
        self._timeout = timeout
        self._client: Optional[httpx.AsyncClient] = None

        # Services will be initialized when context manager is entered
        self.workspace: AsyncWorkspaceService
        self.sandbox: AsyncSandboxService
        self.process: AsyncProcessService
        self.pty: AsyncPtyService

    async def __aenter__(self) -> "AsyncWorkspaceClient":
        """Enter async context manager"""
        headers = {"Content-Type": "application/json"}
        if self._api_key:
            headers["Authorization"] = f"Bearer {self._api_key}"

        self._client = httpx.AsyncClient(
            base_url=f"{self._api_url}/api/v1",
            headers=headers,
            timeout=self._timeout,
        )

        # Initialize services
        self.workspace = AsyncWorkspaceService(self._client, self._api_url)
        self.sandbox = AsyncSandboxService(self._client, self._api_url)
        self.process = AsyncProcessService(self._client, self._api_url)
        self.pty = AsyncPtyService(self._client, self._api_url)

        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Exit async context manager"""
        if self._client:
            await self._client.aclose()
            self._client = None

    async def health(self) -> dict:
        """Check if the server is healthy"""
        if not self._client:
            raise RuntimeError("Client not initialized. Use 'async with' context manager.")
        response = await self._client.get("/health")
        response.raise_for_status()
        return response.json()

    @staticmethod
    def create(
        api_url: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ) -> "AsyncWorkspaceClient":
        """
        Factory method to create an AsyncWorkspaceClient.

        Usage:
            async with AsyncWorkspaceClient.create("http://localhost:8080") as client:
                workspace = await client.workspace.create()
                sandbox = await client.sandbox.create(CreateSandboxParams(workspace_id=workspace.id))
        """
        return AsyncWorkspaceClient(api_url, api_key, timeout)
