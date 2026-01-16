"""
Type definitions for the Workspace SDK
"""

from dataclasses import dataclass, field
from typing import Optional, Dict, List, Literal, Callable, Awaitable
from datetime import datetime


SandboxState = Literal["starting", "running", "stopping", "stopped", "error"]
FileType = Literal["file", "directory", "symlink"]


@dataclass
class Workspace:
    """Workspace resource"""
    id: str
    created_at: str
    updated_at: str
    name: Optional[str] = None
    nfs_url: Optional[str] = None
    metadata: Optional[Dict[str, str]] = None


@dataclass
class CreateWorkspaceParams:
    """Parameters for creating a workspace"""
    name: Optional[str] = None
    metadata: Optional[Dict[str, str]] = None


@dataclass
class Sandbox:
    """Sandbox resource"""
    id: str
    workspace_id: str
    template: str
    state: SandboxState
    created_at: str
    updated_at: str
    name: Optional[str] = None
    env: Optional[Dict[str, str]] = None
    metadata: Optional[Dict[str, str]] = None
    timeout: Optional[int] = None
    error_message: Optional[str] = None


@dataclass
class CreateSandboxParams:
    """Parameters for creating a sandbox"""
    workspace_id: str
    template: Optional[str] = None
    name: Optional[str] = None
    env: Optional[Dict[str, str]] = None
    metadata: Optional[Dict[str, str]] = None
    timeout: Optional[int] = None


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


ProcessEvent = StdoutEvent | StderrEvent | ExitEvent | ErrorEvent


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
    cols: int
    rows: int
    _write: Callable[[bytes], Awaitable[None]] = field(repr=False)
    _resize: Callable[[int, int], Awaitable[None]] = field(repr=False)
    _kill: Callable[[], Awaitable[None]] = field(repr=False)
    _on_data: Callable[[Callable[[bytes], None]], None] = field(repr=False)
    _on_close: Callable[[Callable[[], None]], None] = field(repr=False)

    async def write(self, data: bytes | str) -> None:
        """Write data to the PTY"""
        if isinstance(data, str):
            data = data.encode()
        await self._write(data)

    async def resize(self, cols: int, rows: int) -> None:
        """Resize the PTY"""
        await self._resize(cols, rows)

    async def kill(self) -> None:
        """Kill the PTY"""
        await self._kill()

    def on_data(self, callback: Callable[[bytes], None]) -> None:
        """Register a callback for data events"""
        self._on_data(callback)

    def on_close(self, callback: Callable[[], None]) -> None:
        """Register a callback for close events"""
        self._on_close(callback)


@dataclass
class FileInfo:
    """File information"""
    name: str
    path: str
    type: FileType
    size: int
    modified_at: Optional[str] = None
