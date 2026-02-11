import datetime

from google.protobuf import timestamp_pb2 as _timestamp_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class FsFileType(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    FS_FILE_TYPE_UNSPECIFIED: _ClassVar[FsFileType]
    FS_FILE_TYPE_FILE: _ClassVar[FsFileType]
    FS_FILE_TYPE_DIRECTORY: _ClassVar[FsFileType]
    FS_FILE_TYPE_SYMLINK: _ClassVar[FsFileType]

class FsRenameFlags(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    FS_RENAME_FLAGS_NONE: _ClassVar[FsRenameFlags]
    FS_RENAME_FLAGS_NOREPLACE: _ClassVar[FsRenameFlags]
    FS_RENAME_FLAGS_EXCHANGE: _ClassVar[FsRenameFlags]
FS_FILE_TYPE_UNSPECIFIED: FsFileType
FS_FILE_TYPE_FILE: FsFileType
FS_FILE_TYPE_DIRECTORY: FsFileType
FS_FILE_TYPE_SYMLINK: FsFileType
FS_RENAME_FLAGS_NONE: FsRenameFlags
FS_RENAME_FLAGS_NOREPLACE: FsRenameFlags
FS_RENAME_FLAGS_EXCHANGE: FsRenameFlags

class FsFileAttr(_message.Message):
    __slots__ = ("file_type", "size", "mode", "uid", "gid", "atime", "mtime", "ctime", "nlink", "blksize", "blocks")
    FILE_TYPE_FIELD_NUMBER: _ClassVar[int]
    SIZE_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    UID_FIELD_NUMBER: _ClassVar[int]
    GID_FIELD_NUMBER: _ClassVar[int]
    ATIME_FIELD_NUMBER: _ClassVar[int]
    MTIME_FIELD_NUMBER: _ClassVar[int]
    CTIME_FIELD_NUMBER: _ClassVar[int]
    NLINK_FIELD_NUMBER: _ClassVar[int]
    BLKSIZE_FIELD_NUMBER: _ClassVar[int]
    BLOCKS_FIELD_NUMBER: _ClassVar[int]
    file_type: FsFileType
    size: int
    mode: int
    uid: int
    gid: int
    atime: _timestamp_pb2.Timestamp
    mtime: _timestamp_pb2.Timestamp
    ctime: _timestamp_pb2.Timestamp
    nlink: int
    blksize: int
    blocks: int
    def __init__(self, file_type: _Optional[_Union[FsFileType, str]] = ..., size: _Optional[int] = ..., mode: _Optional[int] = ..., uid: _Optional[int] = ..., gid: _Optional[int] = ..., atime: _Optional[_Union[datetime.datetime, _timestamp_pb2.Timestamp, _Mapping]] = ..., mtime: _Optional[_Union[datetime.datetime, _timestamp_pb2.Timestamp, _Mapping]] = ..., ctime: _Optional[_Union[datetime.datetime, _timestamp_pb2.Timestamp, _Mapping]] = ..., nlink: _Optional[int] = ..., blksize: _Optional[int] = ..., blocks: _Optional[int] = ...) -> None: ...

class FsDirEntry(_message.Message):
    __slots__ = ("name", "attr")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ATTR_FIELD_NUMBER: _ClassVar[int]
    name: str
    attr: FsFileAttr
    def __init__(self, name: _Optional[str] = ..., attr: _Optional[_Union[FsFileAttr, _Mapping]] = ...) -> None: ...

class FsStatRequest(_message.Message):
    __slots__ = ("workspace_id", "path")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ...) -> None: ...

class FsStatResponse(_message.Message):
    __slots__ = ("attr",)
    ATTR_FIELD_NUMBER: _ClassVar[int]
    attr: FsFileAttr
    def __init__(self, attr: _Optional[_Union[FsFileAttr, _Mapping]] = ...) -> None: ...

class FsReadFileRequest(_message.Message):
    __slots__ = ("workspace_id", "path", "chunk_size")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    CHUNK_SIZE_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    chunk_size: int
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ..., chunk_size: _Optional[int] = ...) -> None: ...

class FsReadFileResponse(_message.Message):
    __slots__ = ("data", "eof")
    DATA_FIELD_NUMBER: _ClassVar[int]
    EOF_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    eof: bool
    def __init__(self, data: _Optional[bytes] = ..., eof: bool = ...) -> None: ...

