# FileSystem 服务接口文档

FileSystem 服务提供 Sandbox 内的文件系统操作能力，包括文件读写、目录管理、权限控制、文件搜索和监视等功能。

---

## 目录

- [1. 概述](#1-概述)
- [2. 数据类型](#2-数据类型)
- [3. FileSystemService 接口](#3-filesystemservice-接口)
- [4. REST API](#4-rest-api)
- [5. 使用示例](#5-使用示例)
- [6. 错误处理](#6-错误处理)

---

## 1. 概述

### 1.1 功能说明

FileSystem 服务提供以下核心功能：

| 功能类别 | 功能 | 描述 |
|---------|------|------|
| **读取操作** | read | 读取文件内容（支持文本、二进制、流） |
| **写入操作** | write, writeFiles | 写入单个或多个文件 |
| **目录操作** | mkdir, list | 创建目录、列出目录内容 |
| **文件操作** | copy, move, remove | 复制、移动、删除文件/目录 |
| **信息查询** | exists, stat | 检查存在性、获取文件信息 |
| **搜索功能** | find, grep | 按名称/内容搜索文件 |
| **监视功能** | watch | 监视文件/目录变化 |
| **权限管理** | chmod, chown | 修改权限和所有者 |
| **传输功能** | upload, download | 大文件上传下载 |

### 1.2 路径规范

- 所有路径必须是**绝对路径**，以 `/` 开头
- 路径分隔符统一使用 `/`
- 支持的最大路径长度：4096 字符
- 支持的最大文件名长度：255 字符
- 禁止使用的字符：`\0`（空字符）

```typescript
// 有效路径示例
'/home/user/file.txt'
'/app/src/index.ts'
'/var/log/app.log'

// 无效路径示例
'relative/path.txt'    // 相对路径
'./file.txt'           // 相对路径
'../parent/file.txt'   // 相对路径
```

---

## 2. 数据类型

### 2.1 FileInfo

文件/目录信息。

```typescript
/**
 * 文件/目录信息
 */
interface FileInfo {
  /**
   * 文件/目录名称
   * @example "index.ts"
   */
  name: string

  /**
   * 完整路径
   * @example "/app/src/index.ts"
   */
  path: string

  /**
   * 类型
   * @see FileType
   */
  type: FileType

  /**
   * 文件大小（字节）
   * 目录大小为 0 或目录本身的 inode 大小
   */
  size: number

  /**
   * 权限位（八进制）
   * @example 0o644
   */
  mode: number

  /**
   * 权限字符串
   * @example "rw-r--r--"
   */
  permissions: string

  /**
   * 所有者用户名
   * @example "user"
   */
  owner: string

  /**
   * 所属组名
   * @example "user"
   */
  group: string

  /**
   * 最后修改时间
   */
  modifiedAt: Date

  /**
   * 最后访问时间
   */
  accessedAt: Date

  /**
   * 创建时间（如果文件系统支持）
   */
  createdAt?: Date

  /**
   * 符号链接目标路径（仅 type=symlink 时有值）
   * @example "/usr/bin/python3"
   */
  symlinkTarget?: string

  /**
   * 是否是隐藏文件（以 . 开头）
   */
  hidden: boolean

  /**
   * MIME 类型（仅文件有值）
   * @example "text/plain"
   */
  mimeType?: string
}
```

```python
from dataclasses import dataclass
from datetime import datetime
from typing import Optional
from enum import Enum

class FileType(str, Enum):
    FILE = "file"
    DIRECTORY = "dir"
    SYMLINK = "symlink"

@dataclass
class FileInfo:
    """文件/目录信息"""

    name: str
    """文件/目录名称"""

    path: str
    """完整路径"""

    type: FileType
    """类型"""

    size: int
    """文件大小（字节）"""

    mode: int
    """权限位（八进制）"""

    permissions: str
    """权限字符串，如 'rw-r--r--'"""

    owner: str
    """所有者用户名"""

    group: str
    """所属组名"""

    modified_at: datetime
    """最后修改时间"""

    accessed_at: datetime
    """最后访问时间"""

    created_at: Optional[datetime] = None
    """创建时间"""

    symlink_target: Optional[str] = None
    """符号链接目标路径"""

    hidden: bool = False
    """是否是隐藏文件"""

    mime_type: Optional[str] = None
    """MIME 类型"""
```

### 2.2 WriteEntry

批量写入条目。

```typescript
/**
 * 批量写入文件条目
 */
interface WriteEntry {
  /**
   * 目标路径（绝对路径���
   */
  path: string

  /**
   * 文件内容
   * - string: 文本内容（UTF-8 编码）
   * - Uint8Array: 二进制内容
   * - Blob: 二进制数据
   * - ReadableStream: 流式数据
   */
  content: string | Uint8Array | Blob | ReadableStream<Uint8Array>

  /**
   * 文件权限（八进制）
   * @default 0o644
   */
  mode?: number

  /**
   * 是否创建父目录
   * @default true
   */
  createParents?: boolean

  /**
   * 是否覆盖已存在的文件
   * @default true
   */
  overwrite?: boolean
}
```

```python
@dataclass
class WriteEntry:
    """批量写入文件条目"""

    path: str
    """目标路径"""

    content: Union[str, bytes, BinaryIO]
    """文件内容"""

    mode: int = 0o644
    """文件权限"""

    create_parents: bool = True
    """是否创建父目录"""

    overwrite: bool = True
    """是否覆盖已存在的文件"""
```

### 2.3 FileEvent

文件系统事件。

```typescript
/**
 * 文件系统事件类型
 */
enum FileEventType {
  /** 创建 */
  CREATED = 'created',
  /** 修改 */
  MODIFIED = 'modified',
  /** 删除 */
  DELETED = 'deleted',
  /** 重命名 */
  RENAMED = 'renamed'
}

/**
 * 文件系统事件
 */
interface FileEvent {
  /**
   * 事件类型
   */
  type: FileEventType

  /**
   * 文件/目录路径
   */
  path: string

  /**
   * 旧路径（仅 type=renamed 时有值）
   */
  oldPath?: string

  /**
   * 事件时间戳
   */
  timestamp: Date

  /**
   * 是否是目录
   */
  isDirectory: boolean
}
```

### 2.4 WatchHandle

监视句柄。

```typescript
/**
 * 文件监视句柄
 */
interface WatchHandle {
  /**
   * 监视 ID
   */
  readonly id: string

  /**
   * 监视的路径
   */
  readonly path: string

  /**
   * 是否正在监视
   */
  readonly active: boolean

  /**
   * 停止监视
   */
  stop(): Promise<void>

  /**
   * 暂停监视
   */
  pause(): void

  /**
   * 恢复监视
   */
  resume(): void
}
```

### 2.5 SearchFilesParams

文件搜索参数。

```typescript
/**
 * 文件搜索参数
 */
interface SearchFilesParams {
  /**
   * 搜索起始路径
   */
  path: string

  /**
   * 匹配模式
   * - glob 模式: "*.ts", "**\/*.json"
   * - 正则表达式（以 / 开头和结尾）: "/.*\\.test\\.ts$/"
   */
  pattern: string

  /**
   * 最大搜索深度
   * - undefined: 不限制
   * - 0: 仅当前目录
   * - n: 最多 n 层子目录
   * @default undefined
   */
  maxDepth?: number

  /**
   * 是否包含隐藏文件（以 . 开头）
   * @default false
   */
  includeHidden?: boolean

  /**
   * 限制文件类型
   */
  type?: FileType

  /**
   * 最大返回数量
   * @default 1000
   */
  limit?: number

  /**
   * 排除的目录模式
   * @example ["node_modules", ".git", "dist"]
   */
  excludeDirs?: string[]

  /**
   * 文件大小限制（字节）
   */
  sizeRange?: {
    min?: number
    max?: number
  }

  /**
   * 修改时间范围
   */
  modifiedRange?: {
    after?: Date
    before?: Date
  }
}
```

### 2.6 GrepParams

内容搜索参数。

```typescript
/**
 * 文件内容搜索参数
 */
interface GrepParams {
  /**
   * 搜索路径（文件或目录）
   */
  path: string

  /**
   * 搜索模式（正则表达式）
   */
  pattern: string

  /**
   * 是否区分大小写
   * @default true
   */
  caseSensitive?: boolean

  /**
   * 是否全词匹配
   * @default false
   */
  wholeWord?: boolean

  /**
   * 文件过滤 glob 模式
   * @example "*.ts"
   */
  include?: string

  /**
   * 排除的 glob 模式
   * @example ["*.min.js", "*.map"]
   */
  exclude?: string[]

  /**
   * 最大返回数量
   * @default 1000
   */
  limit?: number

  /**
   * 返回匹配行的上下文行数
   * @default 0
   */
  context?: number

  /**
   * 是否搜索二进制文件
   * @default false
   */
  searchBinary?: boolean
}

/**
 * 搜索结果匹配项
 */
interface GrepMatch {
  /**
   * 文件路径
   */
  file: string

  /**
   * 行号（1-based）
   */
  line: number

  /**
   * 列号（1-based）
   */
  column: number

  /**
   * 匹配的行内容
   */
  content: string

  /**
   * 上下文行（如果指定了 context）
   */
  contextBefore?: string[]
  contextAfter?: string[]
}
```

---

## 3. FileSystemService 接口

### 3.1 read

读取文件内容。

```typescript
/**
 * 读取文件内容（文本格式）
 *
 * @param path - 文件路径（绝对路径）
 * @param options - 读取选项
 * @returns 文件文本内容
 * @throws {FileNotFoundError} 文件不存在
 * @throws {FileIsDirectoryError} 路径是目录
 * @throws {PermissionDeniedError} 没有读取权限
 *
 * @example
 * const content = await sandbox.fs.read('/app/config.json')
 */
read(path: string, options?: ReadOptions & { format?: 'text' }): Promise<string>

/**
 * 读取文件内容（二进制格式）
 *
 * @param path - 文件路径
 * @param options - 读取选项
 * @returns 文件二进制内容
 *
 * @example
 * const bytes = await sandbox.fs.read('/app/image.png', { format: 'bytes' })
 */
read(path: string, options: ReadOptions & { format: 'bytes' }): Promise<Uint8Array>

/**
 * 读取文件内容（流格式）
 *
 * 适用于大文件，避免一次性加载到内存。
 *
 * @param path - 文件路径
 * @param options - 读取选项
 * @returns 可读流
 *
 * @example
 * const stream = await sandbox.fs.read('/app/large-file.log', { format: 'stream' })
 * for await (const chunk of stream) {
 *   process.stdout.write(chunk)
 * }
 */
read(path: string, options: ReadOptions & { format: 'stream' }): Promise<ReadableStream<Uint8Array>>

/**
 * 读取选项
 */
interface ReadOptions {
  /**
   * 读取格式
   * @default 'text'
   */
  format?: 'text' | 'bytes' | 'stream'

  /**
   * 文本编码（仅 format='text' 时有效）
   * @default 'utf-8'
   */
  encoding?: string

  /**
   * 起始位置（字节偏移）
   */
  offset?: number

  /**
   * 读取长度（字节数）
   */
  length?: number

  /**
   * 运行用户
   */
  user?: string

  /**
   * 请求超时（毫秒）
   */
  timeout?: number
}
```

```python
from typing import Union, Iterator, overload, Literal

class FileSystemService:
    @overload
    def read(
        self,
        path: str,
        format: Literal["text"] = "text",
        encoding: str = "utf-8",
        user: Optional[str] = None,
        timeout: Optional[float] = None
    ) -> str: ...

    @overload
    def read(
        self,
        path: str,
        format: Literal["bytes"],
        user: Optional[str] = None,
        timeout: Optional[float] = None
    ) -> bytes: ...

    @overload
    def read(
        self,
        path: str,
        format: Literal["stream"],
        user: Optional[str] = None,
        timeout: Optional[float] = None
    ) -> Iterator[bytes]: ...

    def read(
        self,
        path: str,
        format: str = "text",
        encoding: str = "utf-8",
        user: Optional[str] = None,
        timeout: Optional[float] = None
    ) -> Union[str, bytes, Iterator[bytes]]:
        """
        读取文件内容

        Args:
            path: 文件路径（绝对路径）
            format: 读取格式 ('text' | 'bytes' | 'stream')
            encoding: 文本编码（仅 format='text' 时有效）
            user: 运行用户
            timeout: 请求超时（秒）

        Returns:
            文件内容（格式取决于 format 参数）

        Raises:
            FileNotFoundError: 文件不存在
            FileIsDirectoryError: 路径是目录
            PermissionDeniedError: 没有读取权限
        """
```

### 3.2 write

写入文件。

```typescript
/**
 * 写入文件
 *
 * @param path - 目标路径（绝对路径）
 * @param content - 文件内容
 * @param options - 写入选项
 * @returns 写入结果信息
 * @throws {FileIsDirectoryError} 目标是目录
 * @throws {PermissionDeniedError} 没有写入权限
 * @throws {DiskFullError} 磁盘空间不足
 *
 * @example
 * // 写入文本文件
 * await sandbox.fs.write('/app/config.json', JSON.stringify(config))
 *
 * @example
 * // 写入二进制文件
 * await sandbox.fs.write('/app/data.bin', new Uint8Array([0x00, 0x01, 0x02]))
 *
 * @example
 * // 追加写入
 * await sandbox.fs.write('/app/log.txt', 'new line\n', { append: true })
 */
write(
  path: string,
  content: string | Uint8Array | Blob | ReadableStream<Uint8Array>,
  options?: WriteOptions
): Promise<WriteResult>

/**
 * 写入选项
 */
interface WriteOptions {
  /**
   * 文件权限（八进制）
   * @default 0o644
   */
  mode?: number

  /**
   * 是否创建父目录
   * @default true
   */
  createParents?: boolean

  /**
   * 是否覆盖已存在的文件
   * @default true
   */
  overwrite?: boolean

  /**
   * 追加模式（不覆盖，在末尾追加）
   * @default false
   */
  append?: boolean

  /**
   * 文本编码（写入字符串时）
   * @default 'utf-8'
   */
  encoding?: string

  /**
   * 运行用户
   */
  user?: string

  /**
   * 请求超时（毫秒）
   */
  timeout?: number
}

/**
 * 写入结果
 */
interface WriteResult {
  /**
   * 文件路径
   */
  path: string

  /**
   * 写入字节数
   */
  bytesWritten: number

  /**
   * 文件是否是新创建的
   */
  created: boolean
}
```

```python
def write(
    self,
    path: str,
    content: Union[str, bytes, BinaryIO],
    *,
    mode: int = 0o644,
    create_parents: bool = True,
    overwrite: bool = True,
    append: bool = False,
    encoding: str = "utf-8",
    user: Optional[str] = None,
    timeout: Optional[float] = None
) -> WriteResult:
    """
    写入文件

    Args:
        path: 目标路径
        content: 文件内容
        mode: 文件权限
        create_parents: 是否创建父目录
        overwrite: 是否覆盖已存在的文件
        append: ��否追加模式
        encoding: 文本编码
        user: 运行用户
        timeout: 请求超时（秒）

    Returns:
        写入结果

    Raises:
        FileIsDirectoryError: 目标是目录
        PermissionDeniedError: 没有写入权限
        DiskFullError: 磁盘空间不足
    """
```

### 3.3 writeFiles

批量写入文件。

```typescript
/**
 * 批量写入文件
 *
 * 原子操作：要么全部成功，要么全部失败。
 *
 * @param files - 文件列表
 * @param options - 写入选项
 * @returns 写入结果列表
 *
 * @example
 * await sandbox.fs.writeFiles([
 *   { path: '/app/index.ts', content: 'console.log("hello")' },
 *   { path: '/app/package.json', content: JSON.stringify(pkg) }
 * ])
 */
writeFiles(files: WriteEntry[], options?: RequestOptions): Promise<WriteResult[]>
```

### 3.4 mkdir

创建目录。

```typescript
/**
 * 创建目录
 *
 * @param path - 目录路径
 * @param options - 创建选项
 * @returns 是否创建成功（如果已存在返回 false）
 * @throws {FileExistsError} 存在同名文件
 * @throws {PermissionDeniedError} 没有权限
 *
 * @example
 * // 创建单个目录
 * await sandbox.fs.mkdir('/app/logs')
 *
 * @example
 * // 递归创建
 * await sandbox.fs.mkdir('/app/data/cache/temp', { recursive: true })
 */
mkdir(
  path: string,
  options?: {
    /**
     * 是否递归创建父目录
     * @default false
     */
    recursive?: boolean

    /**
     * 目录权限
     * @default 0o755
     */
    mode?: number

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<boolean>
```

```python
def mkdir(
    self,
    path: str,
    *,
    recursive: bool = False,
    mode: int = 0o755,
    user: Optional[str] = None,
    timeout: Optional[float] = None
) -> bool:
    """
    创建目录

    Args:
        path: 目录路径
        recursive: 是否递归创建父目录
        mode: 目录权限
        user: 运行用户
        timeout: 请求超时

    Returns:
        是否创建成功（如果已存在返回 False）

    Raises:
        FileExistsError: 存在同名文件
        PermissionDeniedError: 没有权限
    """
```

### 3.5 list

列出目录内容。

```typescript
/**
 * 列出目录内容
 *
 * @param path - 目录路径
 * @param options - 列表选项
 * @returns 文件/目录信息列表
 * @throws {FileNotFoundError} 目录不存在
 * @throws {FileNotDirectoryError} 路径不是目录
 *
 * @example
 * // 列出当前目录
 * const files = await sandbox.fs.list('/app')
 *
 * @example
 * // 递归列出所有文件
 * const allFiles = await sandbox.fs.list('/app', { depth: -1 })
 *
 * @example
 * // 只列出文件
 * const onlyFiles = await sandbox.fs.list('/app', { type: 'file' })
 */
list(
  path: string,
  options?: {
    /**
     * 递归深度
     * - 1: 仅当前目录（默认）
     * - 0: 返回目录本身的信息
     * - n: 递归 n 层
     * - -1: 递归所有层级
     * @default 1
     */
    depth?: number

    /**
     * 过滤文件类型
     */
    type?: FileType

    /**
     * 是否包含隐藏文件
     * @default true
     */
    includeHidden?: boolean

    /**
     * 排序字段
     * @default 'name'
     */
    sortBy?: 'name' | 'size' | 'modifiedAt' | 'type'

    /**
     * 排序方向
     * @default 'asc'
     */
    sortOrder?: 'asc' | 'desc'

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<FileInfo[]>
```

```python
def list(
    self,
    path: str,
    *,
    depth: int = 1,
    type: Optional[FileType] = None,
    include_hidden: bool = True,
    sort_by: str = "name",
    sort_order: str = "asc",
    user: Optional[str] = None,
    timeout: Optional[float] = None
) -> List[FileInfo]:
    """
    列出目录内容

    Args:
        path: 目录路径
        depth: 递归深度 (1=当前目录, -1=所有层级)
        type: 过滤文件类型
        include_hidden: 是否包含隐藏文件
        sort_by: 排序字段 ('name' | 'size' | 'modifiedAt' | 'type')
        sort_order: 排序方向 ('asc' | 'desc')
        user: 运行用户
        timeout: 请求超时

    Returns:
        文件/目录信息列表

    Raises:
        FileNotFoundError: 目录不存在
        FileNotDirectoryError: 路径不是目录
    """
```

### 3.6 copy

复制文件或目录。

```typescript
/**
 * 复制文件或目录
 *
 * @param src - 源路径
 * @param dst - 目标路径
 * @param options - 复制选项
 * @throws {FileNotFoundError} 源不存在
 * @throws {FileExistsError} 目标已存在且 overwrite=false
 * @throws {PermissionDeniedError} 没有权限
 *
 * @example
 * // 复制文件
 * await sandbox.fs.copy('/app/config.json', '/app/config.backup.json')
 *
 * @example
 * // 复制目录
 * await sandbox.fs.copy('/app/src', '/app/src-backup', { recursive: true })
 */
copy(
  src: string,
  dst: string,
  options?: {
    /**
     * 是否递归复制（目录时必须为 true）
     * @default false
     */
    recursive?: boolean

    /**
     * 是否覆盖已存在的目标
     * @default false
     */
    overwrite?: boolean

    /**
     * 是否保留权限和时间戳
     * @default true
     */
    preserveAttributes?: boolean

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<void>
```

### 3.7 move

移动/重命名文件或目录。

```typescript
/**
 * 移动或重命名文件/目录
 *
 * @param src - 源路径
 * @param dst - 目标路径
 * @param options - 移动选项
 * @throws {FileNotFoundError} 源不存在
 * @throws {FileExistsError} 目标已存在且 overwrite=false
 * @throws {PermissionDeniedError} 没有权限
 *
 * @example
 * // 重命名文件
 * await sandbox.fs.move('/app/old.txt', '/app/new.txt')
 *
 * @example
 * // 移动目录
 * await sandbox.fs.move('/app/temp', '/backup/temp')
 */
move(
  src: string,
  dst: string,
  options?: {
    /**
     * 是否覆盖已存在的目标
     * @default false
     */
    overwrite?: boolean

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<void>
```

### 3.8 remove

删除文件或目录。

```typescript
/**
 * 删除文件或目录
 *
 * @param path - 文件/目录路径
 * @param options - 删除选项
 * @throws {FileNotFoundError} 路径不存在（除非 force=true）
 * @throws {DirectoryNotEmptyError} 目录非空且 recursive=false
 * @throws {PermissionDeniedError} 没有权限
 *
 * @example
 * // 删除文件
 * await sandbox.fs.remove('/app/temp.txt')
 *
 * @example
 * // 递归删除目录
 * await sandbox.fs.remove('/app/cache', { recursive: true })
 *
 * @example
 * // 强制删除（忽略不存在的文件）
 * await sandbox.fs.remove('/app/maybe-exists.txt', { force: true })
 */
remove(
  path: string,
  options?: {
    /**
     * 是否递归删除（删除非空目录时必须为 true）
     * @default false
     */
    recursive?: boolean

    /**
     * 是否忽略不存在的文件
     * @default false
     */
    force?: boolean

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<void>
```

### 3.9 exists

检查文件或目录是否存在。

```typescript
/**
 * 检查文件或目录是否存在
 *
 * @param path - 文件/目录路径
 * @param options - 选项
 * @returns 是否存在
 *
 * @example
 * if (await sandbox.fs.exists('/app/config.json')) {
 *   // 文件存在
 * }
 */
exists(
  path: string,
  options?: {
    /**
     * 运行用户
     */
    user?: string
  }
): Promise<boolean>
```

### 3.10 stat

获取文件/目录信息。

```typescript
/**
 * 获取文件或目录详细信息
 *
 * @param path - 文件/目录路径
 * @param options - 选项
 * @returns 文件信息
 * @throws {FileNotFoundError} 路径不存在
 *
 * @example
 * const info = await sandbox.fs.stat('/app/index.ts')
 * console.log(`Size: ${info.size}, Modified: ${info.modifiedAt}`)
 */
stat(
  path: string,
  options?: {
    /**
     * 是否跟随符号链接
     * @default true
     */
    followSymlinks?: boolean

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<FileInfo>
```

### 3.11 find

搜索文件。

```typescript
/**
 * 按名称模式搜索文件
 *
 * @param params - 搜索参数
 * @param options - 请求选项
 * @returns 匹配的文件路径列表
 *
 * @example
 * // 查找所有 TypeScript 文件
 * const tsFiles = await sandbox.fs.find({
 *   path: '/app/src',
 *   pattern: '**\/*.ts'
 * })
 *
 * @example
 * // 查找特定文件
 * const configs = await sandbox.fs.find({
 *   path: '/app',
 *   pattern: '**\/config.{json,yaml,yml}',
 *   excludeDirs: ['node_modules', '.git']
 * })
 */
find(params: SearchFilesParams, options?: RequestOptions): Promise<string[]>
```

```python
def find(
    self,
    path: str,
    pattern: str,
    *,
    max_depth: Optional[int] = None,
    include_hidden: bool = False,
    type: Optional[FileType] = None,
    limit: int = 1000,
    exclude_dirs: Optional[List[str]] = None,
    timeout: Optional[float] = None
) -> List[str]:
    """
    按名称模式搜索文件

    Args:
        path: 搜索起始路径
        pattern: 匹配模式（glob 或正则）
        max_depth: 最大搜索深度
        include_hidden: 是否包含隐藏文件
        type: 限制文件类型
        limit: 最大返回数量
        exclude_dirs: 排除的目录
        timeout: 请求超时

    Returns:
        匹配的文件路径列表
    """
```

### 3.12 grep

在文件内容中搜索。

```typescript
/**
 * 在文件内容中搜索
 *
 * @param params - 搜索参数
 * @param options - 请求选项
 * @returns 匹配结果列表
 *
 * @example
 * // 搜索包含 "TODO" 的代码
 * const matches = await sandbox.fs.grep({
 *   path: '/app/src',
 *   pattern: 'TODO:.*',
 *   include: '*.ts'
 * })
 *
 * @example
 * // 搜索并显示上下文
 * const matches = await sandbox.fs.grep({
 *   path: '/app',
 *   pattern: 'function\\s+\\w+',
 *   context: 2
 * })
 */
grep(params: GrepParams, options?: RequestOptions): Promise<GrepMatch[]>
```

```python
def grep(
    self,
    path: str,
    pattern: str,
    *,
    case_sensitive: bool = True,
    whole_word: bool = False,
    include: Optional[str] = None,
    exclude: Optional[List[str]] = None,
    limit: int = 1000,
    context: int = 0,
    timeout: Optional[float] = None
) -> List[GrepMatch]:
    """
    在文件内容中搜索

    Args:
        path: 搜索路径
        pattern: 正则表达式模式
        case_sensitive: 是否区分大小写
        whole_word: 是否全词匹配
        include: 文件过滤模式
        exclude: 排除的模式
        limit: 最大返回数量
        context: 上下文行数
        timeout: 请求超时

    Returns:
        匹配结果列表
    """
```

### 3.13 watch

监视文件/目录变化。

```typescript
/**
 * 监视文件或目录变化
 *
 * @param path - 监视路径
 * @param callback - 事件回调
 * @param options - 监视选项
 * @returns 监视句柄
 *
 * @example
 * // 监视单个文件
 * const handle = await sandbox.fs.watch('/app/config.json', (event) => {
 *   console.log(`File ${event.type}: ${event.path}`)
 * })
 *
 * // 停止监视
 * await handle.stop()
 *
 * @example
 * // 递归监视目录
 * const handle = await sandbox.fs.watch('/app/src', (event) => {
 *   if (event.type === 'modified') {
 *     console.log(`File changed: ${event.path}`)
 *   }
 * }, { recursive: true })
 */
watch(
  path: string,
  callback: (event: FileEvent) => void | Promise<void>,
  options?: {
    /**
     * 是否递归监视子目录
     * @default false
     */
    recursive?: boolean

    /**
     * 监视超时（毫秒），超时后自动停止
     * @default 0 (不超时)
     */
    timeoutMs?: number

    /**
     * 事件过滤
     */
    events?: FileEventType[]

    /**
     * 文件过滤模式
     */
    include?: string

    /**
     * 排除模式
     */
    exclude?: string[]

    /**
     * 退出回调
     */
    onExit?: (error?: Error) => void

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<WatchHandle>
```

```python
def watch(
    self,
    path: str,
    callback: Callable[[FileEvent], None],
    *,
    recursive: bool = False,
    timeout_ms: int = 0,
    events: Optional[List[FileEventType]] = None,
    include: Optional[str] = None,
    exclude: Optional[List[str]] = None,
    user: Optional[str] = None
) -> WatchHandle:
    """
    监视文件或目录变化

    Args:
        path: 监视路径
        callback: 事件回调函数
        recursive: 是否递归监视子目录
        timeout_ms: 监视超时（毫秒）
        events: 事件过滤
        include: 文件过滤模式
        exclude: 排除模式
        user: 运行用户

    Returns:
        监视句柄
    """
```

### 3.14 chmod

修改文件权限。

```typescript
/**
 * 修改文件或目录权限
 *
 * @param path - 文件/目录路径
 * @param mode - 权限模式
 * @param options - 选项
 * @throws {FileNotFoundError} 路径不存在
 * @throws {PermissionDeniedError} 没有权限
 *
 * @example
 * // 使用八进制数字
 * await sandbox.fs.chmod('/app/script.sh', 0o755)
 *
 * @example
 * // 使用字符串
 * await sandbox.fs.chmod('/app/script.sh', '755')
 *
 * @example
 * // 递归修改目录
 * await sandbox.fs.chmod('/app/bin', 0o755, { recursive: true })
 */
chmod(
  path: string,
  mode: number | string,
  options?: {
    /**
     * 是否递归应用到子文件/目录
     * @default false
     */
    recursive?: boolean

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<void>
```

### 3.15 chown

修改文件所有者。

```typescript
/**
 * 修改文件或目录所有者
 *
 * @param path - 文件/目录路径
 * @param owner - 所有者用户名
 * @param group - 所属组名（可选）
 * @param options - 选项
 * @throws {FileNotFoundError} 路径不存在
 * @throws {PermissionDeniedError} 没有权限
 * @throws {UserNotFoundError} 用户不存在
 *
 * @example
 * await sandbox.fs.chown('/app/data', 'www-data', 'www-data')
 */
chown(
  path: string,
  owner: string,
  group?: string,
  options?: {
    /**
     * 是否递归应用
     * @default false
     */
    recursive?: boolean

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<void>
```

### 3.16 upload

上传文件到 Sandbox。

```typescript
/**
 * 上传本地文件到 Sandbox
 *
 * 支持大文件上传，使用分块传输。
 *
 * @param localPath - 本地文件路径
 * @param remotePath - Sandbox 内目标路径
 * @param options - 上传选项
 * @throws {FileNotFoundError} 本地文件不存在
 * @throws {PermissionDeniedError} 没有写入权限
 * @throws {DiskFullError} 磁盘空间不足
 *
 * @example
 * await sandbox.fs.upload('./data.zip', '/app/data.zip')
 *
 * @example
 * // 带进度回调
 * await sandbox.fs.upload('./large-file.tar', '/app/backup.tar', {
 *   onProgress: (progress) => {
 *     console.log(`Upload: ${progress.percent}%`)
 *   }
 * })
 */
upload(
  localPath: string,
  remotePath: string,
  options?: {
    /**
     * 进度回调
     */
    onProgress?: (progress: ProgressInfo) => void

    /**
     * 文件权限
     * @default 0o644
     */
    mode?: number

    /**
     * 是否覆盖已存在的文件
     * @default true
     */
    overwrite?: boolean

    /**
     * 运行用户
     */
    user?: string
  }
): Promise<void>
```

### 3.17 download

从 Sandbox 下载文件。

```typescript
/**
 * 从 Sandbox 下载文件
 *
 * @param remotePath - Sandbox 内文件路径
 * @param localPath - 本地目标路径（可选）
 * @param options - 下载选项
 * @returns 如果未指定 localPath，返回文件内容
 * @throws {FileNotFoundError} 远程文件不存在
 *
 * @example
 * // 下载到本地文件
 * await sandbox.fs.download('/app/export.csv', './export.csv')
 *
 * @example
 * // 下载到内存
 * const content = await sandbox.fs.download('/app/config.json')
 */
download(remotePath: string, localPath?: string, options?: DownloadOptions): Promise<Buffer | void>

/**
 * 下载选项
 */
interface DownloadOptions {
  /**
   * 进度回调
   */
  onProgress?: (progress: ProgressInfo) => void

  /**
   * 是否覆盖本地文件
   * @default true
   */
  overwrite?: boolean

  /**
   * 超时时间（毫秒）
   */
  timeout?: number

  /**
   * 运行用户
   */
  user?: string
}
```

---

## 4. REST API

### 4.1 端点列表

| 方法 | 端点 | 描述 |
|------|------|------|
| GET | `/api/v1/sandboxes/{id}/fs/read` | 读取文件 |
| POST | `/api/v1/sandboxes/{id}/fs/write` | 写入文件 |
| POST | `/api/v1/sandboxes/{id}/fs/write-batch` | 批量写入 |
| GET | `/api/v1/sandboxes/{id}/fs/list` | 列出目录 |
| POST | `/api/v1/sandboxes/{id}/fs/mkdir` | 创建目录 |
| POST | `/api/v1/sandboxes/{id}/fs/copy` | 复制 |
| POST | `/api/v1/sandboxes/{id}/fs/move` | 移动/重命名 |
| DELETE | `/api/v1/sandboxes/{id}/fs/remove` | 删除 |
| GET | `/api/v1/sandboxes/{id}/fs/stat` | 获取信息 |
| GET | `/api/v1/sandboxes/{id}/fs/exists` | 检查存在 |
| POST | `/api/v1/sandboxes/{id}/fs/find` | 搜索文件 |
| POST | `/api/v1/sandboxes/{id}/fs/grep` | 搜索内容 |
| WS | `/api/v1/sandboxes/{id}/fs/watch` | 监视变化 |
| POST | `/api/v1/sandboxes/{id}/fs/chmod` | 修改权限 |
| POST | `/api/v1/sandboxes/{id}/fs/chown` | 修改所有者 |
| POST | `/api/v1/sandboxes/{id}/fs/upload` | 上传文件 |
| GET | `/api/v1/sandboxes/{id}/fs/download` | 下载文件 |

### 4.2 读取文件

**请求**:

```http
GET /api/v1/sandboxes/sbx-abc123/fs/read?path=/app/config.json&format=text
Authorization: Bearer <token>
```

**响应** (200 OK):

```json
{
  "content": "{\n  \"name\": \"my-app\"\n}",
  "path": "/app/config.json",
  "size": 25,
  "encoding": "utf-8"
}
```

### 4.3 写入文件

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/fs/write
Content-Type: application/json
Authorization: Bearer <token>

{
  "path": "/app/config.json",
  "content": "eyJuYW1lIjogIm15LWFwcCJ9",
  "encoding": "base64",
  "mode": 420,
  "createParents": true
}
```

**响应** (200 OK):

```json
{
  "path": "/app/config.json",
  "bytesWritten": 19,
  "created": true
}
```

### 4.4 列出目录

**请求**:

```http
GET /api/v1/sandboxes/sbx-abc123/fs/list?path=/app&depth=1&sortBy=name
Authorization: Bearer <token>
```

**响应** (200 OK):

```json
{
  "items": [
    {
      "name": "src",
      "path": "/app/src",
      "type": "dir",
      "size": 4096,
      "mode": 493,
      "permissions": "rwxr-xr-x",
      "owner": "user",
      "group": "user",
      "modifiedAt": "2024-01-15T10:30:00Z",
      "hidden": false
    },
    {
      "name": "package.json",
      "path": "/app/package.json",
      "type": "file",
      "size": 1234,
      "mode": 420,
      "permissions": "rw-r--r--",
      "owner": "user",
      "group": "user",
      "modifiedAt": "2024-01-15T10:25:00Z",
      "hidden": false,
      "mimeType": "application/json"
    }
  ],
  "total": 2
}
```

### 4.5 搜索文件

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/fs/find
Content-Type: application/json
Authorization: Bearer <token>

{
  "path": "/app/src",
  "pattern": "**/*.ts",
  "excludeDirs": ["node_modules", "dist"],
  "limit": 100
}
```

**响应** (200 OK):

```json
{
  "matches": [
    "/app/src/index.ts",
    "/app/src/utils/helper.ts",
    "/app/src/components/Button.tsx"
  ],
  "total": 3,
  "truncated": false
}
```

### 4.6 监视变化 (WebSocket)

**连接**:

```
WS /api/v1/sandboxes/sbx-abc123/fs/watch?path=/app/src&recursive=true
```

**服务器消息**:

```json
{
  "type": "event",
  "data": {
    "type": "modified",
    "path": "/app/src/index.ts",
    "timestamp": "2024-01-15T10:35:00Z",
    "isDirectory": false
  }
}
```

---

## 5. 使用示例

### 5.1 TypeScript 示例

```typescript
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function fileOperations() {
  const client = new WorkspaceClient({ apiUrl: 'https://api.example.com' })
  const sandbox = await client.sandbox.create({ template: 'node:20' })

  try {
    // 创建项目结构
    await sandbox.fs.mkdir('/app/src', { recursive: true })

    // 写入多个文件
    await sandbox.fs.writeFiles([
      {
        path: '/app/package.json',
        content: JSON.stringify({
          name: 'my-app',
          version: '1.0.0',
          main: 'src/index.js'
        }, null, 2)
      },
      {
        path: '/app/src/index.js',
        content: 'console.log("Hello, World!");'
      }
    ])

    // 读取文件
    const pkg = await sandbox.fs.read('/app/package.json')
    console.log('Package:', JSON.parse(pkg))

    // 列出目录
    const files = await sandbox.fs.list('/app', { depth: -1 })
    console.log('All files:', files.map(f => f.path))

    // 搜索文件
    const jsFiles = await sandbox.fs.find({
      path: '/app',
      pattern: '**/*.js'
    })
    console.log('JS files:', jsFiles)

    // 搜索内容
    const matches = await sandbox.fs.grep({
      path: '/app',
      pattern: 'console\\.log'
    })
    console.log('Console.log usages:', matches)

    // 监视文件变化
    const watchHandle = await sandbox.fs.watch('/app/src', (event) => {
      console.log(`File ${event.type}: ${event.path}`)
    }, { recursive: true })

    // 修改文件触发监视
    await sandbox.fs.write('/app/src/index.js', 'console.log("Updated!");')

    // 停止监视
    await watchHandle.stop()

    // 复制文件
    await sandbox.fs.copy('/app/src/index.js', '/app/src/index.backup.js')

    // 检查文件是否存在
    const exists = await sandbox.fs.exists('/app/src/index.backup.js')
    console.log('Backup exists:', exists)

    // 获取文件信息
    const stat = await sandbox.fs.stat('/app/src/index.js')
    console.log('File size:', stat.size, 'bytes')

    // 删除备份
    await sandbox.fs.remove('/app/src/index.backup.js')

  } finally {
    await sandbox.delete()
  }
}
```

### 5.2 Python 示例

```python
import json
from workspace_sdk import WorkspaceClient, WriteEntry

def file_operations():
    client = WorkspaceClient()

    with client.sandbox.create(template='python:3.11') as sandbox:
        # 创建目录
        sandbox.fs.mkdir('/app/src', recursive=True)

        # 写入文件
        sandbox.fs.write('/app/main.py', '''
def main():
    print("Hello, World!")

if __name__ == "__main__":
    main()
''')

        # 批量写入
        sandbox.fs.write_files([
            WriteEntry(path='/app/config.json', content=json.dumps({'debug': True})),
            WriteEntry(path='/app/requirements.txt', content='requests>=2.28.0\n')
        ])

        # 读取文件
        content = sandbox.fs.read('/app/main.py')
        print(f"File content:\n{content}")

        # 读取二进制
        binary = sandbox.fs.read('/app/config.json', format='bytes')
        print(f"Binary size: {len(binary)} bytes")

        # 列出目录
        files = sandbox.fs.list('/app', depth=-1)
        for f in files:
            print(f"{f.type}: {f.path} ({f.size} bytes)")

        # 搜索文件
        py_files = sandbox.fs.find('/app', '**/*.py')
        print(f"Python files: {py_files}")

        # 搜索内容
        matches = sandbox.fs.grep('/app', r'def\s+\w+')
        for m in matches:
            print(f"{m.file}:{m.line}: {m.content}")

        # 文件操作
        sandbox.fs.copy('/app/main.py', '/app/main.backup.py')
        sandbox.fs.move('/app/main.backup.py', '/backup/main.py')

        # 检查和获取信息
        if sandbox.fs.exists('/app/main.py'):
            stat = sandbox.fs.stat('/app/main.py')
            print(f"Mode: {oct(stat.mode)}, Owner: {stat.owner}")

        # 修改权限
        sandbox.fs.chmod('/app/main.py', 0o755)

        # 删除
        sandbox.fs.remove('/backup', recursive=True)

if __name__ == '__main__':
    file_operations()
```

---

## 6. 错误处理

### 6.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 3000 | FILE_NOT_FOUND | 404 | 文件/目录不存在 |
| 3001 | FILE_ALREADY_EXISTS | 409 | 文件/目录已存在 |
| 3002 | FILE_PERMISSION_DENIED | 403 | 没有权限 |
| 3003 | FILE_IS_DIRECTORY | 400 | 目标是目录，非文件 |
| 3004 | FILE_NOT_DIRECTORY | 400 | 目标是文件，非目录 |
| 3005 | FILE_TOO_LARGE | 413 | 文件太大 |
| 3006 | FILE_PATH_INVALID | 400 | 路径无效 |
| 3007 | FILE_OPERATION_FAILED | 500 | 操作失败 |
| 3008 | DIRECTORY_NOT_EMPTY | 400 | 目录非空 |
| 3009 | DISK_FULL | 507 | 磁盘空间不足 |

### 6.2 错误处理示例

```typescript
import {
  FileNotFoundError,
  FileExistsError,
  PermissionDeniedError,
  DiskFullError
} from '@workspace-sdk/typescript'

async function safeFileWrite(sandbox: Sandbox, path: string, content: string) {
  try {
    // 检查文件是否存在
    if (await sandbox.fs.exists(path)) {
      // 备份现有文件
      await sandbox.fs.copy(path, `${path}.backup`)
    }

    // 写入新内容
    await sandbox.fs.write(path, content)
    console.log(`Successfully wrote to ${path}`)

  } catch (error) {
    if (error instanceof PermissionDeniedError) {
      console.error(`No permission to write to ${path}`)
      // 尝试使用 root 用户
      await sandbox.fs.write(path, content, { user: 'root' })
    } else if (error instanceof DiskFullError) {
      console.error('Disk is full, cleaning up...')
      await sandbox.fs.remove('/tmp', { recursive: true })
      // 重试写入
      await sandbox.fs.write(path, content)
    } else {
      throw error
    }
  }
}
```

```python
from workspace_sdk import (
    FileNotFoundError,
    FileExistsError,
    PermissionDeniedError,
    DiskFullError
)

def safe_file_read(sandbox, path: str) -> str:
    try:
        return sandbox.fs.read(path)
    except FileNotFoundError:
        print(f"File not found: {path}")
        return ""
    except PermissionDeniedError:
        print(f"No permission to read: {path}")
        # 尝试使用 root
        return sandbox.fs.read(path, user='root')
```

---

## 附录

### A. MIME 类型映射

| 扩展名 | MIME 类型 |
|-------|----------|
| .txt | text/plain |
| .html | text/html |
| .css | text/css |
| .js | application/javascript |
| .ts | application/typescript |
| .json | application/json |
| .xml | application/xml |
| .yaml, .yml | application/x-yaml |
| .md | text/markdown |
| .py | text/x-python |
| .go | text/x-go |
| .rs | text/x-rust |
| .java | text/x-java |
| .png | image/png |
| .jpg, .jpeg | image/jpeg |
| .gif | image/gif |
| .svg | image/svg+xml |
| .pdf | application/pdf |
| .zip | application/zip |
| .tar | application/x-tar |
| .gz | application/gzip |

### B. 权限参考

| 八进制 | 字符串 | 描述 |
|-------|-------|------|
| 0o644 | rw-r--r-- | 普通文件默认权限 |
| 0o755 | rwxr-xr-x | 可执行文件/目录默认权限 |
| 0o600 | rw------- | 私有文件（仅所有者可读写） |
| 0o700 | rwx------ | 私有目录 |
| 0o666 | rw-rw-rw- | 所有人可读写 |
| 0o777 | rwxrwxrwx | 所有人完全权限 |

### C. 文件大小限制

| 操作 | 限制 |
|------|------|
| 单文件读取（text/bytes） | 100 MB |
| 单文件写入 | 1 GB |
| 批量写入总大小 | 100 MB |
| 流式读取 | 无限制 |
| 上传/下载 | 10 GB |
| 搜索返回数量 | 10,000 |
