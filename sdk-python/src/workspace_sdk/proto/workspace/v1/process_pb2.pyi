from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class RunCommandRequest(_message.Message):
    __slots__ = ("sandbox_id", "command", "args", "env", "cwd", "timeout_ms")
    class EnvEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    SANDBOX_ID_FIELD_NUMBER: _ClassVar[int]
    COMMAND_FIELD_NUMBER: _ClassVar[int]
    ARGS_FIELD_NUMBER: _ClassVar[int]
    ENV_FIELD_NUMBER: _ClassVar[int]
    CWD_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    sandbox_id: str
    command: str
    args: _containers.RepeatedScalarFieldContainer[str]
    env: _containers.ScalarMap[str, str]
    cwd: str
    timeout_ms: int
    def __init__(self, sandbox_id: _Optional[str] = ..., command: _Optional[str] = ..., args: _Optional[_Iterable[str]] = ..., env: _Optional[_Mapping[str, str]] = ..., cwd: _Optional[str] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class RunCommandResponse(_message.Message):
    __slots__ = ("result",)
    RESULT_FIELD_NUMBER: _ClassVar[int]
    result: CommandResult
    def __init__(self, result: _Optional[_Union[CommandResult, _Mapping]] = ...) -> None: ...

class CommandResult(_message.Message):
    __slots__ = ("exit_code", "stdout", "stderr")
    EXIT_CODE_FIELD_NUMBER: _ClassVar[int]
    STDOUT_FIELD_NUMBER: _ClassVar[int]
    STDERR_FIELD_NUMBER: _ClassVar[int]
    exit_code: int
    stdout: str
    stderr: str
    def __init__(self, exit_code: _Optional[int] = ..., stdout: _Optional[str] = ..., stderr: _Optional[str] = ...) -> None: ...

class ProcessEvent(_message.Message):
    __slots__ = ("stdout", "stderr", "exit", "error")
    STDOUT_FIELD_NUMBER: _ClassVar[int]
    STDERR_FIELD_NUMBER: _ClassVar[int]
    EXIT_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    stdout: StdoutEvent
    stderr: StderrEvent
    exit: ExitEvent
    error: ErrorEvent
    def __init__(self, stdout: _Optional[_Union[StdoutEvent, _Mapping]] = ..., stderr: _Optional[_Union[StderrEvent, _Mapping]] = ..., exit: _Optional[_Union[ExitEvent, _Mapping]] = ..., error: _Optional[_Union[ErrorEvent, _Mapping]] = ...) -> None: ...

class StdoutEvent(_message.Message):
    __slots__ = ("data",)
    DATA_FIELD_NUMBER: _ClassVar[int]
    data: str
    def __init__(self, data: _Optional[str] = ...) -> None: ...

class StderrEvent(_message.Message):
    __slots__ = ("data",)
    DATA_FIELD_NUMBER: _ClassVar[int]
    data: str
    def __init__(self, data: _Optional[str] = ...) -> None: ...

class ExitEvent(_message.Message):
    __slots__ = ("code",)
    CODE_FIELD_NUMBER: _ClassVar[int]
    code: int
    def __init__(self, code: _Optional[int] = ...) -> None: ...

class ErrorEvent(_message.Message):
    __slots__ = ("message",)
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    message: str
    def __init__(self, message: _Optional[str] = ...) -> None: ...

class KillProcessRequest(_message.Message):
    __slots__ = ("sandbox_id", "pid", "signal")
    SANDBOX_ID_FIELD_NUMBER: _ClassVar[int]
    PID_FIELD_NUMBER: _ClassVar[int]
    SIGNAL_FIELD_NUMBER: _ClassVar[int]
    sandbox_id: str
    pid: int
    signal: int
    def __init__(self, sandbox_id: _Optional[str] = ..., pid: _Optional[int] = ..., signal: _Optional[int] = ...) -> None: ...

class KillProcessResponse(_message.Message):
    __slots__ = ("success",)
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    success: bool
    def __init__(self, success: bool = ...) -> None: ...