class FsWriteFileRequest(_message.Message):
    __slots__ = ("header", "data")
    HEADER_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    header: FsWriteFileHeader
    data: bytes
    def __init__(self, header: _Optional[_Union[FsWriteFileHeader, _Mapping]] = ..., data: _Optional[bytes] = ...) -> None: ...

class FsWriteFileHeader(_message.Message):
    __slots__ = ("workspace_id", "path", "truncate")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    TRUNCATE_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    truncate: bool
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ..., truncate: bool = ...) -> None: ...

class FsWriteFileResponse(_message.Message):
    __slots__ = ("bytes_written",)
    BYTES_WRITTEN_FIELD_NUMBER: _ClassVar[int]
    bytes_written: int
    def __init__(self, bytes_written: _Optional[int] = ...) -> None: ...

class FsListDirRequest(_message.Message):
    __slots__ = ("workspace_id", "path")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ...) -> None: ...

class FsListDirResponse(_message.Message):
    __slots__ = ("entries",)
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    entries: _containers.RepeatedCompositeFieldContainer[FsDirEntry]
    def __init__(self, entries: _Optional[_Iterable[_Union[FsDirEntry, _Mapping]]] = ...) -> None: ...

class FsMkdirRequest(_message.Message):
    __slots__ = ("workspace_id", "path", "mode")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    mode: int
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ..., mode: _Optional[int] = ...) -> None: ...

class FsMkdirResponse(_message.Message):
    __slots__ = ("attr",)
    ATTR_FIELD_NUMBER: _ClassVar[int]
    attr: FsFileAttr
    def __init__(self, attr: _Optional[_Union[FsFileAttr, _Mapping]] = ...) -> None: ...

class FsRemoveFileRequest(_message.Message):
    __slots__ = ("workspace_id", "path")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ...) -> None: ...

class FsRemoveFileResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class FsRemoveDirRequest(_message.Message):
    __slots__ = ("workspace_id", "path", "recursive")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    RECURSIVE_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    recursive: bool
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ..., recursive: bool = ...) -> None: ...

class FsRemoveDirResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class FsRenameRequest(_message.Message):
    __slots__ = ("workspace_id", "old_path", "new_path", "flags")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    OLD_PATH_FIELD_NUMBER: _ClassVar[int]
    NEW_PATH_FIELD_NUMBER: _ClassVar[int]
    FLAGS_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    old_path: str
    new_path: str
    flags: FsRenameFlags
    def __init__(self, workspace_id: _Optional[str] = ..., old_path: _Optional[str] = ..., new_path: _Optional[str] = ..., flags: _Optional[_Union[FsRenameFlags, str]] = ...) -> None: ...

class FsRenameResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class FsCreateRequest(_message.Message):
    __slots__ = ("workspace_id", "path", "mode", "exclusive")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    EXCLUSIVE_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    mode: int
    exclusive: bool
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ..., mode: _Optional[int] = ..., exclusive: bool = ...) -> None: ...

class FsCreateResponse(_message.Message):
    __slots__ = ("attr",)
    ATTR_FIELD_NUMBER: _ClassVar[int]
    attr: FsFileAttr
    def __init__(self, attr: _Optional[_Union[FsFileAttr, _Mapping]] = ...) -> None: ...

class FsSetAttrRequest(_message.Message):
    __slots__ = ("workspace_id", "path", "size", "mode", "uid", "gid", "atime", "mtime")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    SIZE_FIELD_NUMBER: _ClassVar[int]
    MODE_FIELD_NUMBER: _ClassVar[int]
    UID_FIELD_NUMBER: _ClassVar[int]
    GID_FIELD_NUMBER: _ClassVar[int]
    ATIME_FIELD_NUMBER: _ClassVar[int]
    MTIME_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    size: int
    mode: int
    uid: int
    gid: int
    atime: _timestamp_pb2.Timestamp
    mtime: _timestamp_pb2.Timestamp
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ..., size: _Optional[int] = ..., mode: _Optional[int] = ..., uid: _Optional[int] = ..., gid: _Optional[int] = ..., atime: _Optional[_Union[datetime.datetime, _timestamp_pb2.Timestamp, _Mapping]] = ..., mtime: _Optional[_Union[datetime.datetime, _timestamp_pb2.Timestamp, _Mapping]] = ...) -> None: ...

class FsSetAttrResponse(_message.Message):
    __slots__ = ("attr",)
    ATTR_FIELD_NUMBER: _ClassVar[int]
    attr: FsFileAttr
    def __init__(self, attr: _Optional[_Union[FsFileAttr, _Mapping]] = ...) -> None: ...

