"""
Error classes for the Workspace SDK
"""

from typing import Optional

import grpc


class WorkspaceError(Exception):
    """Base error class for Workspace SDK errors"""

    def __init__(self, message: str, code: int, details: Optional[str] = None):
        super().__init__(message)
        self.message = message
        self.code = code
        self.details = details

    def __str__(self) -> str:
        if self.details:
            return f"{self.message} (code: {self.code}, details: {self.details})"
        return f"{self.message} (code: {self.code})"


class NotFoundError(WorkspaceError):
    """Resource not found error"""

    def __init__(self, message: str):
        super().__init__(message, 404)


class SandboxNotFoundError(WorkspaceError):
    """Sandbox not found error"""

    def __init__(self, sandbox_id: str):
        super().__init__(f"Sandbox not found: {sandbox_id}", 2001)
        self.sandbox_id = sandbox_id


class WorkspaceNotFoundError(WorkspaceError):
    """Workspace not found error"""

    def __init__(self, workspace_id: str):
        super().__init__(f"Workspace not found: {workspace_id}", 2002)
        self.workspace_id = workspace_id


class TemplateNotFoundError(WorkspaceError):
    """Template not found error"""

    def __init__(self, template: str):
        super().__init__(f"Template not found: {template}", 2003)
        self.template = template


class FileNotFoundError(WorkspaceError):
    """File not found error"""

    def __init__(self, path: str):
        super().__init__(f"File not found: {path}", 3001)
        self.path = path


class PermissionDeniedError(WorkspaceError):
    """Permission denied error"""

    def __init__(self, path: str):
        super().__init__(f"Permission denied: {path}", 3003)
        self.path = path


class ProcessTimeoutError(WorkspaceError):
    """Process timeout error"""

    def __init__(self) -> None:
        super().__init__("Process timeout", 4002)


class PtyNotFoundError(WorkspaceError):
    """PTY not found error"""

    def __init__(self, pty_id: str):
        super().__init__(f"PTY not found: {pty_id}", 4101)
        self.pty_id = pty_id


class AgentNotConnectedError(WorkspaceError):
    """Agent not connected error"""

    def __init__(self, sandbox_id: str):
        super().__init__(f"Agent not connected for sandbox: {sandbox_id}", 5001)
        self.sandbox_id = sandbox_id


def parse_error_response(response_data: dict) -> WorkspaceError:
    """Parse error response from API into appropriate error class"""
    code = response_data.get("code", 1000)
    message = response_data.get("message", "Unknown error")
    details = response_data.get("details")

    error_map = {
        2001: lambda: SandboxNotFoundError(message.replace("Sandbox not found: ", "")),
        2003: lambda: TemplateNotFoundError(message.replace("Template not found: ", "")),
        3001: lambda: FileNotFoundError(message.replace("File not found: ", "")),
        3003: lambda: PermissionDeniedError(message.replace("Permission denied: ", "")),
        4002: lambda: ProcessTimeoutError(),
        4101: lambda: PtyNotFoundError(message.replace("PTY not found: ", "")),
        5001: lambda: AgentNotConnectedError(message.replace("Agent not connected for sandbox: ", "")),
    }

    if code in error_map:
        return error_map[code]()

    return WorkspaceError(message, code, details)


def convert_grpc_error(error: grpc.RpcError) -> WorkspaceError:
    """Convert gRPC error to appropriate WorkspaceError"""
    code = error.code()
    details = error.details()

    if code == grpc.StatusCode.NOT_FOUND:
        return NotFoundError(details or "Resource not found")
    elif code == grpc.StatusCode.PERMISSION_DENIED:
        return PermissionDeniedError(details or "Permission denied")
    elif code == grpc.StatusCode.DEADLINE_EXCEEDED:
        return ProcessTimeoutError()
    elif code == grpc.StatusCode.UNAVAILABLE:
        return AgentNotConnectedError(details or "Service unavailable")
    elif code == grpc.StatusCode.INVALID_ARGUMENT:
        return WorkspaceError(details or "Invalid argument", 400)
    elif code == grpc.StatusCode.FAILED_PRECONDITION:
        return WorkspaceError(details or "Failed precondition", 412)
    else:
        return WorkspaceError(details or str(error), int(code.value[0]))
