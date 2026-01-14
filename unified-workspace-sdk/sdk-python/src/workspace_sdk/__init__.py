"""
Unified Workspace SDK - Python Client

A Python SDK for interacting with the Workspace service.
Supports both synchronous and asynchronous APIs.
"""

from workspace_sdk.client import WorkspaceClient
from workspace_sdk.async_client import AsyncWorkspaceClient
from workspace_sdk.types import (
    Sandbox,
    SandboxState,
    CreateSandboxParams,
    CommandResult,
    RunCommandOptions,
    ProcessEvent,
    PtyOptions,
    PtyHandle,
    FileInfo,
    FileType,
)
from workspace_sdk.errors import (
    WorkspaceError,
    SandboxNotFoundError,
    TemplateNotFoundError,
    FileNotFoundError,
    PermissionDeniedError,
    ProcessTimeoutError,
    PtyNotFoundError,
    AgentNotConnectedError,
)

__version__ = "0.1.0"

__all__ = [
    # Clients
    "WorkspaceClient",
    "AsyncWorkspaceClient",
    # Types
    "Sandbox",
    "SandboxState",
    "CreateSandboxParams",
    "CommandResult",
    "RunCommandOptions",
    "ProcessEvent",
    "PtyOptions",
    "PtyHandle",
    "FileInfo",
    "FileType",
    # Errors
    "WorkspaceError",
    "SandboxNotFoundError",
    "TemplateNotFoundError",
    "FileNotFoundError",
    "PermissionDeniedError",
    "ProcessTimeoutError",
    "PtyNotFoundError",
    "AgentNotConnectedError",
]