class FsSymlinkRequest(_message.Message):
    __slots__ = ("workspace_id", "link_path", "target")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    LINK_PATH_FIELD_NUMBER: _ClassVar[int]
    TARGET_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    link_path: str
    target: str
    def __init__(self, workspace_id: _Optional[str] = ..., link_path: _Optional[str] = ..., target: _Optional[str] = ...) -> None: ...

class FsSymlinkResponse(_message.Message):
    __slots__ = ("attr",)
    ATTR_FIELD_NUMBER: _ClassVar[int]
    attr: FsFileAttr
    def __init__(self, attr: _Optional[_Union[FsFileAttr, _Mapping]] = ...) -> None: ...

class FsReadLinkRequest(_message.Message):
    __slots__ = ("workspace_id", "path")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ...) -> None: ...

class FsReadLinkResponse(_message.Message):
    __slots__ = ("target",)
    TARGET_FIELD_NUMBER: _ClassVar[int]
    target: str
    def __init__(self, target: _Optional[str] = ...) -> None: ...

class FsReadAtRequest(_message.Message):
    __slots__ = ("workspace_id", "path", "offset", "size")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    OFFSET_FIELD_NUMBER: _ClassVar[int]
    SIZE_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    offset: int
    size: int
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ..., offset: _Optional[int] = ..., size: _Optional[int] = ...) -> None: ...

class FsReadAtResponse(_message.Message):
    __slots__ = ("data", "eof")
    DATA_FIELD_NUMBER: _ClassVar[int]
    EOF_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    eof: bool
    def __init__(self, data: _Optional[bytes] = ..., eof: bool = ...) -> None: ...

class FsWriteAtRequest(_message.Message):
    __slots__ = ("workspace_id", "path", "offset", "data")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    OFFSET_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    path: str
    offset: int
    data: bytes
    def __init__(self, workspace_id: _Optional[str] = ..., path: _Optional[str] = ..., offset: _Optional[int] = ..., data: _Optional[bytes] = ...) -> None: ...

class FsWriteAtResponse(_message.Message):
    __slots__ = ("bytes_written",)
    BYTES_WRITTEN_FIELD_NUMBER: _ClassVar[int]
    bytes_written: int
    def __init__(self, bytes_written: _Optional[int] = ...) -> None: ...

class FsStatFsRequest(_message.Message):
    __slots__ = ("workspace_id",)
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    workspace_id: str
    def __init__(self, workspace_id: _Optional[str] = ...) -> None: ...

class FsStatFsResponse(_message.Message):
    __slots__ = ("blocks", "bfree", "bavail", "files", "ffree", "bsize", "namelen", "frsize")
    BLOCKS_FIELD_NUMBER: _ClassVar[int]
    BFREE_FIELD_NUMBER: _ClassVar[int]
    BAVAIL_FIELD_NUMBER: _ClassVar[int]
    FILES_FIELD_NUMBER: _ClassVar[int]
    FFREE_FIELD_NUMBER: _ClassVar[int]
    BSIZE_FIELD_NUMBER: _ClassVar[int]
    NAMELEN_FIELD_NUMBER: _ClassVar[int]
    FRSIZE_FIELD_NUMBER: _ClassVar[int]
    blocks: int
    bfree: int
    bavail: int
    files: int
    ffree: int
    bsize: int
    namelen: int
    frsize: int
    def __init__(self, blocks: _Optional[int] = ..., bfree: _Optional[int] = ..., bavail: _Optional[int] = ..., files: _Optional[int] = ..., ffree: _Optional[int] = ..., bsize: _Optional[int] = ..., namelen: _Optional[int] = ..., frsize: _Optional[int] = ...) -> None: ...

class DownloadBinaryRequest(_message.Message):
    __slots__ = ("name", "platform", "arch")
    NAME_FIELD_NUMBER: _ClassVar[int]
    PLATFORM_FIELD_NUMBER: _ClassVar[int]
    ARCH_FIELD_NUMBER: _ClassVar[int]
    name: str
    platform: str
    arch: str
    def __init__(self, name: _Optional[str] = ..., platform: _Optional[str] = ..., arch: _Optional[str] = ...) -> None: ...

class DownloadBinaryResponse(_message.Message):
    __slots__ = ("chunk",)
    CHUNK_FIELD_NUMBER: _ClassVar[int]
    chunk: bytes
    def __init__(self, chunk: _Optional[bytes] = ...) -> None: ...
