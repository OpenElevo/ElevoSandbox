"""
FUSE service for mounting workspaces locally via workspace-fuse client.

This service automatically downloads the workspace-fuse binary and manages
FUSE mounts for workspaces.
"""

import hashlib
import os
import platform
import shutil
import signal
import stat
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Dict, Optional
from urllib.request import urlopen, Request
from urllib.error import URLError, HTTPError


# Default version and download URL template
DEFAULT_VERSION = "latest"
GITHUB_RELEASE_URL = "https://github.com/elevo-ai/elevo-workspace/releases/download/{version}/workspace-fuse-{platform}-{arch}"
GITHUB_LATEST_URL = "https://github.com/elevo-ai/elevo-workspace/releases/latest/download/workspace-fuse-{platform}-{arch}"


def get_platform_info() -> tuple[str, str]:
    """Get current platform and architecture."""
    system = platform.system().lower()
    machine = platform.machine().lower()

    # Normalize platform
    if system == "darwin":
        plat = "darwin"
    elif system == "linux":
        plat = "linux"
    else:
        raise RuntimeError(f"Unsupported platform: {system}")

    # Normalize architecture
    if machine in ("x86_64", "amd64"):
        arch = "amd64"
    elif machine in ("aarch64", "arm64"):
        arch = "arm64"
    else:
        raise RuntimeError(f"Unsupported architecture: {machine}")

    return plat, arch


def get_bin_dir() -> Path:
    """Get the directory for storing workspace-fuse binary."""
    # Try ~/.elevo/bin first
    home = Path.home()
    bin_dir = home / ".elevo" / "bin"

    # Fall back to /usr/local/bin if we have write access
    if not bin_dir.parent.exists():
        try:
            bin_dir.parent.mkdir(parents=True, exist_ok=True)
        except PermissionError:
            pass

    if bin_dir.parent.exists():
        bin_dir.mkdir(parents=True, exist_ok=True)
        return bin_dir

    # Try /usr/local/bin
    usr_local = Path("/usr/local/bin")
    if usr_local.exists() and os.access(usr_local, os.W_OK):
        return usr_local

    raise RuntimeError("Cannot find writable directory for workspace-fuse binary")


def _download_from_url(url: str, dest_path: Path, proxy: Optional[str] = None) -> bool:
    """
    Download file from URL to destination path.

    Returns True if successful, False otherwise.
    """
    try:
        request = Request(url)
        if proxy:
            os.environ["http_proxy"] = proxy
            os.environ["https_proxy"] = proxy

        with urlopen(request, timeout=60) as response:
            with open(dest_path, "wb") as f:
                shutil.copyfileobj(response, f)
        return True
    except (URLError, HTTPError, OSError):
        return False


def _try_download_from_server(
    server_url: str,
    dest_path: Path,
    proxy: Optional[str] = None,
) -> bool:
    """
    Try to download workspace-fuse binary from workspace server.

    Args:
        server_url: Base server URL (e.g., http://localhost:8080)
        dest_path: Destination path for the binary
        proxy: HTTP proxy URL (optional)

    Returns:
        True if download succeeded, False otherwise
    """
    plat, arch = get_platform_info()

    # Convert gRPC URL to HTTP URL if needed
    # gRPC is typically on port 9090, HTTP on 8080
    http_url = server_url
    if ":9090" in http_url or ":19090" in http_url:
        http_url = http_url.replace(":9090", ":8080").replace(":19090", ":18080")

    # Build download URL
    download_url = f"{http_url}/api/v1/downloads/workspace-fuse/{plat}/{arch}"

    return _download_from_url(download_url, dest_path, proxy)


