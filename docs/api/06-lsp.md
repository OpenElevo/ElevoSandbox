# LSP 服务接口文档

LSP (Language Server Protocol) 服务提供 Sandbox 内的语言服务器功能，支持代码智能提示、符号查询、跳转定义、查找引用等 IDE 特性。

---

## 目录

- [1. 概述](#1-概述)
- [2. 数据类型](#2-数据类型)
- [3. LSPService 接口](#3-lspservice-接口)
- [4. LSPServerHandle 接口](#4-lspserverhandle-接口)
- [5. REST API](#5-rest-api)
- [6. 使用示例](#6-使用示例)
- [7. 错误处理](#7-错误处理)

---

## 1. 概述

### 1.1 功能说明

| 功能 | 描述 |
|------|------|
| **符号查询** | 获取文档符号、工作空间符号 |
| **代码补全** | 智能代码补全建议 |
| **悬停信息** | 获取符号的类型和文档 |
| **跳转定义** | 跳转到符号定义位置 |
| **查找引用** | 查找符号的所有引用 |
| **诊断信息** | 获取代码错误和警告 |

### 1.2 支持的语言

| 语言 | 语言 ID | 语言服务器 |
|------|--------|-----------|
| Python | `python` | Pylsp / Pyright |
| TypeScript | `typescript` | typescript-language-server |
| JavaScript | `javascript` | typescript-language-server |
| Go | `go` | gopls |
| Rust | `rust` | rust-analyzer |
| Java | `java` | jdtls |
| C/C++ | `c`, `cpp` | clangd |
| C# | `csharp` | OmniSharp |

---

## 2. 数据类型

### 2.1 Position

位置。

```typescript
/**
 * 文本位置
 */
interface Position {
  /**
   * 行号（0-based）
   */
  line: number

  /**
   * 字符偏移（0-based）
   */
  character: number
}
```

### 2.2 Range

范围。

```typescript
/**
 * 文本范围
 */
interface Range {
  /**
   * 起始位置
   */
  start: Position

  /**
   * 结束位置
   */
  end: Position
}
```

### 2.3 Location

位置信息。

```typescript
/**
 * 文件位置信息
 */
interface Location {
  /**
   * 文件 URI
   * @example "file:///app/src/index.ts"
   */
  uri: string

  /**
   * 位置范围
   */
  range: Range
}
```

### 2.4 Symbol

符号信息。

```typescript
/**
 * 符号类型
 */
enum SymbolKind {
  FILE = 1,
  MODULE = 2,
  NAMESPACE = 3,
  PACKAGE = 4,
  CLASS = 5,
  METHOD = 6,
  PROPERTY = 7,
  FIELD = 8,
  CONSTRUCTOR = 9,
  ENUM = 10,
  INTERFACE = 11,
  FUNCTION = 12,
  VARIABLE = 13,
  CONSTANT = 14,
  STRING = 15,
  NUMBER = 16,
  BOOLEAN = 17,
  ARRAY = 18,
  OBJECT = 19,
  KEY = 20,
  NULL = 21,
  ENUM_MEMBER = 22,
  STRUCT = 23,
  EVENT = 24,
  OPERATOR = 25,
  TYPE_PARAMETER = 26
}

/**
 * 符号信息
 */
interface Symbol {
  /**
   * 符号名称
   */
  name: string

  /**
   * 符号类型
   */
  kind: SymbolKind

  /**
   * 符号类型名称
   */
  kindName: string

  /**
   * 详细信息（如函数签名）
   */
  detail?: string

  /**
   * 位置信息
   */
  location: Location

  /**
   * 容器名称（所属类/模块等）
   */
  containerName?: string

  /**
   * 子符号
   */
  children?: Symbol[]

  /**
   * 是否已弃用
   */
  deprecated?: boolean

  /**
   * 标签
   */
  tags?: string[]
}
```

```python
from dataclasses import dataclass
from typing import Optional, List
from enum import IntEnum

class SymbolKind(IntEnum):
    FILE = 1
    MODULE = 2
    NAMESPACE = 3
    PACKAGE = 4
    CLASS = 5
    METHOD = 6
    PROPERTY = 7
    FIELD = 8
    CONSTRUCTOR = 9
    ENUM = 10
    INTERFACE = 11
    FUNCTION = 12
    VARIABLE = 13
    CONSTANT = 14

@dataclass
class Position:
    line: int
    character: int

@dataclass
class Range:
    start: Position
    end: Position

@dataclass
class Location:
    uri: str
    range: Range

@dataclass
class Symbol:
    name: str
    kind: SymbolKind
    kind_name: str
    location: Location
    detail: Optional[str] = None
    container_name: Optional[str] = None
    children: Optional[List['Symbol']] = None
    deprecated: bool = False
```

### 2.5 CompletionItem

补全项。

```typescript
/**
 * 补全项类型
 */
enum CompletionItemKind {
  TEXT = 1,
  METHOD = 2,
  FUNCTION = 3,
  CONSTRUCTOR = 4,
  FIELD = 5,
  VARIABLE = 6,
  CLASS = 7,
  INTERFACE = 8,
  MODULE = 9,
  PROPERTY = 10,
  UNIT = 11,
  VALUE = 12,
  ENUM = 13,
  KEYWORD = 14,
  SNIPPET = 15,
  COLOR = 16,
  FILE = 17,
  REFERENCE = 18,
  FOLDER = 19,
  ENUM_MEMBER = 20,
  CONSTANT = 21,
  STRUCT = 22,
  EVENT = 23,
  OPERATOR = 24,
  TYPE_PARAMETER = 25
}

/**
 * 补全项
 */
interface CompletionItem {
  /**
   * 显示标签
   */
  label: string

  /**
   * 补全类型
   */
  kind: CompletionItemKind

  /**
   * 补全类型名称
   */
  kindName: string

  /**
   * 详细信息
   */
  detail?: string

  /**
   * 文档说明（Markdown）
   */
  documentation?: string

  /**
   * 排序文本
   */
  sortText?: string

  /**
   * 过滤文本
   */
  filterText?: string

  /**
   * 插入文本
   */
  insertText: string

  /**
   * 插入文本格式
   */
  insertTextFormat?: 'plainText' | 'snippet'

  /**
   * 替换范围
   */
  textEdit?: {
    range: Range
    newText: string
  }

  /**
   * 附加编辑（如自动导入）
   */
  additionalTextEdits?: Array<{
    range: Range
    newText: string
  }>

  /**
   * 是否已弃用
   */
  deprecated?: boolean

  /**
   * 是否预选
   */
  preselect?: boolean
}

/**
 * 补全列表
 */
interface CompletionList {
  /**
   * 是否不完整（需要继续输入以获取更多）
   */
  isIncomplete: boolean

  /**
   * 补全项列表
   */
  items: CompletionItem[]
}
```

### 2.6 Diagnostic

诊断信息。

```typescript
/**
 * 诊断严重性
 */
enum DiagnosticSeverity {
  ERROR = 1,
  WARNING = 2,
  INFORMATION = 3,
  HINT = 4
}

/**
 * 诊断信息
 */
interface Diagnostic {
  /**
   * 位置范围
   */
  range: Range

  /**
   * 严重性
   */
  severity: DiagnosticSeverity

  /**
   * 严重性名称
   */
  severityName: string

  /**
   * 诊断消息
   */
  message: string

  /**
   * 来源（如 "typescript", "eslint"）
   */
  source?: string

  /**
   * 诊断代码
   */
  code?: string | number

  /**
   * 代码描述 URL
   */
  codeDescription?: {
    href: string
  }

  /**
   * 相关信息
   */
  relatedInformation?: Array<{
    location: Location
    message: string
  }>

  /**
   * 标签
   */
  tags?: Array<'unnecessary' | 'deprecated'>
}
```

### 2.7 HoverInfo

悬停信息。

```typescript
/**
 * 悬停信息
 */
interface HoverInfo {
  /**
   * 内容（Markdown 格式）
   */
  contents: string

  /**
   * 适用范围
   */
  range?: Range
}
```

### 2.8 LSPServerInfo

LSP 服务器信息。

```typescript
/**
 * LSP 服务器信息
 */
interface LSPServerInfo {
  /**
   * 服务器 ID
   */
  id: string

  /**
   * 语言
   */
  language: LSPLanguageId

  /**
   * 项目路径
   */
  projectPath: string

  /**
   * 语言服务器名称
   */
  serverName: string

  /**
   * 语言服务器版本
   */
  serverVersion?: string

  /**
   * 是否正在运行
   */
  running: boolean

  /**
   * 创建时间
   */
  createdAt: Date

  /**
   * 最后活动时间
   */
  lastActivityAt: Date
}
```

---

## 3. LSPService 接口

### 3.1 create

创建 LSP 服务器。

```typescript
/**
 * 创建 LSP 服务器
 *
 * @param language - 语言 ID
 * @param projectPath - 项目路径
 * @param options - 创建选项
 * @returns LSP 服务器句柄
 * @throws {LanguageNotSupportedError} 不支持的语言
 * @throws {LSPStartFailedError} 服务器启动失败
 *
 * @example
 * const lsp = await sandbox.lsp.create('typescript', '/app')
 *
 * // 使用服务器
 * const symbols = await lsp.documentSymbols('/app/src/index.ts')
 *
 * // 关闭服务器
 * await lsp.stop()
 *
 * @example
 * // Python 项目
 * const lsp = await sandbox.lsp.create('python', '/app', {
 *   settings: {
 *     'python.analysis.typeCheckingMode': 'strict'
 *   }
 * })
 */
create(
  language: LSPLanguageId,
  projectPath: string,
  options?: {
    /**
     * 语言服务器特定设置
     */
    settings?: Record<string, unknown>

    /**
     * 是否自动安装依赖
     * @default true
     */
    autoInstall?: boolean

    /**
     * 初始化超时（毫秒）
     * @default 60000
     */
    initTimeout?: number
  }
): Promise<LSPServerHandle>
```

```python
def create(
    self,
    language: LSPLanguageId,
    project_path: str,
    *,
    settings: Optional[Dict[str, Any]] = None,
    auto_install: bool = True,
    init_timeout: int = 60000
) -> LSPServerHandle:
    """
    创建 LSP 服务器

    Args:
        language: 语言 ID
        project_path: 项目路径
        settings: 语言服务器设置
        auto_install: 是否自动安装依赖
        init_timeout: 初始化超时（毫秒）

    Returns:
        LSP 服务器句柄

    Raises:
        LanguageNotSupportedError: 不支持的语言
        LSPStartFailedError: 服务器启动失败
    """
```

### 3.2 list

列出所有 LSP 服务器。

```typescript
/**
 * 列出所有 LSP 服务器
 *
 * @returns LSP 服务器信息列表
 *
 * @example
 * const servers = await sandbox.lsp.list()
 * for (const server of servers) {
 *   console.log(`${server.language}: ${server.projectPath}`)
 * }
 */
list(): Promise<LSPServerInfo[]>
```

### 3.3 get

获取 LSP 服务器。

```typescript
/**
 * 获取 LSP 服务器句柄
 *
 * @param id - 服务器 ID
 * @returns LSP 服务器句柄
 * @throws {LSPNotFoundError} 服务器不存在
 *
 * @example
 * const lsp = await sandbox.lsp.get('lsp-abc123')
 */
get(id: string): Promise<LSPServerHandle>
```

---

## 4. LSPServerHandle 接口

### 4.1 属性

```typescript
interface LSPServerHandle {
  /**
   * 服务器 ID
   */
  readonly id: string

  /**
   * 语言
   */
  readonly language: LSPLanguageId

  /**
   * 项目路径
   */
  readonly projectPath: string

  /**
   * 是否正在运行
   */
  readonly running: boolean
}
```

### 4.2 stop

停止服务器。

```typescript
/**
 * 停止 LSP 服务器
 *
 * @example
 * await lsp.stop()
 */
stop(): Promise<void>
```

### 4.3 didOpen

通知打开文档。

```typescript
/**
 * 通知 LSP 服务器打开文档
 *
 * 在使用文档相关功能前需要先调用此方法。
 *
 * @param path - 文件路径
 * @param options - 选项
 *
 * @example
 * await lsp.didOpen('/app/src/index.ts')
 * // 现在可以使用补全、悬停等功能
 */
didOpen(
  path: string,
  options?: {
    /**
     * 文件内容（如果不提供，从文件系统读取）
     */
    content?: string

    /**
     * 版本号
     */
    version?: number
  }
): Promise<void>
```

### 4.4 didClose

通知关闭文档。

```typescript
/**
 * 通知 LSP 服务器关闭文档
 *
 * @param path - 文件路径
 */
didClose(path: string): Promise<void>
```

### 4.5 didChange

通知文档变更。

```typescript
/**
 * 通知 LSP 服务器文档变更
 *
 * @param path - 文件路径
 * @param changes - 变更列表
 *
 * @example
 * // 全量更新
 * await lsp.didChange('/app/src/index.ts', [
 *   { text: newContent }
 * ])
 *
 * @example
 * // 增量更新
 * await lsp.didChange('/app/src/index.ts', [
 *   {
 *     range: { start: { line: 5, character: 0 }, end: { line: 5, character: 10 } },
 *     text: 'new text'
 *   }
 * ])
 */
didChange(
  path: string,
  changes: Array<{
    range?: Range
    text: string
  }>
): Promise<void>
```

### 4.6 documentSymbols

获取文档符号。

```typescript
/**
 * 获取文档符号
 *
 * @param path - 文件路径
 * @returns 符号列表（树形结构）
 *
 * @example
 * await lsp.didOpen('/app/src/index.ts')
 * const symbols = await lsp.documentSymbols('/app/src/index.ts')
 *
 * for (const symbol of symbols) {
 *   console.log(`${symbol.kindName}: ${symbol.name}`)
 *   if (symbol.children) {
 *     for (const child of symbol.children) {
 *       console.log(`  ${child.kindName}: ${child.name}`)
 *     }
 *   }
 * }
 */
documentSymbols(path: string): Promise<Symbol[]>
```

### 4.7 workspaceSymbols

搜索工作空间符号。

```typescript
/**
 * 搜索工作空间符号
 *
 * @param query - 搜索关键字
 * @param options - 搜索选项
 * @returns 符号列表
 *
 * @example
 * // 搜索名为 "User" 的符号
 * const symbols = await lsp.workspaceSymbols('User')
 *
 * @example
 * // 搜索所有类
 * const classes = await lsp.workspaceSymbols('', {
 *   kinds: [SymbolKind.CLASS]
 * })
 */
workspaceSymbols(
  query: string,
  options?: {
    /**
     * 限制符号类型
     */
    kinds?: SymbolKind[]

    /**
     * 最大返回数量
     * @default 100
     */
    limit?: number
  }
): Promise<Symbol[]>
```

### 4.8 completion

获取代码补全。

```typescript
/**
 * 获取代码补全建议
 *
 * @param path - 文件路径
 * @param position - 光标位置
 * @param options - 补全选项
 * @returns 补全列表
 *
 * @example
 * await lsp.didOpen('/app/src/index.ts')
 * const completions = await lsp.completion('/app/src/index.ts', {
 *   line: 10,
 *   character: 5
 * })
 *
 * for (const item of completions.items) {
 *   console.log(`${item.kindName}: ${item.label}`)
 * }
 */
completion(
  path: string,
  position: Position,
  options?: {
    /**
     * 触发字符
     */
    triggerCharacter?: string

    /**
     * 触发类型
     */
    triggerKind?: 'invoked' | 'character' | 'incomplete'
  }
): Promise<CompletionList>
```

### 4.9 hover

获取悬停信息。

```typescript
/**
 * 获取悬停信息
 *
 * @param path - 文件路径
 * @param position - 光标位置
 * @returns 悬停信息，如果无信息返回 null
 *
 * @example
 * await lsp.didOpen('/app/src/index.ts')
 * const hover = await lsp.hover('/app/src/index.ts', {
 *   line: 10,
 *   character: 15
 * })
 *
 * if (hover) {
 *   console.log(hover.contents)
 * }
 */
hover(path: string, position: Position): Promise<HoverInfo | null>
```

### 4.10 definition

跳转到定义。

```typescript
/**
 * 跳转到定义
 *
 * @param path - 文件路径
 * @param position - 光标位置
 * @returns 定义位置列表
 *
 * @example
 * await lsp.didOpen('/app/src/index.ts')
 * const definitions = await lsp.definition('/app/src/index.ts', {
 *   line: 10,
 *   character: 15
 * })
 *
 * for (const def of definitions) {
 *   console.log(`${def.uri}:${def.range.start.line}:${def.range.start.character}`)
 * }
 */
definition(path: string, position: Position): Promise<Location[]>
```

### 4.11 references

查找引用。

```typescript
/**
 * 查找所有引用
 *
 * @param path - 文件路径
 * @param position - 光标位置
 * @param options - 选项
 * @returns 引用位置列表
 *
 * @example
 * await lsp.didOpen('/app/src/index.ts')
 * const references = await lsp.references('/app/src/index.ts', {
 *   line: 10,
 *   character: 15
 * })
 *
 * console.log(`Found ${references.length} references`)
 */
references(
  path: string,
  position: Position,
  options?: {
    /**
     * 是否包含定义位置
     * @default true
     */
    includeDeclaration?: boolean
  }
): Promise<Location[]>
```

### 4.12 diagnostics

获取诊断信息。

```typescript
/**
 * 获取文档诊断信息
 *
 * @param path - 文件路径
 * @returns 诊断信息列表
 *
 * @example
 * await lsp.didOpen('/app/src/index.ts')
 * const diagnostics = await lsp.diagnostics('/app/src/index.ts')
 *
 * const errors = diagnostics.filter(d => d.severity === DiagnosticSeverity.ERROR)
 * console.log(`Found ${errors.length} errors`)
 *
 * for (const diag of errors) {
 *   console.log(`${diag.range.start.line}: ${diag.message}`)
 * }
 */
diagnostics(path: string): Promise<Diagnostic[]>
```

### 4.13 signatureHelp

获取签名帮助。

```typescript
/**
 * 获取函数签名帮助
 *
 * @param path - 文件路径
 * @param position - 光标位置
 * @returns 签名帮助信息
 *
 * @example
 * const help = await lsp.signatureHelp('/app/src/index.ts', {
 *   line: 10,
 *   character: 20
 * })
 *
 * if (help) {
 *   console.log(`Active signature: ${help.activeSignature}`)
 *   console.log(`Active parameter: ${help.activeParameter}`)
 * }
 */
signatureHelp(path: string, position: Position): Promise<SignatureHelp | null>

/**
 * 签名帮助
 */
interface SignatureHelp {
  signatures: SignatureInformation[]
  activeSignature: number
  activeParameter: number
}

interface SignatureInformation {
  label: string
  documentation?: string
  parameters?: ParameterInformation[]
}

interface ParameterInformation {
  label: string | [number, number]
  documentation?: string
}
```

### 4.14 rename

重命名符号。

```typescript
/**
 * 重命名符号
 *
 * @param path - 文件路径
 * @param position - 光标位置
 * @param newName - 新名称
 * @returns 需要修改的文件和编辑
 *
 * @example
 * const edits = await lsp.rename('/app/src/index.ts', {
 *   line: 5,
 *   character: 10
 * }, 'newFunctionName')
 *
 * // 应用编辑
 * for (const [uri, fileEdits] of Object.entries(edits)) {
 *   const path = uri.replace('file://', '')
 *   let content = await sandbox.fs.read(path)
 *
 *   // 应用编辑（从后往前，避免位置偏移）
 *   for (const edit of fileEdits.reverse()) {
 *     // ... 应用编辑
 *   }
 *
 *   await sandbox.fs.write(path, content)
 * }
 */
rename(
  path: string,
  position: Position,
  newName: string
): Promise<Record<string, TextEdit[]>>

interface TextEdit {
  range: Range
  newText: string
}
```

### 4.15 codeAction

获取代码操作。

```typescript
/**
 * 获取代码操作（快速修复、重构等）
 *
 * @param path - 文件路径
 * @param range - 选择范围
 * @param options - 选项
 * @returns 代码操作列表
 *
 * @example
 * const actions = await lsp.codeAction('/app/src/index.ts', {
 *   start: { line: 5, character: 0 },
 *   end: { line: 5, character: 20 }
 * })
 *
 * for (const action of actions) {
 *   console.log(`${action.kind}: ${action.title}`)
 * }
 */
codeAction(
  path: string,
  range: Range,
  options?: {
    /**
     * 诊断信息（用于快速修复）
     */
    diagnostics?: Diagnostic[]

    /**
     * 只获取指定类型的操作
     */
    only?: string[]
  }
): Promise<CodeAction[]>

interface CodeAction {
  title: string
  kind?: string
  diagnostics?: Diagnostic[]
  isPreferred?: boolean
  edit?: WorkspaceEdit
  command?: Command
}

interface WorkspaceEdit {
  changes?: Record<string, TextEdit[]>
}

interface Command {
  title: string
  command: string
  arguments?: unknown[]
}
```

---

## 5. REST API

### 5.1 端点列表

| 方法 | 端点 | 描述 |
|------|------|------|
| POST | `/api/v1/sandboxes/{id}/lsp` | 创建 LSP 服务器 |
| GET | `/api/v1/sandboxes/{id}/lsp` | 列出 LSP 服务器 |
| GET | `/api/v1/sandboxes/{id}/lsp/{lid}` | 获取 LSP 服务器 |
| DELETE | `/api/v1/sandboxes/{id}/lsp/{lid}` | 停止 LSP 服务器 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/didOpen` | 打开文档 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/didClose` | 关闭文档 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/didChange` | 文档变更 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/symbols` | 文档符号 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/workspaceSymbols` | 工作空间符号 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/completion` | 代码补全 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/hover` | 悬停信息 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/definition` | 跳转定义 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/references` | 查找引用 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/diagnostics` | 诊断信息 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/rename` | 重命名 |
| POST | `/api/v1/sandboxes/{id}/lsp/{lid}/codeAction` | 代码操作 |

### 5.2 创建 LSP 服务器

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/lsp
Content-Type: application/json
Authorization: Bearer <token>

{
  "language": "typescript",
  "projectPath": "/app",
  "settings": {
    "typescript.preferences.importModuleSpecifier": "relative"
  }
}
```

**响应** (201 Created):

```json
{
  "id": "lsp-xyz789",
  "language": "typescript",
  "projectPath": "/app",
  "serverName": "typescript-language-server",
  "serverVersion": "4.0.0",
  "running": true,
  "createdAt": "2024-01-15T10:30:00Z"
}
```

### 5.3 获取代码补全

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/lsp/lsp-xyz789/completion
Content-Type: application/json
Authorization: Bearer <token>

{
  "path": "/app/src/index.ts",
  "position": {
    "line": 10,
    "character": 5
  }
}
```

**响应** (200 OK):

```json
{
  "isIncomplete": false,
  "items": [
    {
      "label": "console",
      "kind": 6,
      "kindName": "Variable",
      "detail": "var console: Console",
      "insertText": "console",
      "insertTextFormat": "plainText"
    },
    {
      "label": "const",
      "kind": 14,
      "kindName": "Keyword",
      "insertText": "const",
      "insertTextFormat": "plainText"
    }
  ]
}
```

### 5.4 获取诊断信息

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/lsp/lsp-xyz789/diagnostics
Content-Type: application/json
Authorization: Bearer <token>

{
  "path": "/app/src/index.ts"
}
```

**响应** (200 OK):

```json
{
  "diagnostics": [
    {
      "range": {
        "start": {"line": 5, "character": 10},
        "end": {"line": 5, "character": 20}
      },
      "severity": 1,
      "severityName": "Error",
      "message": "Property 'foo' does not exist on type 'Bar'.",
      "source": "typescript",
      "code": 2339
    }
  ]
}
```

---

## 6. 使用示例

### 6.1 TypeScript 示例

```typescript
import { WorkspaceClient, LSPLanguageId, SymbolKind } from '@workspace-sdk/typescript'

async function lspExample() {
  const client = new WorkspaceClient({ apiUrl: 'https://api.example.com' })
  const sandbox = await client.sandbox.create({ template: 'node:20' })

  try {
    // 创建项目文件
    await sandbox.fs.mkdir('/app/src', { recursive: true })
    await sandbox.fs.write('/app/src/index.ts', `
interface User {
  id: number;
  name: string;
  email: string;
}

function createUser(name: string, email: string): User {
  return {
    id: Date.now(),
    name,
    email
  };
}

const user = createUser('John', 'john@example.com');
console.log(user.name);
`)

    await sandbox.fs.write('/app/tsconfig.json', JSON.stringify({
      compilerOptions: {
        target: 'ES2020',
        module: 'commonjs',
        strict: true
      }
    }, null, 2))

    // 创建 LSP 服务器
    console.log('Creating LSP server...')
    const lsp = await sandbox.lsp.create('typescript', '/app')

    // 打开文档
    await lsp.didOpen('/app/src/index.ts')

    // ========== 文档符号 ==========
    console.log('\n=== Document Symbols ===')
    const symbols = await lsp.documentSymbols('/app/src/index.ts')

    function printSymbols(symbols: Symbol[], indent = 0) {
      for (const symbol of symbols) {
        console.log(`${'  '.repeat(indent)}${symbol.kindName}: ${symbol.name}`)
        if (symbol.children) {
          printSymbols(symbol.children, indent + 1)
        }
      }
    }
    printSymbols(symbols)

    // ========== 代码补全 ==========
    console.log('\n=== Completion ===')
    const completions = await lsp.completion('/app/src/index.ts', {
      line: 15, // user. 之后
      character: 18
    })

    console.log('Completions for "user.":')
    for (const item of completions.items.slice(0, 5)) {
      console.log(`  ${item.kindName}: ${item.label}`)
    }

    // ========== 悬停信息 ==========
    console.log('\n=== Hover ===')
    const hover = await lsp.hover('/app/src/index.ts', {
      line: 7, // createUser 函数
      character: 10
    })

    if (hover) {
      console.log('Hover info:')
      console.log(hover.contents)
    }

    // ========== 跳转定义 ==========
    console.log('\n=== Definition ===')
    const definitions = await lsp.definition('/app/src/index.ts', {
      line: 14, // createUser 调用
      character: 15
    })

    for (const def of definitions) {
      console.log(`Definition: ${def.uri}:${def.range.start.line}:${def.range.start.character}`)
    }

    // ========== 查找引用 ==========
    console.log('\n=== References ===')
    const references = await lsp.references('/app/src/index.ts', {
      line: 1, // User 接口
      character: 12
    })

    console.log(`Found ${references.length} references to "User"`)

    // ========== 诊断信息 ==========
    console.log('\n=== Diagnostics ===')
    const diagnostics = await lsp.diagnostics('/app/src/index.ts')

    if (diagnostics.length === 0) {
      console.log('No errors or warnings')
    } else {
      for (const diag of diagnostics) {
        console.log(`${diag.severityName} at line ${diag.range.start.line}: ${diag.message}`)
      }
    }

    // 关闭服务器
    await lsp.stop()

  } finally {
    await sandbox.delete()
  }
}

lspExample().catch(console.error)
```

### 6.2 Python 示例

```python
from workspace_sdk import WorkspaceClient, LSPLanguageId

def lsp_example():
    client = WorkspaceClient()

    with client.sandbox.create(template='python:3.11') as sandbox:
        # 创建项目文件
        sandbox.fs.mkdir('/app/src', recursive=True)
        sandbox.fs.write('/app/src/main.py', '''
from dataclasses import dataclass
from typing import List, Optional

@dataclass
class User:
    id: int
    name: str
    email: str

def create_user(name: str, email: str) -> User:
    """Create a new user with auto-generated ID."""
    return User(id=hash(email), name=name, email=email)

def get_users() -> List[User]:
    """Get all users."""
    return []

user = create_user("John", "john@example.com")
print(user.name)
''')

        # 创建 LSP 服务器
        print('Creating LSP server...')
        lsp = sandbox.lsp.create('python', '/app')

        # 打开文档
        lsp.did_open('/app/src/main.py')

        # 文档符号
        print('\n=== Document Symbols ===')
        symbols = lsp.document_symbols('/app/src/main.py')
        for symbol in symbols:
            print(f'{symbol.kind_name}: {symbol.name}')

        # 代码补全
        print('\n=== Completion ===')
        completions = lsp.completion('/app/src/main.py', line=19, character=11)
        print('Completions for "user.":')
        for item in completions.items[:5]:
            print(f'  {item.kind_name}: {item.label}')

        # 悬停信息
        print('\n=== Hover ===')
        hover = lsp.hover('/app/src/main.py', line=11, character=5)
        if hover:
            print(f'Hover info:\n{hover.contents}')

        # 诊断信息
        print('\n=== Diagnostics ===')
        diagnostics = lsp.diagnostics('/app/src/main.py')
        if not diagnostics:
            print('No errors or warnings')
        else:
            for diag in diagnostics:
                print(f'{diag.severity_name} at line {diag.range.start.line}: {diag.message}')

        # 关闭服务器
        lsp.stop()

if __name__ == '__main__':
    lsp_example()
```

---

## 7. 错误处理

### 7.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 6000 | LSP_NOT_FOUND | 404 | LSP 服务器不存在 |
| 6001 | LSP_START_FAILED | 500 | LSP 服务器启动失败 |
| 6002 | LSP_NOT_SUPPORTED | 400 | 不支持的语言 |
| 6003 | LSP_NOT_READY | 503 | LSP 服务器未就绪 |
| 6004 | LSP_REQUEST_FAILED | 500 | LSP 请求失败 |
| 6005 | LSP_DOCUMENT_NOT_OPEN | 400 | 文档未打开 |

### 7.2 错误处理示例

```typescript
import {
  LSPNotFoundError,
  LSPStartFailedError,
  LanguageNotSupportedError
} from '@workspace-sdk/typescript'

async function safeLSP(sandbox: Sandbox, language: string, path: string) {
  try {
    const lsp = await sandbox.lsp.create(language, path)
    return lsp
  } catch (error) {
    if (error instanceof LanguageNotSupportedError) {
      console.error(`Language ${language} is not supported`)
      return null
    }

    if (error instanceof LSPStartFailedError) {
      console.error('LSP server failed to start:', error.message)
      // 可能需要安装依赖
      await sandbox.process.run('npm install', { cwd: path })
      // 重试
      return await sandbox.lsp.create(language, path)
    }

    throw error
  }
}
```

---

## 附录

### A. 语言服务器配置

#### TypeScript/JavaScript

```typescript
{
  settings: {
    'typescript.preferences.importModuleSpecifier': 'relative',
    'typescript.suggest.autoImports': true,
    'typescript.updateImportsOnFileMove.enabled': 'always'
  }
}
```

#### Python

```typescript
{
  settings: {
    'python.analysis.typeCheckingMode': 'basic', // 'off' | 'basic' | 'strict'
    'python.analysis.autoImportCompletions': true,
    'python.analysis.diagnosticMode': 'workspace'
  }
}
```

#### Go

```typescript
{
  settings: {
    'gopls.staticcheck': true,
    'gopls.analyses.unusedparams': true,
    'gopls.usePlaceholders': true
  }
}
```

### B. 符号类型参考

| Kind | 值 | 描述 |
|------|---|------|
| File | 1 | 文件 |
| Module | 2 | 模块 |
| Namespace | 3 | 命名空间 |
| Package | 4 | 包 |
| Class | 5 | 类 |
| Method | 6 | 方法 |
| Property | 7 | 属性 |
| Field | 8 | 字段 |
| Constructor | 9 | 构造函数 |
| Enum | 10 | 枚举 |
| Interface | 11 | 接口 |
| Function | 12 | 函数 |
| Variable | 13 | 变量 |
| Constant | 14 | 常量 |

### C. 资源限制

| 资源 | 限制 |
|------|------|
| 每个 Sandbox 的 LSP 服务器数 | 5 |
| LSP 初始化超时 | 60 秒 |
| LSP 请求超时 | 30 秒 |
| 符号搜索结果数量 | 1000 |
