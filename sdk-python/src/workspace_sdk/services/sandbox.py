"""
Sandbox service for managing sandbox lifecycle via gRPC
"""

from typing import Optional, List

import grpc

from workspace_sdk.types import Sandbox, SandboxState, CreateSandboxParams
from workspace_sdk.errors import convert_grpc_error
from workspace_sdk.proto.workspace.v1 import sandbox_pb2, sandbox_pb2_grpc


def _create_metadata(api_key: Optional[str]) -> List[tuple]:
    """Create gRPC metadata with auth token"""
    if api_key:
        return [("authorization", f"Bearer {api_key}")]
    return []


class SandboxService:
    """Sync service for managing sandboxes via gRPC"""

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

    def create(self, params: CreateSandboxParams) -> Sandbox:
        """Create a new sandbox bound to a workspace"""
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
            resp = self._stub.CreateSandbox(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_sandbox(resp.sandbox)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def get(self, sandbox_id: str) -> Sandbox:
        """Get a sandbox by ID"""
        req = sandbox_pb2.GetSandboxRequest(id=sandbox_id)
        try:
            resp = self._stub.GetSandbox(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return self._transform_sandbox(resp.sandbox)
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def list(self, state: Optional[SandboxState] = None) -> List[Sandbox]:
        """List all sandboxes"""
        req = sandbox_pb2.ListSandboxesRequest()
        if state:
            req.state = self._state_to_proto(state)

        try:
            resp = self._stub.ListSandboxes(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return [self._transform_sandbox(s) for s in resp.sandboxes]
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def delete(self, sandbox_id: str, force: bool = False) -> None:
        """Delete a sandbox"""
        req = sandbox_pb2.DeleteSandboxRequest(id=sandbox_id, force=force)
        try:
            self._stub.DeleteSandbox(
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
