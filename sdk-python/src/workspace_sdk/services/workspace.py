"""
Workspace service for managing workspaces and file operations
"""

from typing import Optional, List
import httpx

from workspace_sdk.types import Workspace, CreateWorkspaceParams, FileInfo


class AsyncWorkspaceService:
    """Async service for managing workspaces and file operations"""

    def __init__(self, client: httpx.AsyncClient, api_url: str):
        self._client = client
        self._api_url = api_url

    # ==================== Workspace CRUD ====================

    async def create(self, params: Optional[CreateWorkspaceParams] = None) -> Workspace:
        """Create a new workspace"""
        data = {}
        if params:
            if params.name:
                data["name"] = params.name
            if params.metadata:
                data["metadata"] = params.metadata

        response = await self._client.post("/workspaces", json=data)
        response.raise_for_status()
        return self._transform_workspace(response.json())

    async def get(self, workspace_id: str) -> Workspace:
        """Get a workspace by ID"""
        response = await self._client.get(f"/workspaces/{workspace_id}")
        response.raise_for_status()
        return self._transform_workspace(response.json())

    async def list(self) -> List[Workspace]:
        """List all workspaces"""
        response = await self._client.get("/workspaces")
        response.raise_for_status()
        data = response.json()
        return [self._transform_workspace(w) for w in data.get("workspaces", [])]

    async def delete(self, workspace_id: str) -> None:
        """Delete a workspace"""
        response = await self._client.delete(f"/workspaces/{workspace_id}")
        response.raise_for_status()

    # ==================== File Operations ====================

    async def read_file(self, workspace_id: str, path: str) -> str:
        """Read a file from workspace"""
        response = await self._client.get(
            f"/workspaces/{workspace_id}/files",
            params={"path": path}
        )
        response.raise_for_status()
        return response.json()["content"]

    async def read_file_bytes(self, workspace_id: str, path: str) -> bytes:
        """Read a file as bytes from workspace"""
        response = await self._client.get(
            f"/workspaces/{workspace_id}/files",
            params={"path": path},
        )
        response.raise_for_status()
        return response.content

    async def write_file(self, workspace_id: str, path: str, content: str | bytes) -> None:
        """Write a file to workspace"""
        if isinstance(content, bytes):
            content = content.decode('utf-8')
        response = await self._client.put(
            f"/workspaces/{workspace_id}/files",
            params={"path": path},
            json={"content": content}
        )
        response.raise_for_status()

    async def mkdir(self, workspace_id: str, path: str) -> None:
        """Create a directory in workspace"""
        response = await self._client.post(
            f"/workspaces/{workspace_id}/files/mkdir",
            json={"path": path}
        )
        response.raise_for_status()

    async def list_files(self, workspace_id: str, path: str) -> List[FileInfo]:
        """List directory contents in workspace"""
        response = await self._client.get(
            f"/workspaces/{workspace_id}/files/list",
            params={"path": path}
        )
        response.raise_for_status()
        data = response.json()
        return [self._transform_file_info(f) for f in data.get("files", [])]

    async def delete_file(self, workspace_id: str, path: str, recursive: bool = False) -> None:
        """Delete a file or directory in workspace"""
        response = await self._client.delete(
            f"/workspaces/{workspace_id}/files",
            params={"path": path, "recursive": "true" if recursive else "false"}
        )
        response.raise_for_status()

    async def move_file(self, workspace_id: str, source: str, destination: str) -> None:
        """Move/rename a file or directory in workspace"""
        response = await self._client.post(
            f"/workspaces/{workspace_id}/files/move",
            json={"source": source, "destination": destination}
        )
        response.raise_for_status()

    async def copy_file(self, workspace_id: str, source: str, destination: str) -> None:
        """Copy a file or directory in workspace"""
        response = await self._client.post(
            f"/workspaces/{workspace_id}/files/copy",
            json={"source": source, "destination": destination}
        )
        response.raise_for_status()

    async def get_file_info(self, workspace_id: str, path: str) -> FileInfo:
        """Get file information in workspace"""
        response = await self._client.get(
            f"/workspaces/{workspace_id}/files/info",
            params={"path": path}
        )
        response.raise_for_status()
        return self._transform_file_info(response.json())

    async def exists(self, workspace_id: str, path: str) -> bool:
        """Check if a file or directory exists in workspace"""
        try:
            await self.get_file_info(workspace_id, path)
            return True
        except Exception:
            return False

    # ==================== Transform Helpers ====================

    def _transform_workspace(self, data: dict) -> Workspace:
        """Transform API response to Workspace type"""
        return Workspace(
            id=data["id"],
            name=data.get("name"),
            nfs_url=data.get("nfs_url"),
            metadata=data.get("metadata"),
            created_at=data["created_at"],
            updated_at=data["updated_at"],
        )

    def _transform_file_info(self, data: dict) -> FileInfo:
        """Transform API response to FileInfo type"""
        return FileInfo(
            name=data["name"],
            path=data["path"],
            type=data["type"],
            size=data["size"],
            modified_at=data.get("modified_at"),
        )


