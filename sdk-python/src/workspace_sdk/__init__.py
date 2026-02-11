"""
Elevo Workspace SDK - Python Client

A Python SDK for interacting with the Workspace service via gRPC.
"""

from workspace_sdk.client import WorkspaceClient
from workspace_sdk.async_client import AsyncWorkspaceClient
from workspace_sdk.types import (
    Workspace,
    CreateWorkspaceParams,
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
    NotFoundError,
    SandboxNotFoundError,
    WorkspaceNotFoundError,
    TemplateNotFoundError,
    FileNotFoundError,
    PermissionDeniedError,
    ProcessTimeoutError,
    PtyNotFoundError,
    AgentNotConnectedError,
)
from workspace_sdk.services.pty import PtySession

__version__ = "0.2.0"

__all__ = [
    # Clients
    "WorkspaceClient",
    "AsyncWorkspaceClient",
    # Types
    "Workspace",
    "CreateWorkspaceParams",
    "Sandbox",
    "SandboxState",
    "CreateSandboxParams",
    "CommandResult",
    "RunCommandOptions",
    "ProcessEvent",
    "PtyOptions",
    "PtyHandle",
    "PtySession",
    "FileInfo",
    "FileType",
    # Errors
    "WorkspaceError",
    "NotFoundError",
    "SandboxNotFoundError",
    "WorkspaceNotFoundError",
    "TemplateNotFoundError",
    "FileNotFoundError",
    "PermissionDeniedError",
    "ProcessTimeoutError",
    "PtyNotFoundError",
    "AgentNotConnectedError",
]
