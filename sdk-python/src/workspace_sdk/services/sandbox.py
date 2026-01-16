"""
Sandbox service for managing sandbox lifecycle
"""

from typing import Optional, List
import httpx

from workspace_sdk.types import Sandbox, SandboxState, CreateSandboxParams


class AsyncSandboxService:
    """Async service for managing sandboxes"""

    def __init__(self, client: httpx.AsyncClient, api_url: str):
        self._client = client
        self._api_url = api_url

    async def create(self, params: CreateSandboxParams) -> Sandbox:
        """Create a new sandbox bound to a workspace"""
        data = {
            "workspace_id": params.workspace_id,
        }
        if params.template:
            data["template"] = params.template
        if params.name:
            data["name"] = params.name
        if params.env:
            data["env"] = params.env
        if params.metadata:
            data["metadata"] = params.metadata
        if params.timeout:
            data["timeout"] = params.timeout

        response = await self._client.post("/sandboxes", json=data)
        response.raise_for_status()
        return self._transform_sandbox(response.json())

    async def get(self, sandbox_id: str) -> Sandbox:
        """Get a sandbox by ID"""
        response = await self._client.get(f"/sandboxes/{sandbox_id}")
        response.raise_for_status()
        return self._transform_sandbox(response.json())

    async def list(self, state: Optional[SandboxState] = None) -> List[Sandbox]:
        """List all sandboxes"""
        params = {}
        if state:
            params["state"] = state

        response = await self._client.get("/sandboxes", params=params)
        response.raise_for_status()
        data = response.json()
        return [self._transform_sandbox(s) for s in data.get("sandboxes", [])]

    async def delete(self, sandbox_id: str, force: bool = False) -> None:
        """Delete a sandbox"""
        params = {}
        if force:
            params["force"] = "true"

        response = await self._client.delete(f"/sandboxes/{sandbox_id}", params=params)
        response.raise_for_status()

    def _transform_sandbox(self, data: dict) -> Sandbox:
        """Transform API response to Sandbox type"""
        return Sandbox(
            id=data["id"],
            workspace_id=data["workspace_id"],
            name=data.get("name"),
            template=data["template"],
            state=data["state"],
            env=data.get("env"),
            metadata=data.get("metadata"),
            created_at=data["created_at"],
            updated_at=data["updated_at"],
            timeout=data.get("timeout"),
            error_message=data.get("error_message"),
        )


class SandboxService:
    """Sync service for managing sandboxes (wrapper around async service)"""

    def __init__(self, client: httpx.Client, api_url: str):
        self._client = client
        self._api_url = api_url

    def create(self, params: CreateSandboxParams) -> Sandbox:
        """Create a new sandbox bound to a workspace"""
        data = {
            "workspace_id": params.workspace_id,
        }
        if params.template:
            data["template"] = params.template
        if params.name:
            data["name"] = params.name
        if params.env:
            data["env"] = params.env
        if params.metadata:
            data["metadata"] = params.metadata
        if params.timeout:
            data["timeout"] = params.timeout

        response = self._client.post("/sandboxes", json=data)
        response.raise_for_status()
        return self._transform_sandbox(response.json())

    def get(self, sandbox_id: str) -> Sandbox:
        """Get a sandbox by ID"""
        response = self._client.get(f"/sandboxes/{sandbox_id}")
        response.raise_for_status()
        return self._transform_sandbox(response.json())

    def list(self, state: Optional[SandboxState] = None) -> List[Sandbox]:
        """List all sandboxes"""
        params = {}
        if state:
            params["state"] = state

        response = self._client.get("/sandboxes", params=params)
        response.raise_for_status()
        data = response.json()
        return [self._transform_sandbox(s) for s in data.get("sandboxes", [])]

    def delete(self, sandbox_id: str, force: bool = False) -> None:
        """Delete a sandbox"""
        params = {}
        if force:
            params["force"] = "true"

        response = self._client.delete(f"/sandboxes/{sandbox_id}", params=params)
        response.raise_for_status()

    def _transform_sandbox(self, data: dict) -> Sandbox:
        """Transform API response to Sandbox type"""
        return Sandbox(
            id=data["id"],
            workspace_id=data["workspace_id"],
            name=data.get("name"),
            template=data["template"],
            state=data["state"],
            env=data.get("env"),
            metadata=data.get("metadata"),
            created_at=data["created_at"],
            updated_at=data["updated_at"],
            timeout=data.get("timeout"),
            error_message=data.get("error_message"),
        )
