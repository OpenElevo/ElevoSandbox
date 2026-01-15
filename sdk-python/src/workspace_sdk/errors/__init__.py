"""
Error classes for the Workspace SDK
"""

from typing import Optional


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


class SandboxNotFoundError(WorkspaceError):
    """Sandbox not found error"""

    def __init__(self, sandbox_id: str):
        super().__init__(f"Sandbox not found: {sandbox_id}", 2001)
        self.sandbox_id = sandbox_id


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