def download_binary(
    version: str = DEFAULT_VERSION,
    proxy: Optional[str] = None,
    server_url: Optional[str] = None,
) -> Path:
    """
    Download workspace-fuse binary for current platform.

    Download priority:
    1. From workspace server (if server_url provided and binary available)
    2. From GitHub Releases (fallback)

    Args:
        version: Version to download (default: "latest")
        proxy: HTTP proxy URL (optional)
        server_url: Workspace server URL for downloading binary (optional)

    Returns:
        Path to the downloaded binary
    """
    plat, arch = get_platform_info()
    bin_dir = get_bin_dir()
    bin_path = bin_dir / "workspace-fuse"
    temp_path = bin_path.with_suffix(".tmp")

    try:
        downloaded = False

        # Try server first if URL provided
        if server_url:
            downloaded = _try_download_from_server(server_url, temp_path, proxy)

        # Fallback to GitHub
        if not downloaded:
            if version == "latest":
                url = GITHUB_LATEST_URL.format(platform=plat, arch=arch)
            else:
                url = GITHUB_RELEASE_URL.format(version=version, platform=plat, arch=arch)

            if not _download_from_url(url, temp_path, proxy):
                raise RuntimeError(f"Failed to download workspace-fuse from both server and GitHub")

        # Make executable
        temp_path.chmod(temp_path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)

        # Verify it's a valid executable
        result = subprocess.run(
            [str(temp_path), "--version"],
            capture_output=True,
            timeout=10,
        )
        if result.returncode != 0:
            raise RuntimeError(f"Downloaded binary is not valid: {result.stderr.decode()}")

        # Move to final location
        temp_path.rename(bin_path)
        return bin_path

    except Exception:
        if temp_path.exists():
            temp_path.unlink()
        raise


def ensure_binary(
    version: str = DEFAULT_VERSION,
    force_download: bool = False,
    proxy: Optional[str] = None,
    server_url: Optional[str] = None,
) -> Path:
    """
    Ensure workspace-fuse binary is available.

    Args:
        version: Version to use (default: "latest")
        force_download: Force re-download even if binary exists
        proxy: HTTP proxy URL (optional)
        server_url: Workspace server URL for downloading binary (optional)

    Returns:
        Path to the binary
    """
    bin_dir = get_bin_dir()
    bin_path = bin_dir / "workspace-fuse"

    if bin_path.exists() and not force_download:
        # Verify it works
        try:
            result = subprocess.run(
                [str(bin_path), "--version"],
                capture_output=True,
                timeout=10,
            )
            if result.returncode == 0:
                return bin_path
        except Exception:
            pass

    return download_binary(version=version, proxy=proxy, server_url=server_url)


