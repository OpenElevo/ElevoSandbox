from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class CreatePtyRequest(_message.Message):
    __slots__ = ("sandbox_id", "cols", "rows", "shell", "env")
    class EnvEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    SANDBOX_ID_FIELD_NUMBER: _ClassVar[int]
    COLS_FIELD_NUMBER: _ClassVar[int]
    ROWS_FIELD_NUMBER: _ClassVar[int]
    SHELL_FIELD_NUMBER: _ClassVar[int]
    ENV_FIELD_NUMBER: _ClassVar[int]
    sandbox_id: str
    cols: int
    rows: int
    shell: str
    env: _containers.ScalarMap[str, str]
    def __init__(self, sandbox_id: _Optional[str] = ..., cols: _Optional[int] = ..., rows: _Optional[int] = ..., shell: _Optional[str] = ..., env: _Optional[_Mapping[str, str]] = ...) -> None: ...

class CreatePtyResponse(_message.Message):
    __slots__ = ("pty",)
    PTY_FIELD_NUMBER: _ClassVar[int]
    pty: PtyInfo
    def __init__(self, pty: _Optional[_Union[PtyInfo, _Mapping]] = ...) -> None: ...

class PtyInfo(_message.Message):
    __slots__ = ("id", "sandbox_id", "cols", "rows")
    ID_FIELD_NUMBER: _ClassVar[int]
    SANDBOX_ID_FIELD_NUMBER: _ClassVar[int]
    COLS_FIELD_NUMBER: _ClassVar[int]
    ROWS_FIELD_NUMBER: _ClassVar[int]
    id: str
    sandbox_id: str
    cols: int
    rows: int
    def __init__(self, id: _Optional[str] = ..., sandbox_id: _Optional[str] = ..., cols: _Optional[int] = ..., rows: _Optional[int] = ...) -> None: ...

class ResizePtyRequest(_message.Message):
    __slots__ = ("sandbox_id", "pty_id", "cols", "rows")
    SANDBOX_ID_FIELD_NUMBER: _ClassVar[int]
    PTY_ID_FIELD_NUMBER: _ClassVar[int]
    COLS_FIELD_NUMBER: _ClassVar[int]
    ROWS_FIELD_NUMBER: _ClassVar[int]
    sandbox_id: str
    pty_id: str
    cols: int
    rows: int
    def __init__(self, sandbox_id: _Optional[str] = ..., pty_id: _Optional[str] = ..., cols: _Optional[int] = ..., rows: _Optional[int] = ...) -> None: ...

class ResizePtyResponse(_message.Message):
    __slots__ = ("success",)
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    success: bool
    def __init__(self, success: bool = ...) -> None: ...

class KillPtyRequest(_message.Message):
    __slots__ = ("sandbox_id", "pty_id")
    SANDBOX_ID_FIELD_NUMBER: _ClassVar[int]
    PTY_ID_FIELD_NUMBER: _ClassVar[int]
    sandbox_id: str
    pty_id: str
    def __init__(self, sandbox_id: _Optional[str] = ..., pty_id: _Optional[str] = ...) -> None: ...

class KillPtyResponse(_message.Message):
    __slots__ = ("success",)
    SUCCESS_FIELD_NUMBER: _ClassVar[int]
    success: bool
    def __init__(self, success: bool = ...) -> None: ...

class PtyStreamRequest(_message.Message):
    __slots__ = ("init", "input", "resize")
    INIT_FIELD_NUMBER: _ClassVar[int]
    INPUT_FIELD_NUMBER: _ClassVar[int]
    RESIZE_FIELD_NUMBER: _ClassVar[int]
    init: PtyStreamInit
    input: bytes
    resize: PtyResizeEvent
    def __init__(self, init: _Optional[_Union[PtyStreamInit, _Mapping]] = ..., input: _Optional[bytes] = ..., resize: _Optional[_Union[PtyResizeEvent, _Mapping]] = ...) -> None: ...

class PtyStreamInit(_message.Message):
    __slots__ = ("sandbox_id", "pty_id")
    SANDBOX_ID_FIELD_NUMBER: _ClassVar[int]
    PTY_ID_FIELD_NUMBER: _ClassVar[int]
    sandbox_id: str
    pty_id: str
    def __init__(self, sandbox_id: _Optional[str] = ..., pty_id: _Optional[str] = ...) -> None: ...

class PtyResizeEvent(_message.Message):
    __slots__ = ("cols", "rows")
    COLS_FIELD_NUMBER: _ClassVar[int]
    ROWS_FIELD_NUMBER: _ClassVar[int]
    cols: int
    rows: int
    def __init__(self, cols: _Optional[int] = ..., rows: _Optional[int] = ...) -> None: ...

class PtyStreamResponse(_message.Message):
    __slots__ = ("output", "exit_code", "error")
    OUTPUT_FIELD_NUMBER: _ClassVar[int]
    EXIT_CODE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    output: bytes
    exit_code: int
    error: str
    def __init__(self, output: _Optional[bytes] = ..., exit_code: _Optional[int] = ..., error: _Optional[str] = ...) -> None: ...
