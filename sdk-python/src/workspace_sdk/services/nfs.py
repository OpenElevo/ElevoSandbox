"""
NFS service for mounting sandbox workspaces
"""

import os
import subprocess
import tempfile
from pathlib import Path
from typing import Optional
from urllib.parse import urlparse


class NfsMount:
    """Context manager for NFS mount"""

    def __init__(
        self,
        host: str,
        port: int,
        export_path: str,
        mount_point: Optional[str] = None,
        options: Optional[str] = None,
    ):
        self.host = host
        self.port = port
        self.export_path = export_path
        self._mount_point = mount_point
        self._temp_dir: Optional[tempfile.TemporaryDirectory] = None
        self._mounted = False
        self.options = options or "nfsvers=3,tcp,nolock,port={port},mountport={port}"

    @property
    def mount_point(self) -> str:
        """Get the mount point path"""
        if self._mount_point:
            return self._mount_point
        if self._temp_dir:
            return self._temp_dir.name
        raise RuntimeError("Mount point not initialized")

    def mount(self) -> str:
        """
        Mount the NFS share.

        Returns:
            The mount point path
        """
        if self._mounted:
            return self.mount_point

        # Create mount point if not specified
        if not self._mount_point:
            self._temp_dir = tempfile.TemporaryDirectory(prefix="workspace_nfs_")
            mount_path = self._temp_dir.name
        else:
            mount_path = self._mount_point
            Path(mount_path).mkdir(parents=True, exist_ok=True)

        # Build mount options
        opts = self.options.format(port=self.port)

        # Mount command
        cmd = [
            "mount",
            "-t", "nfs",
            "-o", opts,
            f"{self.host}:{self.export_path}",
            mount_path,
        ]

        try:
            subprocess.run(cmd, check=True, capture_output=True, text=True)
            self._mounted = True
            return mount_path
        except subprocess.CalledProcessError as e:
            if self._temp_dir:
                self._temp_dir.cleanup()
                self._temp_dir = None
            raise RuntimeError(f"Failed to mount NFS: {e.stderr}") from e

    def unmount(self) -> None:
        """Unmount the NFS share"""
        if not self._mounted:
            return

        try:
            subprocess.run(
                ["umount", self.mount_point],
                check=True,
                capture_output=True,
                text=True,
            )
        except subprocess.CalledProcessError as e:
            # Try lazy unmount
            try:
                subprocess.run(
                    ["umount", "-l", self.mount_point],
                    check=True,
                    capture_output=True,
                    text=True,
                )
            except subprocess.CalledProcessError:
                pass
        finally:
            self._mounted = False
            if self._temp_dir:
                self._temp_dir.cleanup()
                self._temp_dir = None

    def __enter__(self) -> "NfsMount":
        self.mount()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.unmount()


class NfsService:
    """Service for managing NFS mounts for sandbox workspaces"""

    def __init__(self, default_host: Optional[str] = None, default_port: int = 2049):
        """
        Initialize NFS service.

        Args:
            default_host: Default NFS server host (can be overridden per mount)
            default_port: Default NFS port (default: 2049)
        """
        self.default_host = default_host
        self.default_port = default_port

    def mount(
        self,
        sandbox_id: str,
        mount_point: Optional[str] = None,
        host: Optional[str] = None,
        port: Optional[int] = None,
        nfs_url: Optional[str] = None,
    ) -> NfsMount:
        """
        Create an NFS mount for a sandbox.

        Args:
            sandbox_id: The sandbox ID to mount
            mount_point: Optional mount point path (auto-created if not specified)
            host: NFS server host (uses default if not specified)
            port: NFS server port (uses default if not specified)
            nfs_url: Full NFS URL (overrides host/port/sandbox_id if provided)

        Returns:
            NfsMount context manager

        Example:
            with nfs.mount("sandbox-123") as mount:
                # Access files at mount.mount_point
                with open(f"{mount.mount_point}/test.txt", "w") as f:
                    f.write("Hello")
        """
        if nfs_url:
            # Parse nfs://host:port/path URL
            parsed = urlparse(nfs_url)
            host = parsed.hostname or self.default_host
            port = parsed.port or self.default_port
            export_path = parsed.path
        else:
            host = host or self.default_host
            port = port or self.default_port
            export_path = f"/{sandbox_id}"

        if not host:
            raise ValueError("NFS host not specified and no default configured")

        return NfsMount(
            host=host,
            port=port,
            export_path=export_path,
            mount_point=mount_point,
        )

    @staticmethod
    def is_available() -> bool:
        """Check if NFS mount is available on this system"""
        try:
            result = subprocess.run(
                ["which", "mount.nfs"],
                capture_output=True,
                text=True,
            )
            return result.returncode == 0
        except Exception:
            return False