class FuseMount:
    """Represents an active FUSE mount for a workspace."""

    def __init__(
        self,
        server: str,
        workspace_id: str,
        token: str,
        mount_point: Optional[str] = None,
        binary_path: Optional[Path] = None,
        cache_ttl: int = 5,
        read_cache_size: int = 256,
        block_size: int = 131072,
        debug: bool = False,
    ):
        """
        Initialize FUSE mount.

        Args:
            server: gRPC server URL (e.g., http://localhost:19090)
            workspace_id: Workspace ID to mount
            token: Authentication token
            mount_point: Local mount point (auto-created if not specified)
            binary_path: Path to workspace-fuse binary (auto-detected if not specified)
            cache_ttl: Metadata cache TTL in seconds (default: 5)
            read_cache_size: Read cache size in MB (default: 256)
            block_size: Block size for reads (default: 128KB)
            debug: Enable debug logging
        """
        self.server = server
        self.workspace_id = workspace_id
        self.token = token
        self._mount_point = mount_point
        self._binary_path = binary_path
        self.cache_ttl = cache_ttl
        self.read_cache_size = read_cache_size
        self.block_size = block_size
        self.debug = debug

        self._temp_dir: Optional[tempfile.TemporaryDirectory] = None
        self._process: Optional[subprocess.Popen] = None
        self._mounted = False

    @property
    def mount_point(self) -> str:
        """Get the mount point path."""
        if self._mount_point:
            return self._mount_point
        if self._temp_dir:
            return self._temp_dir.name
        raise RuntimeError("Mount point not initialized")

    @property
    def path(self) -> str:
        """Alias for mount_point."""
        return self.mount_point

    @property
    def is_mounted(self) -> bool:
        """Check if currently mounted."""
        return self._mounted and self._process is not None and self._process.poll() is None

    def _is_fuse_mounted(self, path: str) -> bool:
        """Check if path is a FUSE mount point by checking /proc/mounts."""
        try:
            with open("/proc/mounts", "r") as f:
                for line in f:
                    parts = line.split()
                    if len(parts) >= 2 and parts[1] == path:
                        # Check if it's a FUSE mount
                        if "fuse" in parts[2].lower():
                            return True
            return False
        except Exception:
            # Fallback: check if we can stat the directory and it's different from parent
            try:
                mount_stat = os.stat(path)
                parent_stat = os.stat(os.path.dirname(path))
                # Different device means it's a mount point
                return mount_stat.st_dev != parent_stat.st_dev
            except Exception:
                return False

    def mount(self, timeout: float = 30.0) -> str:
        """
        Mount the workspace.

        Args:
            timeout: Timeout in seconds to wait for mount to be ready

        Returns:
            The mount point path
        """
        if self._mounted:
            return self.mount_point

        # Ensure binary is available
        if self._binary_path is None:
            self._binary_path = ensure_binary()

        # Create mount point if not specified
        if not self._mount_point:
            self._temp_dir = tempfile.TemporaryDirectory(prefix="workspace_fuse_")
            mount_path = self._temp_dir.name
        else:
            mount_path = self._mount_point
            Path(mount_path).mkdir(parents=True, exist_ok=True)

        # Build command
        cmd = [
            str(self._binary_path),
            "mount",
            "--server", self.server,
            "--workspace", self.workspace_id,
            "--target", mount_path,
            "--foreground",
            "--cache-ttl", str(self.cache_ttl),
            "--read-cache-size", str(self.read_cache_size),
            "--block-size", str(self.block_size),
        ]

        # Token is optional
        if self.token:
            cmd.extend(["--token", self.token])

        if self.debug:
            cmd.append("--debug")

        # Start the FUSE process
        try:
            self._process = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )
        except Exception as e:
            if self._temp_dir:
                self._temp_dir.cleanup()
                self._temp_dir = None
            raise RuntimeError(f"Failed to start workspace-fuse: {e}") from e

        # Wait for mount to be ready
        start_time = time.time()
        while time.time() - start_time < timeout:
            # Check if process died
            if self._process.poll() is not None:
                stderr = self._process.stderr.read().decode() if self._process.stderr else ""
                if self._temp_dir:
                    self._temp_dir.cleanup()
                    self._temp_dir = None
                raise RuntimeError(f"workspace-fuse exited unexpectedly: {stderr}")

            # Check if mount is ready by verifying it's actually a FUSE mount
            if self._is_fuse_mounted(mount_path):
                self._mounted = True
                self._mount_point = mount_path
                return mount_path

            time.sleep(0.1)

        # Timeout
        self._cleanup()
        raise RuntimeError(f"Timeout waiting for mount to be ready after {timeout}s")

    def unmount(self) -> None:
        """Unmount the workspace."""
        if not self._mounted:
            return

        self._cleanup()

    def _cleanup(self) -> None:
        """Clean up resources."""
        # Terminate the FUSE process
        if self._process is not None:
            try:
                self._process.terminate()
                self._process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self._process.kill()
                self._process.wait()
            self._process = None

        # Try fusermount as fallback
        if self._mount_point:
            try:
                subprocess.run(
                    ["fusermount", "-u", self._mount_point],
                    capture_output=True,
                    timeout=5,
                )
            except Exception:
                # Try lazy unmount
                try:
                    subprocess.run(
                        ["fusermount", "-uz", self._mount_point],
                        capture_output=True,
                        timeout=5,
                    )
                except Exception:
                    pass

        self._mounted = False

        # Clean up temp directory
        if self._temp_dir:
            try:
                self._temp_dir.cleanup()
            except Exception:
                pass
            self._temp_dir = None

    def __enter__(self) -> "FuseMount":
        self.mount()
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.unmount()

    def __del__(self) -> None:
        self._cleanup()


