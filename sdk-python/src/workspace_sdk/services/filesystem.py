"""
FileSystem service for file operations
"""

from typing import Optional, List
import httpx

from workspace_sdk.types import FileInfo


class AsyncFileSystemService:
    """Async service for file operations in sandboxes"""

    def __init__(self, client: httpx.AsyncClient, api_url: str):
        self._client = client
        self._api_url = api_url

    async def read(self, sandbox_id: str, path: str) -> str:
        """Read a file as text"""
        response = await self._client.get(
            f"/sandboxes/{sandbox_id}/files",
            params={"path": path},
        )
        response.raise_for_status()
        return response.json()["content"]

    async def read_bytes(self, sandbox_id: str, path: str) -> bytes:
        """Read a file as bytes"""
        response = await self._client.get(
            f"/sandboxes/{sandbox_id}/files",
            params={"path": path},
            headers={"Accept": "application/octet-stream"},
        )
        response.raise_for_status()
        return response.content

    async def write(
        self,
        sandbox_id: str,
        path: str,
        content: str | bytes,
    ) -> None:
        """Write content to a file"""
        if isinstance(content, bytes):
            response = await self._client.put(
                f"/sandboxes/{sandbox_id}/files",
                params={"path": path},
                content=content,
                headers={"Content-Type": "application/octet-stream"},
            )
        else:
            response = await self._client.put(
                f"/sandboxes/{sandbox_id}/files",
                params={"path": path},
                json={"content": content},
            )
        response.raise_for_status()

    async def mkdir(
        self,
        sandbox_id: str,
        path: str,
        recursive: bool = False,
    ) -> None:
        """Create a directory"""
        response = await self._client.post(
            f"/sandboxes/{sandbox_id}/files/mkdir",
            json={"path": path, "recursive": recursive},
        )
        response.raise_for_status()

    async def list(self, sandbox_id: str, path: str) -> List[FileInfo]:
        """List directory contents"""
        response = await self._client.get(
            f"/sandboxes/{sandbox_id}/files/list",
            params={"path": path},
        )
        response.raise_for_status()
        data = response.json()
        return [self._transform_file_info(f) for f in data.get("files", [])]

    async def remove(
        self,
        sandbox_id: str,
        path: str,
        recursive: bool = False,
    ) -> None:
        """Remove a file or directory"""
        response = await self._client.delete(
            f"/sandboxes/{sandbox_id}/files",
            params={"path": path, "recursive": str(recursive).lower()},
        )
        response.raise_for_status()

    async def move(
        self,
        sandbox_id: str,
        source: str,
        destination: str,
    ) -> None:
        """Move/rename a file or directory"""
        response = await self._client.post(
            f"/sandboxes/{sandbox_id}/files/move",
            json={"source": source, "destination": destination},
        )
        response.raise_for_status()

    async def copy(
        self,
        sandbox_id: str,
        source: str,
        destination: str,
    ) -> None:
        """Copy a file or directory"""
        response = await self._client.post(
            f"/sandboxes/{sandbox_id}/files/copy",
            json={"source": source, "destination": destination},
        )
        response.raise_for_status()

    async def get_info(self, sandbox_id: str, path: str) -> FileInfo:
        """Get file information"""
        response = await self._client.get(
            f"/sandboxes/{sandbox_id}/files/info",
            params={"path": path},
        )
        response.raise_for_status()
        return self._transform_file_info(response.json())

    async def exists(self, sandbox_id: str, path: str) -> bool:
        """Check if a file or directory exists"""
        try:
            await self.get_info(sandbox_id, path)
            return True
        except httpx.HTTPStatusError:
            return False

    def _transform_file_info(self, data: dict) -> FileInfo:
        """Transform API response to FileInfo type"""
        return FileInfo(
            name=data["name"],
            path=data["path"],
            type=data["type"],
            size=data["size"],
            modified_at=data.get("modified_at"),
        )


class FileSystemService:
    """Sync service for file operations in sandboxes"""

    def __init__(self, client: httpx.Client, api_url: str):
        self._client = client
        self._api_url = api_url

    def read(self, sandbox_id: str, path: str) -> str:
        """Read a file as text"""
        response = self._client.get(
            f"/sandboxes/{sandbox_id}/files",
            params={"path": path},
        )
        response.raise_for_status()
        return response.json()["content"]

    def read_bytes(self, sandbox_id: str, path: str) -> bytes:
        """Read a file as bytes"""
        response = self._client.get(
            f"/sandboxes/{sandbox_id}/files",
            params={"path": path},
            headers={"Accept": "application/octet-stream"},
        )
        response.raise_for_status()
        return response.content

    def write(
        self,
        sandbox_id: str,
        path: str,
        content: str | bytes,
    ) -> None:
        """Write content to a file"""
        if isinstance(content, bytes):
            response = self._client.put(
                f"/sandboxes/{sandbox_id}/files",
                params={"path": path},
                content=content,
                headers={"Content-Type": "application/octet-stream"},
            )
        else:
            response = self._client.put(
                f"/sandboxes/{sandbox_id}/files",
                params={"path": path},
                json={"content": content},
            )
        response.raise_for_status()

    def mkdir(
        self,
        sandbox_id: str,
        path: str,
        recursive: bool = False,
    ) -> None:
        """Create a directory"""
        response = self._client.post(
            f"/sandboxes/{sandbox_id}/files/mkdir",
            json={"path": path, "recursive": recursive},
        )
        response.raise_for_status()

    def list(self, sandbox_id: str, path: str) -> List[FileInfo]:
        """List directory contents"""
        response = self._client.get(
            f"/sandboxes/{sandbox_id}/files/list",
            params={"path": path},
        )
        response.raise_for_status()
        data = response.json()
        return [self._transform_file_info(f) for f in data.get("files", [])]

    def remove(
        self,
        sandbox_id: str,
        path: str,
        recursive: bool = False,
    ) -> None:
        """Remove a file or directory"""
        response = self._client.delete(
            f"/sandboxes/{sandbox_id}/files",
            params={"path": path, "recursive": str(recursive).lower()},
        )
        response.raise_for_status()

    def move(
        self,
        sandbox_id: str,
        source: str,
        destination: str,
    ) -> None:
        """Move/rename a file or directory"""
        response = self._client.post(
            f"/sandboxes/{sandbox_id}/files/move",
            json={"source": source, "destination": destination},
        )
        response.raise_for_status()

    def copy(
        self,
        sandbox_id: str,
        source: str,
        destination: str,
    ) -> None:
        """Copy a file or directory"""
        response = self._client.post(
            f"/sandboxes/{sandbox_id}/files/copy",
            json={"source": source, "destination": destination},
        )
        response.raise_for_status()

    def get_info(self, sandbox_id: str, path: str) -> FileInfo:
        """Get file information"""
        response = self._client.get(
            f"/sandboxes/{sandbox_id}/files/info",
            params={"path": path},
        )
        response.raise_for_status()
        return self._transform_file_info(response.json())

    def exists(self, sandbox_id: str, path: str) -> bool:
        """Check if a file or directory exists"""
        try:
            self.get_info(sandbox_id, path)
            return True
        except httpx.HTTPStatusError:
            return False

    def _transform_file_info(self, data: dict) -> FileInfo:
        """Transform API response to FileInfo type"""
        return FileInfo(
            name=data["name"],
            path=data["path"],
            type=data["type"],
            size=data["size"],
            modified_at=data.get("modified_at"),
        )
