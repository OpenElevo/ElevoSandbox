"""
PTY service for interactive terminals via gRPC
"""

from typing import Optional, Callable, List
import threading
import queue

import grpc

from workspace_sdk.types import PtyOptions, PtyHandle
from workspace_sdk.errors import convert_grpc_error
from workspace_sdk.proto.workspace.v1 import pty_pb2, pty_pb2_grpc


def _create_metadata(api_key: Optional[str]) -> List[tuple]:
    """Create gRPC metadata with auth token"""
    if api_key:
        return [("authorization", f"Bearer {api_key}")]
    return []


class PtySession:
    """Represents an active PTY session with gRPC bidirectional stream"""

    def __init__(
        self,
        handle: PtyHandle,
        stream,
        sandbox_id: str,
    ):
        self.handle = handle
        self._stream = stream
        self._sandbox_id = sandbox_id
        self._closed = False
        self._data_callback: Optional[Callable[[bytes], None]] = None
        self._close_callback: Optional[Callable[[], None]] = None
        self._error_callback: Optional[Callable[[Exception], None]] = None
        self._outgoing: queue.Queue = queue.Queue()
        self._read_thread: Optional[threading.Thread] = None
        self._write_thread: Optional[threading.Thread] = None

    def start(self) -> None:
        """Start the read/write loops"""
        self._read_thread = threading.Thread(target=self._read_loop, daemon=True)
        self._write_thread = threading.Thread(target=self._write_loop, daemon=True)
        self._read_thread.start()
        self._write_thread.start()

    def write(self, data: bytes) -> None:
        """Send data to the PTY"""
        if self._closed:
            raise RuntimeError("Session is closed")
        self._outgoing.put(("input", data))

    def resize(self, cols: int, rows: int) -> None:
        """Resize the PTY"""
        if self._closed:
            raise RuntimeError("Session is closed")
        self._outgoing.put(("resize", (cols, rows)))

    def close(self) -> None:
        """Close the PTY session"""
        if self._closed:
            return
        self._closed = True
        self._outgoing.put(("close", None))

    def on_data(self, callback: Callable[[bytes], None]) -> None:
        """Set callback for incoming data"""
        self._data_callback = callback

    def on_close(self, callback: Callable[[], None]) -> None:
        """Set callback for session close"""
        self._close_callback = callback

    def on_error(self, callback: Callable[[Exception], None]) -> None:
        """Set callback for errors"""
        self._error_callback = callback

    def _read_loop(self) -> None:
        """Read messages from gRPC stream"""
        try:
            for resp in self._stream:
                which = resp.WhichOneof("response")
                if which == "output":
                    if self._data_callback:
                        self._data_callback(resp.output)
                elif which == "exit_code":
                    break
                elif which == "error":
                    if self._error_callback:
                        self._error_callback(RuntimeError(resp.error))
                    break
        except grpc.RpcError as e:
            if not self._closed and self._error_callback:
                self._error_callback(convert_grpc_error(e))
        finally:
            self._closed = True
            if self._close_callback:
                self._close_callback()

    def _write_loop(self) -> None:
        """Write messages to gRPC stream"""
        try:
            while not self._closed:
                try:
                    msg_type, data = self._outgoing.get(timeout=0.1)
                except queue.Empty:
                    continue

                if msg_type == "close":
                    break
                elif msg_type == "input":
                    req = pty_pb2.PtyStreamRequest(input=data)
                    self._stream.write(req)
                elif msg_type == "resize":
                    cols, rows = data
                    req = pty_pb2.PtyStreamRequest(
                        resize=pty_pb2.PtyResizeEvent(cols=cols, rows=rows)
                    )
                    self._stream.write(req)
        except grpc.RpcError as e:
            if not self._closed and self._error_callback:
                self._error_callback(convert_grpc_error(e))
        finally:
            try:
                self._stream.done_writing()
            except Exception:
                pass


class PtyService:
    """Sync service for managing interactive terminals via gRPC"""

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

    def create(
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
            resp = self._stub.CreatePty(
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

    def connect(
        self,
        sandbox_id: str,
        options: Optional[PtyOptions] = None,
    ) -> PtySession:
        """Create a PTY and establish a bidirectional stream"""
        handle = self.create(sandbox_id, options)

        try:
            stream = self._stub.PtyStream(metadata=self._metadata())

            # Send init message
            init_req = pty_pb2.PtyStreamRequest(
                init=pty_pb2.PtyStreamInit(
                    sandbox_id=sandbox_id,
                    pty_id=handle.id,
                )
            )
            stream.write(init_req)

            session = PtySession(handle, stream, sandbox_id)
            session.start()
            return session
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def resize(
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
            self._stub.ResizePty(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)

    def kill(self, sandbox_id: str, pty_id: str) -> None:
        """Kill a PTY"""
        req = pty_pb2.KillPtyRequest(
            sandbox_id=sandbox_id,
            pty_id=pty_id,
        )
        try:
            self._stub.KillPty(
                req, metadata=self._metadata(), timeout=self._timeout
            )
        except grpc.RpcError as e:
            raise convert_grpc_error(e)
