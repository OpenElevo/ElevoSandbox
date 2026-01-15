"""
PTY service for interactive terminals
"""

from typing import Optional, Callable
import asyncio
import httpx
import websockets

from workspace_sdk.types import PtyOptions, PtyHandle


class AsyncPtyService:
    """Async service for managing interactive terminals"""

    def __init__(self, client: httpx.AsyncClient, api_url: str):
        self._client = client
        self._api_url = api_url

    async def create(
        self,
        sandbox_id: str,
        options: Optional[PtyOptions] = None,
    ) -> PtyHandle:
        """Create a new PTY"""
        opts = options or PtyOptions()
        data = {
            "cols": opts.cols,
            "rows": opts.rows,
        }
        if opts.shell:
            data["shell"] = opts.shell
        if opts.env:
            data["env"] = opts.env

        response = await self._client.post(
            f"/sandboxes/{sandbox_id}/pty",
            json=data,
        )
        response.raise_for_status()
        result = response.json()

        pty_id = result["id"]
        cols = result["cols"]
        rows = result["rows"]

        # Create WebSocket connection
        ws_url = self._api_url.replace("http", "ws")
        ws_uri = f"{ws_url}/api/v1/sandboxes/{sandbox_id}/pty/{pty_id}"

        ws = await websockets.connect(ws_uri)

        data_callback: Optional[Callable[[bytes], None]] = None
        close_callback: Optional[Callable[[], None]] = None

        async def receive_loop() -> None:
            nonlocal data_callback, close_callback
            try:
                async for message in ws:
                    if isinstance(message, bytes) and data_callback:
                        data_callback(message)
            except websockets.ConnectionClosed:
                pass
            finally:
                if close_callback:
                    close_callback()

        # Start receive loop in background
        asyncio.create_task(receive_loop())

        async def write(data: bytes) -> None:
            await ws.send(data)

        async def resize(new_cols: int, new_rows: int) -> None:
            await self._client.post(
                f"/sandboxes/{sandbox_id}/pty/{pty_id}/resize",
                json={"cols": new_cols, "rows": new_rows},
            )

        async def kill() -> None:
            await ws.close()
            await self._client.delete(f"/sandboxes/{sandbox_id}/pty/{pty_id}")

        def on_data(callback: Callable[[bytes], None]) -> None:
            nonlocal data_callback
            data_callback = callback

        def on_close(callback: Callable[[], None]) -> None:
            nonlocal close_callback
            close_callback = callback

        return PtyHandle(
            id=pty_id,
            cols=cols,
            rows=rows,
            _write=write,
            _resize=resize,
            _kill=kill,
            _on_data=on_data,
            _on_close=on_close,
        )

    async def resize(
        self,
        sandbox_id: str,
        pty_id: str,
        cols: int,
        rows: int,
    ) -> None:
        """Resize a PTY"""
        response = await self._client.post(
            f"/sandboxes/{sandbox_id}/pty/{pty_id}/resize",
            json={"cols": cols, "rows": rows},
        )
        response.raise_for_status()

    async def kill(self, sandbox_id: str, pty_id: str) -> None:
        """Kill a PTY"""
        response = await self._client.delete(
            f"/sandboxes/{sandbox_id}/pty/{pty_id}"
        )
        response.raise_for_status()


class PtyService:
    """Sync service for managing interactive terminals

    Note: PTY operations are inherently async due to WebSocket.
    This service provides sync wrappers that run async code.
    """

    def __init__(self, client: httpx.Client, api_url: str):
        self._client = client
        self._api_url = api_url

    def resize(
        self,
        sandbox_id: str,
        pty_id: str,
        cols: int,
        rows: int,
    ) -> None:
        """Resize a PTY"""
        response = self._client.post(
            f"/sandboxes/{sandbox_id}/pty/{pty_id}/resize",
            json={"cols": cols, "rows": rows},
        )
        response.raise_for_status()

    def kill(self, sandbox_id: str, pty_id: str) -> None:
        """Kill a PTY"""
        response = self._client.delete(
            f"/sandboxes/{sandbox_id}/pty/{pty_id}"
        )
        response.raise_for_status()