class FuseService:
    """Service for managing FUSE mounts for workspaces."""

    def __init__(
        self,
        server: str,
        default_token: Optional[str] = None,
        binary_version: str = DEFAULT_VERSION,
        proxy: Optional[str] = None,
        http_server: Optional[str] = None,
    ):
        """
        Initialize FUSE service.

        Args:
            server: gRPC server URL (e.g., http://localhost:19090)
            default_token: Default authentication token
            binary_version: workspace-fuse version to use
            proxy: HTTP proxy for downloading binary
            http_server: HTTP server URL for downloading binary (optional, auto-derived from server if not set)
        """
        self.server = server
        self.default_token = default_token
        self.binary_version = binary_version
        self.proxy = proxy
        self.http_server = http_server or server
        self._binary_path: Optional[Path] = None
        self._mounts: Dict[str, FuseMount] = {}

    def _ensure_binary(self) -> Path:
        """Ensure binary is available."""
        if self._binary_path is None:
            self._binary_path = ensure_binary(
                version=self.binary_version,
                proxy=self.proxy,
                server_url=self.http_server,
            )
        return self._binary_path

    def mount(
        self,
        workspace_id: str,
        token: Optional[str] = None,
        mount_point: Optional[str] = None,
        cache_ttl: int = 5,
        read_cache_size: int = 256,
        block_size: int = 131072,
        debug: bool = False,
    ) -> FuseMount:
        """
        Mount a workspace via FUSE.

        Args:
            workspace_id: Workspace ID to mount
            token: Authentication token (uses default if not specified)
            mount_point: Local mount point (auto-created if not specified)
            cache_ttl: Metadata cache TTL in seconds (default: 5)
            read_cache_size: Read cache size in MB (default: 256)
            block_size: Block size for reads (default: 128KB)
            debug: Enable debug logging

        Returns:
            FuseMount instance

        Example:
            with fuse.mount("workspace-123") as ws:
                # Access files at ws.path
                with open(f"{ws.path}/test.txt", "w") as f:
                    f.write("Hello")
        """
        token = token or self.default_token
        # Token is now optional - server may not require authentication

        # Check if already mounted
        if workspace_id in self._mounts:
            existing = self._mounts[workspace_id]
            if existing.is_mounted:
                return existing
            else:
                del self._mounts[workspace_id]

        mount = FuseMount(
            server=self.server,
            workspace_id=workspace_id,
            token=token,
            mount_point=mount_point,
            binary_path=self._ensure_binary(),
            cache_ttl=cache_ttl,
            read_cache_size=read_cache_size,
            block_size=block_size,
            debug=debug,
        )

        self._mounts[workspace_id] = mount
        return mount

    def unmount(self, workspace_id: str) -> None:
        """
        Unmount a workspace.

        Args:
            workspace_id: Workspace ID to unmount
        """
        if workspace_id in self._mounts:
            self._mounts[workspace_id].unmount()
            del self._mounts[workspace_id]

    def unmount_all(self) -> None:
        """Unmount all workspaces."""
        for mount in list(self._mounts.values()):
            mount.unmount()
        self._mounts.clear()

    def list_mounts(self) -> Dict[str, str]:
        """
        List all active mounts.

        Returns:
            Dict mapping workspace_id to mount_point
        """
        return {
            ws_id: mount.mount_point
            for ws_id, mount in self._mounts.items()
            if mount.is_mounted
        }

    @staticmethod
    def is_available() -> bool:
        """Check if FUSE is available on this system."""
        # Check for fusermount
        try:
            result = subprocess.run(
                ["which", "fusermount"],
                capture_output=True,
            )
            if result.returncode != 0:
                return False
        except Exception:
            return False

        # Check for /dev/fuse
        if not Path("/dev/fuse").exists():
            return False

        return True

    def __del__(self) -> None:
        self.unmount_all()
