"""
Process service for executing commands via gRPC
"""

from typing import Optional, Iterator, List

import grpc

from workspace_sdk.types import (
    CommandResult,
    RunCommandOptions,
    ProcessEvent,
    StdoutEvent,
    StderrEvent,
    ExitEvent,
    ErrorEvent,
)
from workspace_sdk.errors import convert_grpc_error
from workspace_sdk.proto.workspace.v1 import process_pb2, process_pb2_grpc


def _create_metadata(api_key: Optional[str]) -> List[tuple]:
    """Create gRPC metadata with auth token"""
    if api_key:
        return [("authorization", f"Bearer {api_key}")]
    return []


class ProcessService:
    """Sync service for executing commands in sandboxes via gRPC"""

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

    def run(
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
            resp = self._stub.RunCommand(
                req, metadata=self._metadata(), timeout=self._timeout
            )
            return CommandResult(
                exit_code=resp.result.exit_code,
                stdout=resp.result.stdout,
                stderr=resp.result.stderr,
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def run_stream(
        self,
        sandbox_id: str,
        command: str,
        options: Optional[RunCommandOptions] = None,
    ) -> Iterator[ProcessEvent]:
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
            for event in stream:
                parsed = self._parse_event(event)
                if parsed:
                    yield parsed
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def kill(
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
            self._stub.KillProcess(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def shell(
        self,
        sandbox_id: str,
        script: str,
        env: Optional[dict] = None,
    ) -> CommandResult:
        """Run a shell script using bash -c"""
        return self.run(
            sandbox_id,
            "bash",
            RunCommandOptions(args=["-c", script], env=env or {}),
        )

    def exec(
        self,
        sandbox_id: str,
        command: str,
        *args: str,
    ) -> str:
        """Execute a command and return stdout, raising on non-zero exit"""
        result = self.run(sandbox_id, command, RunCommandOptions(args=list(args)))
        if result.exit_code != 0:
            raise RuntimeError(
                f"Command failed with exit code {result.exit_code}: {result.stderr}"
            )
        return result.stdout

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
