"""
Process service for executing commands
"""

from typing import Optional, AsyncIterator, Iterator
import json
import httpx

from workspace_sdk.types import (
    CommandResult,
    RunCommandOptions,
    ProcessEvent,
    StdoutEvent,
    StderrEvent,
    ExitEvent,
    ErrorEvent,
)


class AsyncProcessService:
    """Async service for executing commands in sandboxes"""

    def __init__(self, client: httpx.AsyncClient, api_url: str):
        self._client = client
        self._api_url = api_url

    async def run(
        self,
        sandbox_id: str,
        command: str,
        options: Optional[RunCommandOptions] = None,
    ) -> CommandResult:
        """Run a command and wait for completion"""
        data = {
            "command": command,
            "args": options.args if options and options.args else [],
            "env": options.env if options and options.env else {},
        }
        if options and options.cwd:
            data["cwd"] = options.cwd
        if options and options.timeout:
            data["timeout"] = options.timeout

        response = await self._client.post(
            f"/sandboxes/{sandbox_id}/process/run",
            json=data,
        )
        response.raise_for_status()
        result = response.json()

        return CommandResult(
            exit_code=result["exit_code"],
            stdout=result["stdout"],
            stderr=result["stderr"],
        )

    async def run_stream(
        self,
        sandbox_id: str,
        command: str,
        options: Optional[RunCommandOptions] = None,
    ) -> AsyncIterator[ProcessEvent]:
        """Run a command with streaming output"""
        params = {
            "command": command,
            "args": json.dumps(options.args if options and options.args else []),
            "env": json.dumps(options.env if options and options.env else {}),
        }
        if options and options.cwd:
            params["cwd"] = options.cwd
        if options and options.timeout:
            params["timeout"] = str(options.timeout)

        url = f"{self._api_url}/api/v1/sandboxes/{sandbox_id}/process/run/stream"

        async with httpx.AsyncClient() as stream_client:
            async with stream_client.stream("GET", url, params=params) as response:
                response.raise_for_status()
                async for line in response.aiter_lines():
                    if line.startswith("data: "):
                        data = json.loads(line[6:])
                        event = self._parse_event(data)
                        if event:
                            yield event

    async def kill(
        self,
        sandbox_id: str,
        pid: int,
        signal: int = 15,
    ) -> None:
        """Kill a running process"""
        response = await self._client.post(
            f"/sandboxes/{sandbox_id}/process/{pid}/kill",
            json={"signal": signal},
        )
        response.raise_for_status()

    def _parse_event(self, data: dict) -> Optional[ProcessEvent]:
        """Parse event data into ProcessEvent"""
        event_type = data.get("type")
        if event_type == "stdout":
            return StdoutEvent(type="stdout", data=data["data"])
        elif event_type == "stderr":
            return StderrEvent(type="stderr", data=data["data"])
        elif event_type == "exit":
            return ExitEvent(type="exit", code=data["code"])
        elif event_type == "error":
            return ErrorEvent(type="error", message=data["message"])
        return None


class ProcessService:
    """Sync service for executing commands in sandboxes"""

    def __init__(self, client: httpx.Client, api_url: str):
        self._client = client
        self._api_url = api_url

    def run(
        self,
        sandbox_id: str,
        command: str,
        options: Optional[RunCommandOptions] = None,
    ) -> CommandResult:
        """Run a command and wait for completion"""
        data = {
            "command": command,
            "args": options.args if options and options.args else [],
            "env": options.env if options and options.env else {},
        }
        if options and options.cwd:
            data["cwd"] = options.cwd
        if options and options.timeout:
            data["timeout"] = options.timeout

        response = self._client.post(
            f"/sandboxes/{sandbox_id}/process/run",
            json=data,
        )
        response.raise_for_status()
        result = response.json()

        return CommandResult(
            exit_code=result["exit_code"],
            stdout=result["stdout"],
            stderr=result["stderr"],
        )

    def run_stream(
        self,
        sandbox_id: str,
        command: str,
        options: Optional[RunCommandOptions] = None,
    ) -> Iterator[ProcessEvent]:
        """Run a command with streaming output"""
        params = {
            "command": command,
            "args": json.dumps(options.args if options and options.args else []),
            "env": json.dumps(options.env if options and options.env else {}),
        }
        if options and options.cwd:
            params["cwd"] = options.cwd
        if options and options.timeout:
            params["timeout"] = str(options.timeout)

        url = f"{self._api_url}/api/v1/sandboxes/{sandbox_id}/process/run/stream"

        with httpx.Client() as stream_client:
            with stream_client.stream("GET", url, params=params) as response:
                response.raise_for_status()
                for line in response.iter_lines():
                    if line.startswith("data: "):
                        data = json.loads(line[6:])
                        event = self._parse_event(data)
                        if event:
                            yield event

    def kill(
        self,
        sandbox_id: str,
        pid: int,
        signal: int = 15,
    ) -> None:
        """Kill a running process"""
        response = self._client.post(
            f"/sandboxes/{sandbox_id}/process/{pid}/kill",
            json={"signal": signal},
        )
        response.raise_for_status()

    def _parse_event(self, data: dict) -> Optional[ProcessEvent]:
        """Parse event data into ProcessEvent"""
        event_type = data.get("type")
        if event_type == "stdout":
            return StdoutEvent(type="stdout", data=data["data"])
        elif event_type == "stderr":
            return StderrEvent(type="stderr", data=data["data"])
        elif event_type == "exit":
            return ExitEvent(type="exit", code=data["code"])
        elif event_type == "error":
            return ErrorEvent(type="error", message=data["message"])
        return None
