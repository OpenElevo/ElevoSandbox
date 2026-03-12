"""
Type definitions for the Workspace SDK
"""

from dataclasses import dataclass, field
from typing import Optional, Dict, List, Literal, Callable, Awaitable, Union
from enum import Enum


class StorageType(str, Enum):
    """Storage type for a workspace"""
    MANAGED = "managed"
    REMOTE = "remote"


class SandboxState(str, Enum):
    """Sandbox state enum"""
    UNKNOWN = "unknown"
    STARTING = "starting"
    RUNNING = "running"
    STOPPING = "stopping"
    STOPPED = "stopped"
    FAILED = "failed"


FileType = Literal["file", "directory", "symlink"]


@dataclass
class Workspace:
    """Workspace resource"""
    id: str
    created_at: Optional[str] = None
    updated_at: Optional[str] = None
    name: Optional[str] = None
    nfs_url: Optional[str] = None
    storage_type: Optional[str] = None
    storage_config: Optional[str] = None
    metadata: Optional[Dict[str, str]] = None


@dataclass
class CreateWorkspaceParams:
    """Parameters for creating a workspace"""
    name: Optional[str] = None
    storage_type: Optional[str] = None
    metadata: Optional[Dict[str, str]] = None


@dataclass
class MountRequest:
    """Mount request for attaching a share to a sandbox"""
    share_id: str
    mount_path: str


@dataclass
class SandboxMount:
    """Sandbox mount info"""
    sandbox_id: str
    share_id: str
    mount_path: str


@dataclass
class Sandbox:
    """Sandbox resource"""
    id: str
    template: str
    state: SandboxState
    workspace_id: Optional[str] = None  # Deprecated: use namespace_id
    namespace_id: Optional[str] = None
    created_at: Optional[str] = None
    updated_at: Optional[str] = None
    name: Optional[str] = None
    root_path: Optional[str] = None
    env: Optional[Dict[str, str]] = None
    metadata: Optional[Dict[str, str]] = None
    timeout: Optional[int] = None
    error_message: Optional[str] = None
    mounts: Optional[List[SandboxMount]] = None


@dataclass
class CreateSandboxParams:
    """Parameters for creating a sandbox"""
    workspace_id: Optional[str] = None  # Deprecated: use namespace_id
    namespace_id: Optional[str] = None
    template: Optional[str] = None
    name: Optional[str] = None
    root_path: Optional[str] = None
    env: Optional[Dict[str, str]] = None
    metadata: Optional[Dict[str, str]] = None
    timeout: Optional[int] = None
    mounts: Optional[List[MountRequest]] = None


@dataclass
class CommandResult:
    """Command execution result"""
    exit_code: int
    stdout: str
    stderr: str


@dataclass
class RunCommandOptions:
    """Options for running a command"""
    args: Optional[List[str]] = None
    env: Optional[Dict[str, str]] = None
    cwd: Optional[str] = None
    timeout: Optional[int] = None


@dataclass
class StdoutEvent:
    """Standard output event"""
    type: Literal["stdout"]
    data: str


@dataclass
class StderrEvent:
    """Standard error event"""
    type: Literal["stderr"]
    data: str


@dataclass
class ExitEvent:
    """Process exit event"""
    type: Literal["exit"]
    code: int


@dataclass
class ErrorEvent:
    """Error event"""
    type: Literal["error"]
    message: str


ProcessEvent = Union[StdoutEvent, StderrEvent, ExitEvent, ErrorEvent]


@dataclass
class PtyOptions:
    """PTY creation options"""
    cols: int = 80
    rows: int = 24
    shell: Optional[str] = None
    env: Optional[Dict[str, str]] = None


@dataclass
class PtyHandle:
    """PTY handle for interacting with a terminal"""
    id: str
    sandbox_id: str
    cols: int
    rows: int


@dataclass
class FileInfo:
    """File information"""
    name: str
    path: str
    type: FileType
    size: int
    modified_at: Optional[str] = None
