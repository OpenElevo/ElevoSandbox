"""
Async Workspace Client - Main entry point for async SDK usage with gRPC
"""

from typing import Optional, List, AsyncIterator

import grpc
import grpc.aio

from workspace_sdk.types import (
    Workspace,
    CreateWorkspaceParams,
    Sandbox,
    SandboxState,
    CreateSandboxParams,
    CommandResult,
    RunCommandOptions,
    ProcessEvent,
    StdoutEvent,
    StderrEvent,
    ExitEvent,
    ErrorEvent,
    PtyOptions,
    PtyHandle,
    FileInfo,
)
from workspace_sdk.errors import convert_grpc_error
from workspace_sdk.proto.workspace.v1 import (
    workspace_pb2,
    workspace_pb2_grpc,
    sandbox_pb2,
    sandbox_pb2_grpc,
    process_pb2,
    process_pb2_grpc,
    pty_pb2,
    pty_pb2_grpc,
)


def _create_metadata(api_key: Optional[str]) -> List[tuple]:
    """Create gRPC metadata with auth token"""
    if api_key:
        return [("authorization", f"Bearer {api_key}")]
    return []


class AsyncWorkspaceService:
    """Async service for managing workspaces via gRPC"""

    def __init__(
        self,
        stub: workspace_pb2_grpc.WorkspaceServiceStub,
        api_key: Optional[str],
        timeout: float,
    ):
        self._stub = stub
        self._api_key = api_key
        self._timeout = timeout

    def _metadata(self) -> List[tuple]:
        return _create_metadata(self._api_key)

    async def create(self, params: Optional[CreateWorkspaceParams] = None) -> Workspace:
        """Create a new workspace"""
        req = workspace_pb2.CreateWorkspaceRequest()
        if params:
            if params.name:
                req.name = params.name
            if params.metadata:
                req.metadata.update(params.metadata)

        try:
            resp = await self._stub.CreateWorkspace(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_workspace(resp.workspace)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def get(self, workspace_id: str) -> Workspace:
        """Get a workspace by ID"""
        req = workspace_pb2.GetWorkspaceRequest(id=workspace_id)
        try:
            resp = await self._stub.GetWorkspace(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_workspace(resp.workspace)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def list(self) -> List[Workspace]:
        """List all workspaces"""
        req = workspace_pb2.ListWorkspacesRequest()
        try:
            resp = await self._stub.ListWorkspaces(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return [self._transform_workspace(w) for w in resp.workspaces]
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def delete(self, workspace_id: str) -> None:
        """Delete a workspace"""
        req = workspace_pb2.DeleteWorkspaceRequest(id=workspace_id)
        try:
            await self._stub.DeleteWorkspace(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def _transform_workspace(self, ws) -> Workspace:
        """Transform proto Workspace to SDK Workspace type"""
        created_at = None
        updated_at = None
        if ws.created_at:
            created_at = ws.created_at.ToDatetime().isoformat()
        if ws.updated_at:
            updated_at = ws.updated_at.ToDatetime().isoformat()

        return Workspace(
            id=ws.id,
            name=ws.name if ws.HasField("name") else None,
            nfs_url=ws.nfs_url if ws.HasField("nfs_url") else None,
            metadata=dict(ws.metadata) if ws.metadata else None,
            created_at=created_at,
            updated_at=updated_at,
        )


class AsyncSandboxService:
    """Async service for managing sandboxes via gRPC"""

    def __init__(
        self,
        stub: sandbox_pb2_grpc.SandboxServiceStub,
        api_key: Optional[str],
        timeout: float,
    ):
        self._stub = stub
        self._api_key = api_key
        self._timeout = timeout

    def _metadata(self) -> List[tuple]:
        return _create_metadata(self._api_key)

    async def create(self, params: CreateSandboxParams) -> Sandbox:
        """Create a new sandbox"""
        req = sandbox_pb2.CreateSandboxRequest(workspace_id=params.workspace_id)
        if params.template:
            req.template = params.template
        if params.name:
            req.name = params.name
        if params.env:
            req.env.update(params.env)
        if params.metadata:
            req.metadata.update(params.metadata)
        if params.timeout:
            req.timeout = params.timeout

        try:
            resp = await self._stub.CreateSandbox(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_sandbox(resp.sandbox)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def get(self, sandbox_id: str) -> Sandbox:
        """Get a sandbox by ID"""
        req = sandbox_pb2.GetSandboxRequest(id=sandbox_id)
        try:
            resp = await self._stub.GetSandbox(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_sandbox(resp.sandbox)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def list(self, state: Optional[SandboxState] = None) -> List[Sandbox]:
        """List all sandboxes"""
        req = sandbox_pb2.ListSandboxesRequest()
        if state:
            req.state = self._state_to_proto(state)

        try:
            resp = await self._stub.ListSandboxes(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return [self._transform_sandbox(s) for s in resp.sandboxes]
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def delete(self, sandbox_id: str, force: bool = False) -> None:
        """Delete a sandbox"""
        req = sandbox_pb2.DeleteSandboxRequest(id=sandbox_id, force=force)
        try:
            await self._stub.DeleteSandbox(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def _transform_sandbox(self, sb) -> Sandbox:
        """Transform proto Sandbox to SDK Sandbox type"""
        created_at = None
        updated_at = None
        if sb.created_at:
            created_at = sb.created_at.ToDatetime().isoformat()
        if sb.updated_at:
            updated_at = sb.updated_at.ToDatetime().isoformat()

        return Sandbox(
            id=sb.id,
            workspace_id=sb.workspace_id,
            name=sb.name if sb.HasField("name") else None,
            template=sb.template,
            state=self._proto_to_state(sb.state),
            env=dict(sb.env) if sb.env else None,
            metadata=dict(sb.metadata) if sb.metadata else None,
            created_at=created_at,
            updated_at=updated_at,
            timeout=sb.timeout if sb.timeout else None,
            error_message=sb.error_message if sb.HasField("error_message") else None,
        )

    def _proto_to_state(self, state: int) -> SandboxState:
        """Convert proto SandboxState to SDK SandboxState"""
        state_map = {
            sandbox_pb2.SANDBOX_STATE_STARTING: SandboxState.STARTING,
            sandbox_pb2.SANDBOX_STATE_RUNNING: SandboxState.RUNNING,
            sandbox_pb2.SANDBOX_STATE_STOPPING: SandboxState.STOPPING,
            sandbox_pb2.SANDBOX_STATE_STOPPED: SandboxState.STOPPED,
            sandbox_pb2.SANDBOX_STATE_ERROR: SandboxState.FAILED,
        }
        return state_map.get(state, SandboxState.UNKNOWN)

    def _state_to_proto(self, state: SandboxState) -> int:
        """Convert SDK SandboxState to proto SandboxState"""
        state_map = {
            SandboxState.STARTING: sandbox_pb2.SANDBOX_STATE_STARTING,
            SandboxState.RUNNING: sandbox_pb2.SANDBOX_STATE_RUNNING,
            SandboxState.STOPPING: sandbox_pb2.SANDBOX_STATE_STOPPING,
            SandboxState.STOPPED: sandbox_pb2.SANDBOX_STATE_STOPPED,
            SandboxState.FAILED: sandbox_pb2.SANDBOX_STATE_ERROR,
        }
        return state_map.get(state, sandbox_pb2.SANDBOX_STATE_UNSPECIFIED)


class AsyncProcessService:
    """Async service for executing commands via gRPC"""

    def __init__(
        self,
        stub: process_pb2_grpc.ProcessServiceStub,
        api_key: Optional[str],
        timeout: float,
    ):
        self._stub = stub
        self._api_key = api_key
        self._timeout = timeout

    def _metadata(self) -> List[tuple]:
        return _create_metadata(self._api_key)

    async def run(
        self,
        sandbox_id: str,
        command: str,
        options: Optional[RunCommandOptions] = None,
    ) -> CommandResult:
        """Run a command and wait for completion"""
        req = process_pb2.RunCommandRequest(
            sandbox_id=sandbox_id,
            command=command,
            args=options.args if options and options.args else [],
            env=options.env if options and options.env else {},
        )
        if options and options.cwd:
            req.cwd = options.cwd
        if options and options.timeout:
            req.timeout_ms = options.timeout

        try:
            resp = await self._stub.RunCommand(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return CommandResult(
                exit_code=resp.result.exit_code,
                stdout=resp.result.stdout,
                stderr=resp.result.stderr,
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def run_stream(
        self,
        sandbox_id: str,
        command: str,
        options: Optional[RunCommandOptions] = None,
    ) -> AsyncIterator[ProcessEvent]:
        """Run a command with streaming output"""
        req = process_pb2.RunCommandRequest(
            sandbox_id=sandbox_id,
            command=command,
            args=options.args if options and options.args else [],
            env=options.env if options and options.env else {},
        )
        if options and options.cwd:
            req.cwd = options.cwd
        if options and options.timeout:
            req.timeout_ms = options.timeout

        try:
            stream = self._stub.RunCommandStream(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            async for event in stream:
                parsed = self._parse_event(event)
                if parsed:
                    yield parsed
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def kill(
        self,
        sandbox_id: str,
        pid: int,
        signal: int = 15,
    ) -> None:
        """Kill a running process"""
        req = process_pb2.KillProcessRequest(
            sandbox_id=sandbox_id,
            pid=pid,
            signal=signal,
        )
        try:
            await self._stub.KillProcess(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def _parse_event(self, event) -> Optional[ProcessEvent]:
        """Parse proto ProcessEvent into SDK ProcessEvent"""
        which = event.WhichOneof("event")
        if which == "stdout":
            return StdoutEvent(type="stdout", data=event.stdout.data)
        elif which == "stderr":
            return StderrEvent(type="stderr", data=event.stderr.data)
        elif which == "exit":
            return ExitEvent(type="exit", code=event.exit.code)
        elif which == "error":
            return ErrorEvent(type="error", message=event.error.message)
        return None


class AsyncPtyService:
    """Async service for managing PTY sessions via gRPC"""

    def __init__(
        self,
        stub: pty_pb2_grpc.PtyServiceStub,
        api_key: Optional[str],
        timeout: float,
    ):
        self._stub = stub
        self._api_key = api_key
        self._timeout = timeout

    def _metadata(self) -> List[tuple]:
        return _create_metadata(self._api_key)

    async def create(
        self,
        sandbox_id: str,
        options: Optional[PtyOptions] = None,
    ) -> PtyHandle:
        """Create a new PTY"""
        opts = options or PtyOptions()
        req = pty_pb2.CreatePtyRequest(
            sandbox_id=sandbox_id,
            cols=opts.cols or 80,
            rows=opts.rows or 24,
        )
        if opts.shell:
            req.shell = opts.shell
        if opts.env:
            req.env.update(opts.env)

        try:
            resp = await self._stub.CreatePty(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return PtyHandle(
                id=resp.pty.id,
                sandbox_id=resp.pty.sandbox_id,
                cols=resp.pty.cols,
                rows=resp.pty.rows,
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def resize(
        self,
        sandbox_id: str,
        pty_id: str,
        cols: int,
        rows: int,
    ) -> None:
        """Resize a PTY"""
        req = pty_pb2.ResizePtyRequest(
            sandbox_id=sandbox_id,
            pty_id=pty_id,
            cols=cols,
            rows=rows,
        )
        try:
            await self._stub.ResizePty(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    async def kill(self, sandbox_id: str, pty_id: str) -> None:
        """Kill a PTY"""
        req = pty_pb2.KillPtyRequest(
            sandbox_id=sandbox_id,
            pty_id=pty_id,
        )
        try:
            await self._stub.KillPty(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)


class AsyncWorkspaceClient:
    """Async client for interacting with the Workspace service via gRPC"""

    def __init__(
        self,
        server_addr: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ):
        """
        Initialize the async workspace client.

        Args:
            server_addr: gRPC server address (e.g., "localhost:9090")
            api_key: Optional API key for authentication
            timeout: Request timeout in seconds (default: 30)
        """
        self._server_addr = server_addr
        self._api_key = api_key
        self._timeout = timeout
        self._channel: Optional[grpc.aio.Channel] = None

        # Services will be initialized when context manager is entered
        self.workspace: AsyncWorkspaceService
        self.sandbox: AsyncSandboxService
        self.process: AsyncProcessService
        self.pty: AsyncPtyService

    async def __aenter__(self) -> "AsyncWorkspaceClient":
        """Enter async context manager"""
        self._channel = grpc.aio.insecure_channel(self._server_addr)

        # Create stubs
        workspace_stub = workspace_pb2_grpc.WorkspaceServiceStub(self._channel)
        sandbox_stub = sandbox_pb2_grpc.SandboxServiceStub(self._channel)
        process_stub = process_pb2_grpc.ProcessServiceStub(self._channel)
        pty_stub = pty_pb2_grpc.PtyServiceStub(self._channel)

        # Initialize services
        self.workspace = AsyncWorkspaceService(
            workspace_stub, self._api_key, self._timeout
        )
        self.sandbox = AsyncSandboxService(sandbox_stub, self._api_key, self._timeout)
        self.process = AsyncProcessService(process_stub, self._api_key, self._timeout)
        self.pty = AsyncPtyService(pty_stub, self._api_key, self._timeout)

        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        """Exit async context manager"""
        if self._channel:
            await self._channel.close()
            self._channel = None

    @staticmethod
    def create(
        server_addr: str,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ) -> "AsyncWorkspaceClient":
        """
        Factory method to create an AsyncWorkspaceClient.

        Usage:
            async with AsyncWorkspaceClient.create("localhost:9090") as client:
                workspace = await client.workspace.create()
                sandbox = await client.sandbox.create(CreateSandboxParams(workspace_id=workspace.id))
        """
        return AsyncWorkspaceClient(server_addr, api_key, timeout)