class WorkspaceService:
    """Sync service for managing workspaces and file operations"""

    def __init__(self, client: httpx.Client, api_url: str):
        self._client = client
        self._api_url = api_url

    # ==================== Workspace CRUD ====================

    def create(self, params: Optional[CreateWorkspaceParams] = None) -> Workspace:
        """Create a new workspace"""
        data = {}
        if params:
            if params.name:
                data["name"] = params.name
            if params.metadata:
                data["metadata"] = params.metadata

        response = self._client.post("/workspaces", json=data)
        response.raise_for_status()
        return self._transform_workspace(response.json())

    def get(self, workspace_id: str) -> Workspace:
        """Get a workspace by ID"""
        response = self._client.get(f"/workspaces/{workspace_id}")
        response.raise_for_status()
        return self._transform_workspace(response.json())

    def list(self) -> List[Workspace]:
        """List all workspaces"""
        response = self._client.get("/workspaces")
        response.raise_for_status()
        data = response.json()
        return [self._transform_workspace(w) for w in data.get("workspaces", [])]

    def delete(self, workspace_id: str) -> None:
        """Delete a workspace"""
        response = self._client.delete(f"/workspaces/{workspace_id}")
        response.raise_for_status()

    # ==================== File Operations ====================

    def read_file(self, workspace_id: str, path: str) -> str:
        """Read a file from workspace"""
        response = self._client.get(
            f"/workspaces/{workspace_id}/files",
            params={"path": path}
        )
        response.raise_for_status()
        return response.json()["content"]

    def read_file_bytes(self, workspace_id: str, path: str) -> bytes:
        """Read a file as bytes from workspace"""
        response = self._client.get(
            f"/workspaces/{workspace_id}/files",
            params={"path": path},
        )
        response.raise_for_status()
        return response.content

    def write_file(self, workspace_id: str, path: str, content: str | bytes) -> None:
        """Write a file to workspace"""
        if isinstance(content, bytes):
            content = content.decode('utf-8')
        response = self._client.put(
            f"/workspaces/{workspace_id}/files",
            params={"path": path},
            json={"content": content}
        )
        response.raise_for_status()

    def mkdir(self, workspace_id: str, path: str) -> None:
        """Create a directory in workspace"""
        response = self._client.post(
            f"/workspaces/{workspace_id}/files/mkdir",
            json={"path": path}
        )
        response.raise_for_status()

    def list_files(self, workspace_id: str, path: str) -> List[FileInfo]:
        """List directory contents in workspace"""
        response = self._client.get(
            f"/workspaces/{workspace_id}/files/list",
            params={"path": path}
        )
        response.raise_for_status()
        data = response.json()
        return [self._transform_file_info(f) for f in data.get("files", [])]

    def delete_file(self, workspace_id: str, path: str, recursive: bool = False) -> None:
        """Delete a file or directory in workspace"""
        response = self._client.delete(
            f"/workspaces/{workspace_id}/files",
            params={"path": path, "recursive": "true" if recursive else "false"}
        )
        response.raise_for_status()

    def move_file(self, workspace_id: str, source: str, destination: str) -> None:
        """Move/rename a file or directory in workspace"""
        response = self._client.post(
            f"/workspaces/{workspace_id}/files/move",
            json={"source": source, "destination": destination}
        )
        response.raise_for_status()

    def copy_file(self, workspace_id: str, source: str, destination: str) -> None:
        """Copy a file or directory in workspace"""
        response = self._client.post(
            f"/workspaces/{workspace_id}/files/copy",
            json={"source": source, "destination": destination}
        )
        response.raise_for_status()

    def get_file_info(self, workspace_id: str, path: str) -> FileInfo:
        """Get file information in workspace"""
        response = self._client.get(
            f"/workspaces/{workspace_id}/files/info",
            params={"path": path}
        )
        response.raise_for_status()
        return self._transform_file_info(response.json())

    def exists(self, workspace_id: str, path: str) -> bool:
        """Check if a file or directory exists in workspace"""
        try:
            self.get_file_info(workspace_id, path)
            return True
        except Exception:
            return False

    # ==================== Transform Helpers ====================

    def _transform_workspace(self, data: dict) -> Workspace:
        """Transform API response to Workspace type"""
        return Workspace(
            id=data["id"],
            name=data.get("name"),
            nfs_url=data.get("nfs_url"),
            metadata=data.get("metadata"),
            created_at=data["created_at"],
            updated_at=data["updated_at"],
        )

    def _transform_file_info(self, data: dict) -> FileInfo:
        """Transform API response to FileInfo type"""
        return FileInfo(
            name=data["name"],
            path=data["path"],
            type=data["type"],
            size=data["size"],
            modified_at=data.get("modified_at"),
        )
