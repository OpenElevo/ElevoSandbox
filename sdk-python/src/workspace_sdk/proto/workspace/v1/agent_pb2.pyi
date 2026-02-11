from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class AgentMessage(_message.Message):
    __slots__ = ("handshake", "heartbeat", "command_response", "pty_output")
    HANDSHAKE_FIELD_NUMBER: _ClassVar[int]
    HEARTBEAT_FIELD_NUMBER: _ClassVar[int]
    COMMAND_RESPONSE_FIELD_NUMBER: _ClassVar[int]
    PTY_OUTPUT_FIELD_NUMBER: _ClassVar[int]
    handshake: AgentHandshake
    heartbeat: AgentHeartbeat
    command_response: AgentCommandResponse
    pty_output: AgentPtyOutput
    def __init__(self, handshake: _Optional[_Union[AgentHandshake, _Mapping]] = ..., heartbeat: _Optional[_Union[AgentHeartbeat, _Mapping]] = ..., command_response: _Optional[_Union[AgentCommandResponse, _Mapping]] = ..., pty_output: _Optional[_Union[AgentPtyOutput, _Mapping]] = ...) -> None: ...

class AgentHandshake(_message.Message):
    __slots__ = ("sandbox_id", "version")
    SANDBOX_ID_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    sandbox_id: str
    version: str
    def __init__(self, sandbox_id: _Optional[str] = ..., version: _Optional[str] = ...) -> None: ...

class AgentHeartbeat(_message.Message):
    __slots__ = ("timestamp",)
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    timestamp: int
    def __init__(self, timestamp: _Optional[int] = ...) -> None: ...

class AgentCommandResponse(_message.Message):
    __slots__ = ("correlation_id", "success", "error")
    CORRELATION_ID_FIELD_NUMBER: _ClassVar[int]
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    correlation_id: str
    success: AgentCommandSuccess
    error: AgentCommandError
    def __init__(self, correlation_id: _Optional[str] = ..., success: _Optional[_Union[AgentCommandSuccess, _Mapping]] = ..., error: _Optional[_Union[AgentCommandError, _Mapping]] = ...) -> None: ...

class AgentCommandSuccess(_message.Message):
    __slots__ = ("exit_code", "stdout", "stderr")
    EXIT_CODE_FIELD_NUMBER: _ClassVar[int]
    STDOUT_FIELD_NUMBER: _ClassVar[int]
    STDERR_FIELD_NUMBER: _ClassVar[int]
    exit_code: int
    stdout: str
    stderr: str
    def __init__(self, exit_code: _Optional[int] = ..., stdout: _Optional[str] = ..., stderr: _Optional[str] = ...) -> None: ...

class AgentCommandError(_message.Message):
    __slots__ = ("code", "message")
    CODE_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    code: int
    message: str
    def __init__(self, code: _Optional[int] = ..., message: _Optional[str] = ...) -> None: ...

class AgentPtyOutput(_message.Message):
    __slots__ = ("pty_id", "data")
    PTY_ID_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    pty_id: str
    data: bytes
    def __init__(self, pty_id: _Optional[str] = ..., data: _Optional[bytes] = ...) -> None: ...

class ServerMessage(_message.Message):
    __slots__ = ("handshake_ack", "heartbeat_ack", "run_command", "kill_process", "create_pty", "resize_pty", "kill_pty", "pty_input")
    HANDSHAKE_ACK_FIELD_NUMBER: _ClassVar[int]
    HEARTBEAT_ACK_FIELD_NUMBER: _ClassVar[int]
    RUN_COMMAND_FIELD_NUMBER: _ClassVar[int]
    KILL_PROCESS_FIELD_NUMBER: _ClassVar[int]
    CREATE_PTY_FIELD_NUMBER: _ClassVar[int]
    RESIZE_PTY_FIELD_NUMBER: _ClassVar[int]
    KILL_PTY_FIELD_NUMBER: _ClassVar[int]
    PTY_INPUT_FIELD_NUMBER: _ClassVar[int]
    handshake_ack: ServerHandshakeAck
    heartbeat_ack: ServerHeartbeatAck
    run_command: AgentRunCommandRequest
    kill_process: AgentKillProcessRequest
    create_pty: AgentCreatePtyRequest
    resize_pty: AgentResizePtyRequest
    kill_pty: AgentKillPtyRequest
    pty_input: AgentPtyInput
    def __init__(self, handshake_ack: _Optional[_Union[ServerHandshakeAck, _Mapping]] = ..., heartbeat_ack: _Optional[_Union[ServerHeartbeatAck, _Mapping]] = ..., run_command: _Optional[_Union[AgentRunCommandRequest, _Mapping]] = ..., kill_process: _Optional[_Union[AgentKillProcessRequest, _Mapping]] = ..., create_pty: _Optional[_Union[AgentCreatePtyRequest, _Mapping]] = ..., resize_pty: _Optional[_Union[AgentResizePtyRequest, _Mapping]] = ..., kill_pty: _Optional[_Union[AgentKillPtyRequest, _Mapping]] = ..., pty_input: _Optional[_Union[AgentPtyInput, _Mapping]] = ...) -> None: ...

