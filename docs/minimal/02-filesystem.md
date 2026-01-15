# FileSystem 服务接口文档

FileSystem 服务提供 Sandbox 内的文件系统操作功能。

---

## 目录

- [1. 概述](#1-概述)
- [2. 类型定义](#2-类型定义)
- [3. 方法详情](#3-方法详情)
- [4. REST API](#4-rest-api)
- [5. 使用示例](#5-使用示例)
- [6. 错误处理](#6-错误处理)

---

## 1. 概述

FileSystem 服务通过 Sandbox 实例访问，提供文件和目录的基本操作。

### 1.1 功能列表

| 方法 | 描述 |
|-----|------|
| `read` | 读取文件内容 |
| `write` | 写入文件内容 |
| `mkdir` | 创建目录 |
| `list` | 列出目录内容 |
| `remove` | 删除文件或目录 |
| `move` | 移动或重命名文件 |
| `getInfo` | 获取文件信息 |

### 1.2 访问方式

```typescript
const sandbox = await client.sandbox.get('sbx-abc123')
const content = await sandbox.fs.read('/app/main.py')
```

---

## 2. 类型定义

### 2.1 FileSystem

FileSystem 服务接口。

```typescript
interface FileSystem {
  read(path: string): Promise<string>
  write(path: string, content: string): Promise<void>
  mkdir(path: string): Promise<void>
  list(path: string): Promise<FileInfo[]>
  remove(path: string): Promise<void>
  move(source: string, destination: string): Promise<void>
  getInfo(path: string): Promise<FileInfo>
}
```

**Python 定义**:

```python
class FileSystem(Protocol):
    async def read(self, path: str) -> str: ...
    async def write(self, path: str, content: str) -> None: ...
    async def mkdir(self, path: str) -> None: ...
    async def list(self, path: str) -> List[FileInfo]: ...
    async def remove(self, path: str) -> None: ...
    async def move(self, source: str, destination: str) -> None: ...
    async def get_info(self, path: str) -> FileInfo: ...
```

### 2.2 FileInfo

文件信息结构。

```typescript
interface FileInfo {
  /**
   * 文件名
   * @example "main.py"
   */
  name: string

  /**
   * 完整路径
   * @example "/app/main.py"
   */
  path: string

  /**
   * 文件类型
   */
  type: FileType

  /**
   * 文件大小 (字节)
   * @example 1024
   */
  size: number
}
```

**Python 定义**:

```python
@dataclass
class FileInfo:
    name: str
    """文件名"""

    path: str
    """完整路径"""

    type: FileType
    """文件类型"""

    size: int
    """文件大小 (字节)"""
```

### 2.3 FileType

文件类型枚举。

```typescript
type FileType = 'file' | 'directory'
```

**Python 定义**:

```python
class FileType(str, Enum):
    FILE = "file"
    DIRECTORY = "directory"
```

---

## 3. 方法详情

### 3.1 read

读取文件内容。

**签名**:

```typescript
read(path: string): Promise<string>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `path` | `string` | 是 | 文件的绝对路径 |

**返回值**:

| 类型 | 描述 |
|-----|------|
| `Promise<string>` | 文件内容 (UTF-8 编码) |

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 3001 | `FILE_NOT_FOUND` | 文件不存在 |
| 3009 | `NOT_A_FILE` | 路径是目录而非文件 |

**示例**:

```typescript
// TypeScript
const content = await sandbox.fs.read('/app/config.json')
const config = JSON.parse(content)
```

```python
# Python
content = await sandbox.fs.read("/app/config.json")
config = json.loads(content)
```

---

### 3.2 write

写入文件内容。如果文件不存在则创建，存在则覆盖。

**签名**:

```typescript
write(path: string, content: string): Promise<void>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `path` | `string` | 是 | 文件的绝对路径 |
| `content` | `string` | 是 | 文件内容 |

**返回值**: 无

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 3004 | `PERMISSION_DENIED` | 权限不足 |
| 3005 | `DISK_QUOTA_EXCEEDED` | 磁盘空间不足 |
| 3006 | `INVALID_PATH` | 无效路径 |

**示例**:

```typescript
// TypeScript
await sandbox.fs.write('/app/main.py', `
print("Hello World")
`)
```

```python
# Python
await sandbox.fs.write("/app/main.py", '''
print("Hello World")
''')
```

---

### 3.3 mkdir

创建目录。自动创建父目录 (类似 `mkdir -p`)。

**签名**:

```typescript
mkdir(path: string): Promise<void>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `path` | `string` | 是 | 目录的绝对路径 |

**返回值**: 无

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 3002 | `FILE_ALREADY_EXISTS` | 同名文件已存在 |
| 3004 | `PERMISSION_DENIED` | 权限不足 |

**示例**:

```typescript
// TypeScript
await sandbox.fs.mkdir('/app/src/components')
```

```python
# Python
await sandbox.fs.mkdir("/app/src/components")
```

---

### 3.4 list

列出目录内容。

**签名**:

```typescript
list(path: string): Promise<FileInfo[]>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `path` | `string` | 是 | 目录的绝对路径 |

**返回值**:

| 类型 | 描述 |
|-----|------|
| `Promise<FileInfo[]>` | 目录内文件和子目录列表 |

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 3001 | `FILE_NOT_FOUND` | 目录不存在 |
| 3008 | `NOT_A_DIRECTORY` | 路径是文件而非目录 |

**示例**:

```typescript
// TypeScript
const files = await sandbox.fs.list('/app')
for (const file of files) {
  console.log(`${file.type === 'directory' ? '📁' : '📄'} ${file.name}`)
}
```

```python
# Python
files = await sandbox.fs.list("/app")
for file in files:
    icon = "📁" if file.type == FileType.DIRECTORY else "📄"
    print(f"{icon} {file.name}")
```

---

### 3.5 remove

删除文件或目录。

**签名**:

```typescript
remove(path: string): Promise<void>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `path` | `string` | 是 | 文件或目录的绝对路径 |

**返回值**: 无

**说明**:
- 删除文件时直接删除
- 删除目录时递归删除所有内容

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 3001 | `FILE_NOT_FOUND` | 文件或目录不存在 |
| 3004 | `PERMISSION_DENIED` | 权限不足 |

**示例**:

```typescript
// TypeScript
await sandbox.fs.remove('/app/temp')
```

```python
# Python
await sandbox.fs.remove("/app/temp")
```

---

### 3.6 move

移动或重命名文件/目录。

**签名**:

```typescript
move(source: string, destination: string): Promise<void>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `source` | `string` | 是 | 源路径 |
| `destination` | `string` | 是 | 目标路径 |

**返回值**: 无

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 3001 | `FILE_NOT_FOUND` | 源文件不存在 |
| 3002 | `FILE_ALREADY_EXISTS` | 目标文件已存在 |
| 3004 | `PERMISSION_DENIED` | 权限不足 |

**示例**:

```typescript
// TypeScript
// 重命名
await sandbox.fs.move('/app/old.py', '/app/new.py')

// 移动到其他目录
await sandbox.fs.move('/app/file.py', '/app/src/file.py')
```

```python
# Python
# 重命名
await sandbox.fs.move("/app/old.py", "/app/new.py")

# 移动到其他目录
await sandbox.fs.move("/app/file.py", "/app/src/file.py")
```

---

### 3.7 getInfo

获取文件或目录的详细信息。

**签名**:

```typescript
getInfo(path: string): Promise<FileInfo>
```

**参数**:

| 参数 | 类型 | 必填 | 描述 |
|-----|------|-----|------|
| `path` | `string` | 是 | 文件或目录的绝对路径 |

**返回值**:

| 类型 | 描述 |
|-----|------|
| `Promise<FileInfo>` | 文件信息 |

**异常**:

| 错误码 | 名称 | 描述 |
|-------|------|------|
| 3001 | `FILE_NOT_FOUND` | 文件或目录不存在 |

**示例**:

```typescript
// TypeScript
const info = await sandbox.fs.getInfo('/app/main.py')
console.log(`Size: ${info.size} bytes`)
```

```python
# Python
info = await sandbox.fs.get_info("/app/main.py")
print(f"Size: {info.size} bytes")
```

---

## 4. REST API

所有 FileSystem API 的基础路径: `/api/v1/sandboxes/{sandboxId}/files`

### 4.1 读取文件

```
GET /api/v1/sandboxes/{sandboxId}/files/read?path={path}
```

**参数**:

| 参数 | 位置 | 类型 | 描述 |
|-----|------|-----|------|
| `sandboxId` | path | string | Sandbox ID |
| `path` | query | string | 文件路径 (URL 编码) |

**响应** (200 OK):

```
Content-Type: text/plain

print("Hello World")
```

### 4.2 写入文件

```
POST /api/v1/sandboxes/{sandboxId}/files/write?path={path}
```

**请求**:

```
Content-Type: text/plain

print("Hello World")
```

**响应** (204 No Content): 无响应体

### 4.3 创建目录

```
POST /api/v1/sandboxes/{sandboxId}/files/mkdir?path={path}
```

**响应** (204 No Content): 无响应体

### 4.4 列出目录

```
GET /api/v1/sandboxes/{sandboxId}/files?path={path}
```

**响应** (200 OK):

```json
[
  {
    "name": "main.py",
    "path": "/app/main.py",
    "type": "file",
    "size": 256
  },
  {
    "name": "src",
    "path": "/app/src",
    "type": "directory",
    "size": 0
  }
]
```

### 4.5 删除文件/目录

```
DELETE /api/v1/sandboxes/{sandboxId}/files?path={path}
```

**响应** (204 No Content): 无响应体

### 4.6 移动/重命名

```
POST /api/v1/sandboxes/{sandboxId}/files/move
```

**请求**:

```json
{
  "source": "/app/old.py",
  "destination": "/app/new.py"
}
```

**响应** (204 No Content): 无响应体

### 4.7 获取文件信息

```
GET /api/v1/sandboxes/{sandboxId}/files/info?path={path}
```

**响应** (200 OK):

```json
{
  "name": "main.py",
  "path": "/app/main.py",
  "type": "file",
  "size": 256
}
```

---

## 5. 使用示例

### 5.1 创建项目结构

```typescript
// TypeScript
async function createProject(sandbox: Sandbox) {
  // 创建目录结构
  await sandbox.fs.mkdir('/app/src')
  await sandbox.fs.mkdir('/app/tests')

  // 创建文件
  await sandbox.fs.write('/app/src/main.py', `
def main():
    print("Hello World")

if __name__ == "__main__":
    main()
`)

  await sandbox.fs.write('/app/tests/test_main.py', `
from src.main import main

def test_main():
    main()
`)

  await sandbox.fs.write('/app/requirements.txt', 'pytest==7.4.0')
}
```

```python
# Python
async def create_project(sandbox: Sandbox):
    # 创建目录结构
    await sandbox.fs.mkdir("/app/src")
    await sandbox.fs.mkdir("/app/tests")

    # 创建文件
    await sandbox.fs.write("/app/src/main.py", '''
def main():
    print("Hello World")

if __name__ == "__main__":
    main()
''')

    await sandbox.fs.write("/app/tests/test_main.py", '''
from src.main import main

def test_main():
    main()
''')

    await sandbox.fs.write("/app/requirements.txt", "pytest==7.4.0")
```

### 5.2 读取和修改配置

```typescript
// TypeScript
async function updateConfig(sandbox: Sandbox) {
  // 读取配置
  const content = await sandbox.fs.read('/app/config.json')
  const config = JSON.parse(content)

  // 修改配置
  config.debug = true
  config.version = '2.0.0'

  // 写回
  await sandbox.fs.write('/app/config.json', JSON.stringify(config, null, 2))
}
```

### 5.3 列出目录树

```typescript
// TypeScript
async function listTree(sandbox: Sandbox, path: string, indent = 0) {
  const files = await sandbox.fs.list(path)

  for (const file of files) {
    const prefix = '  '.repeat(indent)
    const icon = file.type === 'directory' ? '📁' : '📄'
    console.log(`${prefix}${icon} ${file.name}`)

    if (file.type === 'directory') {
      await listTree(sandbox, file.path, indent + 1)
    }
  }
}

// 使用
await listTree(sandbox, '/app')
```

---

## 6. 错误处理

### 6.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 3001 | `FILE_NOT_FOUND` | 404 | 文件或目录不存在 |
| 3002 | `FILE_ALREADY_EXISTS` | 409 | 文件或目录已存在 |
| 3003 | `DIRECTORY_NOT_EMPTY` | 409 | 目录非空 |
| 3004 | `PERMISSION_DENIED` | 403 | 权限不足 |
| 3005 | `DISK_QUOTA_EXCEEDED` | 507 | 磁盘空间不足 |
| 3006 | `INVALID_PATH` | 400 | 无效路径 |
| 3007 | `FILE_TOO_LARGE` | 413 | 文件过大 |
| 3008 | `NOT_A_DIRECTORY` | 400 | 不是目录 |
| 3009 | `NOT_A_FILE` | 400 | 不是文件 |

### 6.2 错误处理示例

```typescript
// TypeScript
import { FileNotFoundError, PermissionDeniedError } from '@workspace-sdk/typescript'

async function safeRead(sandbox: Sandbox, path: string): Promise<string | null> {
  try {
    return await sandbox.fs.read(path)
  } catch (error) {
    if (error instanceof FileNotFoundError) {
      console.log(`File not found: ${path}`)
      return null
    }
    if (error instanceof PermissionDeniedError) {
      console.error(`Permission denied: ${path}`)
      return null
    }
    throw error
  }
}
```

```python
# Python
from workspace_sdk.errors import FileNotFoundError, PermissionDeniedError

async def safe_read(sandbox: Sandbox, path: str) -> Optional[str]:
    try:
        return await sandbox.fs.read(path)
    except FileNotFoundError:
        print(f"File not found: {path}")
        return None
    except PermissionDeniedError:
        print(f"Permission denied: {path}")
        return None
```