class ServerHandshakeAck(_message.Message):
    __slots__ = ("success", "error")
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    success: bool
    error: str
    def __init__(self, success: bool = ..., error: _Optional[str] = ...) -> None: ...

class ServerHeartbeatAck(_message.Message):
    __slots__ = ("timestamp",)
    TIMESTAMP_FIELD_NUMBER: _ClassVar[int]
    timestamp: int
    def __init__(self, timestamp: _Optional[int] = ...) -> None: ...

class AgentRunCommandRequest(_message.Message):
    __slots__ = ("correlation_id", "command", "args", "env", "cwd", "timeout_ms", "stream")
    class EnvEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    CORRELATION_ID_FIELD_NUMBER: _ClassVar[int]
    COMMAND_FIELD_NUMBER: _ClassVar[int]
    ARGS_FIELD_NUMBER: _ClassVar[int]
    ENV_FIELD_NUMBER: _ClassVar[int]
    CWD_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    STREAM_FIELD_NUMBER: _ClassVar[int]
    correlation_id: str
    command: str
    args: _containers.RepeatedScalarFieldContainer[str]
    env: _containers.ScalarMap[str, str]
    cwd: str
    timeout_ms: int
    stream: bool
    def __init__(self, correlation_id: _Optional[str] = ..., command: _Optional[str] = ..., args: _Optional[_Iterable[str]] = ..., env: _Optional[_Mapping[str, str]] = ..., cwd: _Optional[str] = ..., timeout_ms: _Optional[int] = ..., stream: bool = ...) -> None: ...

class AgentKillProcessRequest(_message.Message):
    __slots__ = ("correlation_id", "pid", "signal")
    CORRELATION_ID_FIELD_NUMBER: _ClassVar[int]
    PID_FIELD_NUMBER: _ClassVar[int]
    SIGNAL_FIELD_NUMBER: _ClassVar[int]
    correlation_id: str
    pid: int
    signal: int
    def __init__(self, correlation_id: _Optional[str] = ..., pid: _Optional[int] = ..., signal: _Optional[int] = ...) -> None: ...

class AgentCreatePtyRequest(_message.Message):
    __slots__ = ("correlation_id", "pty_id", "cols", "rows", "shell", "env")
    class EnvEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    CORRELATION_ID_FIELD_NUMBER: _ClassVar[int]
    PTY_ID_FIELD_NUMBER: _ClassVar[int]
    COLS_FIELD_NUMBER: _ClassVar[int]
    ROWS_FIELD_NUMBER: _ClassVar[int]
    SHELL_FIELD_NUMBER: _ClassVar[int]
    ENV_FIELD_NUMBER: _ClassVar[int]
    correlation_id: str
    pty_id: str
    cols: int
    rows: int
    shell: str
    env: _containers.ScalarMap[str, str]
    def __init__(self, correlation_id: _Optional[str] = ..., pty_id: _Optional[str] = ..., cols: _Optional[int] = ..., rows: _Optional[int] = ..., shell: _Optional[str] = ..., env: _Optional[_Mapping[str, str]] = ...) -> None: ...

class AgentResizePtyRequest(_message.Message):
    __slots__ = ("correlation_id", "pty_id", "cols", "rows")
    CORRELATION_ID_FIELD_NUMBER: _ClassVar[int]
    PTY_ID_FIELD_NUMBER: _ClassVar[int]
    COLS_FIELD_NUMBER: _ClassVar[int]
    ROWS_FIELD_NUMBER: _ClassVar[int]
    correlation_id: str
    pty_id: str
    cols: int
    rows: int
    def __init__(self, correlation_id: _Optional[str] = ..., pty_id: _Optional[str] = ..., cols: _Optional[int] = ..., rows: _Optional[int] = ...) -> None: ...

class AgentKillPtyRequest(_message.Message):
    __slots__ = ("correlation_id", "pty_id")
    CORRELATION_ID_FIELD_NUMBER: _ClassVar[int]
    PTY_ID_FIELD_NUMBER: _ClassVar[int]
    correlation_id: str
    pty_id: str
    def __init__(self, correlation_id: _Optional[str] = ..., pty_id: _Optional[str] = ...) -> None: ...

class AgentPtyInput(_message.Message):
    __slots__ = ("pty_id", "data")
    PTY_ID_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    pty_id: str
    data: bytes
    def __init__(self, pty_id: _Optional[str] = ..., data: _Optional[bytes] = ...) -> None: ...
