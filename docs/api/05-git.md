# Git 服务接口文档

Git 服务提供 Sandbox 内的 Git 版本控制操作能力，支持仓库管理、提交、分支、远程同步等完整的 Git 工作流。

---

## 目录

- [1. 概述](#1-概述)
- [2. 数据类型](#2-数据类型)
- [3. GitService 接口](#3-gitservice-接口)
- [4. REST API](#4-rest-api)
- [5. 使用示例](#5-使用示例)
- [6. 错误处理](#6-错误处理)

---

## 1. 概述

### 1.1 功能说明

| 功能类别 | 功能 | 描述 |
|---------|------|------|
| **仓库操作** | clone, init | 克隆远程仓库、初始化本地仓库 |
| **状态查询** | status, log, diff | 查看状态、提交历史、差异 |
| **分支操作** | branches, createBranch, deleteBranch, checkout | 分支管理 |
| **提交操作** | add, commit | 暂存和提交 |
| **远程操作** | push, pull, fetch | 与远程仓库同步 |
| **高级操作** | merge, rebase, stash | 合并、变基、储藏 |

---

## 2. 数据类型

### 2.1 GitStatus

Git 仓库状态。

```typescript
/**
 * Git 仓库状态
 */
interface GitStatus {
  /**
   * 当前分支名称
   */
  branch: string

  /**
   * 远程跟踪分支
   */
  upstream?: string

  /**
   * 领先远程的提交数
   */
  ahead: number

  /**
   * 落后远程的提交数
   */
  behind: number

  /**
   * 分支是否已发布到远程
   */
  published: boolean

  /**
   * 已暂存的文件
   */
  staged: GitFileStatus[]

  /**
   * 未暂存的修改
   */
  unstaged: GitFileStatus[]

  /**
   * 未跟踪的文件
   */
  untracked: string[]

  /**
   * 忽略的文件（如果请求）
   */
  ignored?: string[]

  /**
   * 冲突的文件
   */
  conflicted: string[]

  /**
   * 是否是干净的工作区
   */
  clean: boolean

  /**
   * 是否正在进行 merge
   */
  merging: boolean

  /**
   * 是否正在进行 rebase
   */
  rebasing: boolean

  /**
   * 是否正在进行 cherry-pick
   */
  cherryPicking: boolean
}

/**
 * 文件状态
 */
interface GitFileStatus {
  /**
   * 文件路径
   */
  path: string

  /**
   * 状态码
   * - M: 修改
   * - A: 新增
   * - D: 删除
   * - R: 重命名
   * - C: 复制
   * - U: 未合并
   */
  status: 'M' | 'A' | 'D' | 'R' | 'C' | 'U'

  /**
   * 原路径（重命名/复制时）
   */
  originalPath?: string
}
```

```python
from dataclasses import dataclass
from typing import List, Optional

@dataclass
class GitFileStatus:
    """文件状态"""
    path: str
    status: str  # 'M' | 'A' | 'D' | 'R' | 'C' | 'U'
    original_path: Optional[str] = None

@dataclass
class GitStatus:
    """Git 仓库状态"""
    branch: str
    upstream: Optional[str]
    ahead: int
    behind: int
    published: bool
    staged: List[GitFileStatus]
    unstaged: List[GitFileStatus]
    untracked: List[str]
    conflicted: List[str]
    clean: bool
    merging: bool = False
    rebasing: bool = False
    cherry_picking: bool = False
    ignored: Optional[List[str]] = None
```

### 2.2 CommitInfo

提交信息。

```typescript
/**
 * 提交信息
 */
interface CommitInfo {
  /**
   * 提交 SHA（完整 40 字符）
   */
  sha: string

  /**
   * 短 SHA（7 字符）
   */
  shortSha: string

  /**
   * 提交消息
   */
  message: string

  /**
   * 提交消息摘要（第一行）
   */
  summary: string

  /**
   * 作者名称
   */
  author: string

  /**
   * 作者邮箱
   */
  authorEmail: string

  /**
   * 作者时间
   */
  authorDate: Date

  /**
   * 提交者名称
   */
  committer: string

  /**
   * 提交者邮箱
   */
  committerEmail: string

  /**
   * 提交时间
   */
  commitDate: Date

  /**
   * 父提交 SHA 列表
   */
  parents: string[]

  /**
   * 是否是 merge 提交
   */
  isMerge: boolean
}
```

### 2.3 BranchInfo

分支信息。

```typescript
/**
 * 分支信息
 */
interface BranchInfo {
  /**
   * 分支名称
   */
  name: string

  /**
   * 是否是当前分支
   */
  current: boolean

  /**
   * 是否是远程分支
   */
  remote: boolean

  /**
   * 远程名称（远程分支时）
   */
  remoteName?: string

  /**
   * 跟踪的远程分支
   */
  upstream?: string

  /**
   * 最新提交 SHA
   */
  commit: string

  /**
   * 最新提交消息摘要
   */
  commitSummary: string

  /**
   * 领先上游的提交数
   */
  ahead?: number

  /**
   * 落后上游的提交数
   */
  behind?: number
}
```

### 2.4 CloneOptions

克隆选项。

```typescript
/**
 * 克隆选项
 */
interface CloneOptions {
  /**
   * 目标路径
   */
  path: string

  /**
   * 克隆的分支
   */
  branch?: string

  /**
   * 指定的提交 ID
   */
  commitId?: string

  /**
   * 浅克隆深度
   * - undefined: 完整克隆
   * - 1: 只获取最新提交
   * - n: 获取最近 n 个提交
   */
  depth?: number

  /**
   * 是否递归克隆子模块
   * @default false
   */
  recursive?: boolean

  /**
   * 认证用户名
   */
  username?: string

  /**
   * 认证密码/令牌
   */
  password?: string

  /**
   * SSH 私钥内容
   */
  sshKey?: string

  /**
   * SSH 私钥密码
   */
  sshKeyPassword?: string

  /**
   * 进度回调
   */
  onProgress?: (progress: GitProgress) => void
}

/**
 * Git 操作进度
 */
interface GitProgress {
  /**
   * 当前阶段
   */
  stage: 'counting' | 'compressing' | 'receiving' | 'resolving'

  /**
   * 当前进度 (0-100)
   */
  percent: number

  /**
   * 已处理对象数
   */
  processed: number

  /**
   * 总对象数
   */
  total: number

  /**
   * 传输字节数
   */
  bytesReceived?: number
}
```

### 2.5 CommitOptions

提交选项。

```typescript
/**
 * 提交选项
 */
interface CommitOptions {
  /**
   * 提交消息
   */
  message: string

  /**
   * 作者名称
   */
  author?: string

  /**
   * 作者邮箱
   */
  email?: string

  /**
   * 是否允许空提交
   * @default false
   */
  allowEmpty?: boolean

  /**
   * 是否修改上一个提交
   * @default false
   */
  amend?: boolean

  /**
   * 是否跳过钩子
   * @default false
   */
  noVerify?: boolean

  /**
   * 签名提交（GPG）
   */
  sign?: boolean

  /**
   * 共同作者
   */
  coAuthors?: Array<{ name: string; email: string }>
}
```

### 2.6 DiffOptions

差异选项。

```typescript
/**
 * 差异选项
 */
interface DiffOptions {
  /**
   * 是否只显示暂存的差异
   * @default false
   */
  staged?: boolean

  /**
   * 与指定提交比较
   */
  commit?: string

  /**
   * 比较两个提交
   */
  commits?: [string, string]

  /**
   * 限制的文件路径
   */
  paths?: string[]

  /**
   * 上下文行数
   * @default 3
   */
  context?: number

  /**
   * 是否忽略空白变化
   * @default false
   */
  ignoreWhitespace?: boolean

  /**
   * 输出格式
   * @default 'unified'
   */
  format?: 'unified' | 'stat' | 'name-only' | 'name-status'
}

/**
 * 差异结果
 */
interface DiffResult {
  /**
   * 差异内容
   */
  diff: string

  /**
   * 文件统计
   */
  stats: {
    files: number
    insertions: number
    deletions: number
  }

  /**
   * 变更的文件
   */
  files: Array<{
    path: string
    status: string
    insertions: number
    deletions: number
  }>
}
```

---

## 3. GitService 接口

### 3.1 clone

克隆远程仓库。

```typescript
/**
 * 克隆远程仓库
 *
 * @param url - 仓库 URL
 * @param options - 克隆选项
 * @throws {GitAuthFailedError} 认证失败
 * @throws {GitRepoNotFoundError} 仓库不存在
 * @throws {FileExistsError} 目标路径已存在
 *
 * @example
 * // 基本克隆
 * await sandbox.git.clone('https://github.com/user/repo.git', {
 *   path: '/app/repo'
 * })
 *
 * @example
 * // 浅克隆指定分支
 * await sandbox.git.clone('https://github.com/user/repo.git', {
 *   path: '/app/repo',
 *   branch: 'develop',
 *   depth: 1
 * })
 *
 * @example
 * // 使用令牌认证
 * await sandbox.git.clone('https://github.com/user/private-repo.git', {
 *   path: '/app/repo',
 *   username: 'oauth2',
 *   password: 'ghp_xxxx'
 * })
 *
 * @example
 * // 使用 SSH
 * await sandbox.git.clone('git@github.com:user/repo.git', {
 *   path: '/app/repo',
 *   sshKey: privateKeyContent
 * })
 */
clone(url: string, options: CloneOptions): Promise<void>
```

```python
def clone(
    self,
    url: str,
    path: str,
    *,
    branch: Optional[str] = None,
    commit_id: Optional[str] = None,
    depth: Optional[int] = None,
    recursive: bool = False,
    username: Optional[str] = None,
    password: Optional[str] = None,
    ssh_key: Optional[str] = None,
    on_progress: Optional[Callable[[GitProgress], None]] = None,
    timeout: Optional[float] = None
) -> None:
    """
    克隆远程仓库

    Args:
        url: 仓库 URL
        path: 目标路径
        branch: 克隆的分支
        commit_id: 指定的提交 ID
        depth: 浅克隆深度
        recursive: 是否递归克隆子模块
        username: 认证用户名
        password: 认证密码/令牌
        ssh_key: SSH 私钥内容
        on_progress: 进度回调
        timeout: 超时时间

    Raises:
        GitAuthFailedError: 认证失败
        GitRepoNotFoundError: 仓库不存在
        FileExistsError: 目标路径已存在
    """
```

### 3.2 init

初始化 Git 仓库。

```typescript
/**
 * 初始化 Git 仓库
 *
 * @param path - 仓库路径
 * @param options - 初始化选项
 *
 * @example
 * await sandbox.git.init('/app/new-project')
 *
 * @example
 * // 初始化裸仓库
 * await sandbox.git.init('/app/repo.git', { bare: true })
 */
init(
  path: string,
  options?: {
    /**
     * 是否创建裸仓库
     * @default false
     */
    bare?: boolean

    /**
     * 初始分支名称
     * @default "main"
     */
    initialBranch?: string

    /**
     * 模板目录
     */
    templateDir?: string
  }
): Promise<void>
```

### 3.3 status

获取仓库状态。

```typescript
/**
 * 获取仓库状态
 *
 * @param path - 仓库路径
 * @param options - 状态选项
 * @returns Git 状态
 * @throws {GitRepoNotFoundError} 不是 Git 仓库
 *
 * @example
 * const status = await sandbox.git.status('/app/repo')
 * if (!status.clean) {
 *   console.log('Staged:', status.staged.length)
 *   console.log('Unstaged:', status.unstaged.length)
 *   console.log('Untracked:', status.untracked.length)
 * }
 */
status(
  path: string,
  options?: {
    /**
     * 是否包含忽略的文件
     * @default false
     */
    includeIgnored?: boolean
  }
): Promise<GitStatus>
```

### 3.4 log

获取提交历史。

```typescript
/**
 * 获取提交历史
 *
 * @param path - 仓库路径
 * @param options - 日志选项
 * @returns 提交信息列表
 *
 * @example
 * // 获取最近 10 个提交
 * const commits = await sandbox.git.log('/app/repo', { limit: 10 })
 *
 * @example
 * // 获取指定文件的历史
 * const commits = await sandbox.git.log('/app/repo', {
 *   paths: ['src/index.ts'],
 *   limit: 20
 * })
 *
 * @example
 * // 获取指定作者的提交
 * const commits = await sandbox.git.log('/app/repo', {
 *   author: 'john@example.com'
 * })
 */
log(
  path: string,
  options?: {
    /**
     * 返回数量限制
     * @default 100
     */
    limit?: number

    /**
     * 跳过数量
     */
    skip?: number

    /**
     * 限制到指定分支/提交
     */
    ref?: string

    /**
     * 限制到指定路径
     */
    paths?: string[]

    /**
     * 按作者过滤
     */
    author?: string

    /**
     * 按消息过滤（正则）
     */
    grep?: string

    /**
     * 开始时间
     */
    since?: Date

    /**
     * 结束时间
     */
    until?: Date

    /**
     * 是否只返回第一父提交（简化 merge 历史）
     * @default false
     */
    firstParent?: boolean
  }
): Promise<CommitInfo[]>
```

### 3.5 diff

获取差异。

```typescript
/**
 * 获取差异
 *
 * @param path - 仓库路径
 * @param options - 差异选项
 * @returns 差异结果
 *
 * @example
 * // 工作区与暂存区的差异
 * const diff = await sandbox.git.diff('/app/repo')
 *
 * @example
 * // 暂存区与 HEAD 的差异
 * const diff = await sandbox.git.diff('/app/repo', { staged: true })
 *
 * @example
 * // 与指定提交的差异
 * const diff = await sandbox.git.diff('/app/repo', { commit: 'HEAD~5' })
 *
 * @example
 * // 两个提交之间的差异
 * const diff = await sandbox.git.diff('/app/repo', {
 *   commits: ['main', 'feature-branch']
 * })
 */
diff(path: string, options?: DiffOptions): Promise<DiffResult>
```

### 3.6 branches

列出分支。

```typescript
/**
 * 列出分支
 *
 * @param path - 仓库路径
 * @param options - 选项
 * @returns 分支信息列表
 *
 * @example
 * const branches = await sandbox.git.branches('/app/repo')
 * const current = branches.find(b => b.current)
 *
 * @example
 * // 包含远程分支
 * const branches = await sandbox.git.branches('/app/repo', {
 *   includeRemote: true
 * })
 */
branches(
  path: string,
  options?: {
    /**
     * 是否包含远程分支
     * @default false
     */
    includeRemote?: boolean

    /**
     * 是否只返回远程分支
     * @default false
     */
    remoteOnly?: boolean

    /**
     * 是否包含 ahead/behind 信息
     * @default true
     */
    includeTracking?: boolean
  }
): Promise<BranchInfo[]>
```

### 3.7 createBranch

创建分支。

```typescript
/**
 * 创建分支
 *
 * @param path - 仓库路径
 * @param name - 分支名称
 * @param options - 选项
 * @throws {BranchExistsError} 分支已存在
 *
 * @example
 * // 从当前 HEAD 创建
 * await sandbox.git.createBranch('/app/repo', 'feature-new')
 *
 * @example
 * // 从指定提交创建并切换
 * await sandbox.git.createBranch('/app/repo', 'hotfix', {
 *   startPoint: 'v1.0.0',
 *   checkout: true
 * })
 */
createBranch(
  path: string,
  name: string,
  options?: {
    /**
     * 起始点（提交/分支/标签）
     * @default "HEAD"
     */
    startPoint?: string

    /**
     * 是否切换到新分支
     * @default false
     */
    checkout?: boolean

    /**
     * 是否设置上游跟踪
     */
    upstream?: string

    /**
     * 如果分支存在是否强制创建（重置）
     * @default false
     */
    force?: boolean
  }
): Promise<void>
```

### 3.8 deleteBranch

删除分支。

```typescript
/**
 * 删除分支
 *
 * @param path - 仓库路径
 * @param name - 分支名称
 * @param options - 选项
 * @throws {BranchNotFoundError} 分支不存在
 * @throws {BranchNotMergedError} 分支未合并（需要 force）
 *
 * @example
 * await sandbox.git.deleteBranch('/app/repo', 'old-feature')
 *
 * @example
 * // 强制删除
 * await sandbox.git.deleteBranch('/app/repo', 'unmerged-branch', {
 *   force: true
 * })
 *
 * @example
 * // 删除远程分支
 * await sandbox.git.deleteBranch('/app/repo', 'feature', {
 *   remote: 'origin'
 * })
 */
deleteBranch(
  path: string,
  name: string,
  options?: {
    /**
     * 是否强制删除
     * @default false
     */
    force?: boolean

    /**
     * 同时删除远程分支
     */
    remote?: string
  }
): Promise<void>
```

### 3.9 checkout

切换分支或恢复文件。

```typescript
/**
 * 切换分支或恢复文件
 *
 * @param path - 仓库路径
 * @param target - 目标分支/提交/文件
 * @param options - 选项
 * @throws {BranchNotFoundError} 分支不存在
 * @throws {CheckoutConflictError} 存在冲突
 *
 * @example
 * // 切换分支
 * await sandbox.git.checkout('/app/repo', 'develop')
 *
 * @example
 * // 切换到提交（分离 HEAD）
 * await sandbox.git.checkout('/app/repo', 'abc1234')
 *
 * @example
 * // 恢复文件
 * await sandbox.git.checkout('/app/repo', 'HEAD', {
 *   paths: ['src/index.ts']
 * })
 *
 * @example
 * // 创建并切换
 * await sandbox.git.checkout('/app/repo', 'new-branch', {
 *   create: true
 * })
 */
checkout(
  path: string,
  target: string,
  options?: {
    /**
     * 是否创建新分支
     * @default false
     */
    create?: boolean

    /**
     * 限制恢复的文件路径
     */
    paths?: string[]

    /**
     * 是否强制切换（丢弃本地修改）
     * @default false
     */
    force?: boolean
  }
): Promise<void>
```

### 3.10 add

添加文件到暂存区。

```typescript
/**
 * 添加文件到暂存区
 *
 * @param path - 仓库路径
 * @param files - 文件列表
 * @param options - 选项
 *
 * @example
 * // 添加指定文件
 * await sandbox.git.add('/app/repo', ['src/index.ts', 'package.json'])
 *
 * @example
 * // 添加所有文件
 * await sandbox.git.add('/app/repo', ['.'])
 *
 * @example
 * // 添加所有修改（不包括新文件）
 * await sandbox.git.add('/app/repo', ['.'], { update: true })
 */
add(
  path: string,
  files: string[],
  options?: {
    /**
     * 只添加已跟踪文件的修改
     * @default false
     */
    update?: boolean

    /**
     * 是否包含忽略的文件
     * @default false
     */
    force?: boolean

    /**
     * 交互式添加（按 hunk）
     * @default false
     */
    patch?: boolean
  }
): Promise<void>
```

### 3.11 commit

创建提交。

```typescript
/**
 * 创建提交
 *
 * @param path - 仓库路径
 * @param options - 提交选项
 * @returns 提交信息
 * @throws {NothingToCommitError} 没有可提交的内容
 *
 * @example
 * const commit = await sandbox.git.commit('/app/repo', {
 *   message: 'Add new feature'
 * })
 * console.log(`Created commit: ${commit.sha}`)
 *
 * @example
 * // 指定作者
 * await sandbox.git.commit('/app/repo', {
 *   message: 'Fix bug',
 *   author: 'John Doe',
 *   email: 'john@example.com'
 * })
 *
 * @example
 * // 修改上一个提交
 * await sandbox.git.commit('/app/repo', {
 *   message: 'Updated message',
 *   amend: true
 * })
 */
commit(path: string, options: CommitOptions): Promise<CommitInfo>
```

### 3.12 push

推送到远程仓库。

```typescript
/**
 * 推送到远程仓库
 *
 * @param path - 仓库路径
 * @param options - 推送选项
 * @throws {GitAuthFailedError} 认证失败
 * @throws {PushRejectedError} 推送被拒绝（需要先 pull）
 *
 * @example
 * await sandbox.git.push('/app/repo')
 *
 * @example
 * // 推送到指定远程和分支
 * await sandbox.git.push('/app/repo', {
 *   remote: 'origin',
 *   branch: 'main'
 * })
 *
 * @example
 * // 强制推送
 * await sandbox.git.push('/app/repo', { force: true })
 *
 * @example
 * // 使用令牌认证
 * await sandbox.git.push('/app/repo', {
 *   username: 'oauth2',
 *   password: 'ghp_xxxx'
 * })
 */
push(
  path: string,
  options?: {
    /**
     * 远程名称
     * @default "origin"
     */
    remote?: string

    /**
     * 分支名称
     */
    branch?: string

    /**
     * 是否强制推送
     * @default false
     */
    force?: boolean

    /**
     * 是否设置上游跟踪
     * @default false
     */
    setUpstream?: boolean

    /**
     * 推送所有分支
     * @default false
     */
    all?: boolean

    /**
     * 推送所有标签
     * @default false
     */
    tags?: boolean

    /**
     * 认证用户名
     */
    username?: string

    /**
     * 认证密码/令牌
     */
    password?: string
  }
): Promise<void>
```

### 3.13 pull

从远程拉取。

```typescript
/**
 * 从远程拉取
 *
 * @param path - 仓库路径
 * @param options - 拉取选项
 * @throws {GitAuthFailedError} 认证失败
 * @throws {MergeConflictError} 存在合并冲突
 *
 * @example
 * await sandbox.git.pull('/app/repo')
 *
 * @example
 * // 拉取并变基
 * await sandbox.git.pull('/app/repo', { rebase: true })
 */
pull(
  path: string,
  options?: {
    /**
     * 远程名称
     * @default "origin"
     */
    remote?: string

    /**
     * 分支名称
     */
    branch?: string

    /**
     * 是否使用 rebase 而非 merge
     * @default false
     */
    rebase?: boolean

    /**
     * 是否只快进
     * @default false
     */
    ffOnly?: boolean

    /**
     * 认证用户名
     */
    username?: string

    /**
     * 认证密码/令牌
     */
    password?: string

    /**
     * 进度回调
     */
    onProgress?: (progress: GitProgress) => void
  }
): Promise<void>
```

### 3.14 fetch

从远程获取。

```typescript
/**
 * 从远程获取（不合并）
 *
 * @param path - 仓库路径
 * @param options - 获取选项
 *
 * @example
 * await sandbox.git.fetch('/app/repo')
 *
 * @example
 * // 获取所有远程
 * await sandbox.git.fetch('/app/repo', { all: true })
 *
 * @example
 * // 清理已删除的远程分支
 * await sandbox.git.fetch('/app/repo', { prune: true })
 */
fetch(
  path: string,
  options?: {
    /**
     * 远程名称
     * @default "origin"
     */
    remote?: string

    /**
     * 是否获取所有远程
     * @default false
     */
    all?: boolean

    /**
     * 是否清理已删除的远程分支
     * @default false
     */
    prune?: boolean

    /**
     * 是否获取标签
     * @default true
     */
    tags?: boolean

    /**
     * 认证用户名
     */
    username?: string

    /**
     * 认证密码/令牌
     */
    password?: string

    /**
     * 进度回调
     */
    onProgress?: (progress: GitProgress) => void
  }
): Promise<void>
```

### 3.15 merge

合并分支。

```typescript
/**
 * 合并分支
 *
 * @param path - 仓库路径
 * @param branch - 要合并的分支
 * @param options - 合并选项
 * @returns 合并后的提交（如果创建）
 * @throws {MergeConflictError} 存在合并冲突
 *
 * @example
 * await sandbox.git.merge('/app/repo', 'feature-branch')
 *
 * @example
 * // 创建合并提交（非快进）
 * await sandbox.git.merge('/app/repo', 'feature', {
 *   noFf: true,
 *   message: 'Merge feature branch'
 * })
 */
merge(
  path: string,
  branch: string,
  options?: {
    /**
     * 是否禁止快进合并
     * @default false
     */
    noFf?: boolean

    /**
     * 是否只允许快进合并
     * @default false
     */
    ffOnly?: boolean

    /**
     * 合并提交消息
     */
    message?: string

    /**
     * 是否自动提交
     * @default true
     */
    commit?: boolean

    /**
     * 合并策略
     */
    strategy?: 'recursive' | 'ours' | 'theirs' | 'octopus'
  }
): Promise<CommitInfo | null>
```

### 3.16 reset

重置 HEAD。

```typescript
/**
 * 重置 HEAD
 *
 * @param path - 仓库路径
 * @param target - 目标提交
 * @param options - 重置选项
 *
 * @example
 * // 软重置（保留修改在暂存区）
 * await sandbox.git.reset('/app/repo', 'HEAD~1', { mode: 'soft' })
 *
 * @example
 * // 混合重置（保留修改在工作区）
 * await sandbox.git.reset('/app/repo', 'HEAD~1', { mode: 'mixed' })
 *
 * @example
 * // 硬重置（丢弃所有修改）
 * await sandbox.git.reset('/app/repo', 'HEAD~1', { mode: 'hard' })
 *
 * @example
 * // 重置特定文件
 * await sandbox.git.reset('/app/repo', 'HEAD', {
 *   paths: ['src/index.ts']
 * })
 */
reset(
  path: string,
  target: string,
  options?: {
    /**
     * 重置模式
     * @default "mixed"
     */
    mode?: 'soft' | 'mixed' | 'hard'

    /**
     * 限制重置的文件
     */
    paths?: string[]
  }
): Promise<void>
```

---

## 4. REST API

### 4.1 端点列表

| 方法 | 端点 | 描述 |
|------|------|------|
| POST | `/api/v1/sandboxes/{id}/git/clone` | 克隆仓库 |
| POST | `/api/v1/sandboxes/{id}/git/init` | 初始化仓库 |
| GET | `/api/v1/sandboxes/{id}/git/status` | 获取状态 |
| GET | `/api/v1/sandboxes/{id}/git/log` | 获取日志 |
| GET | `/api/v1/sandboxes/{id}/git/diff` | 获取差异 |
| GET | `/api/v1/sandboxes/{id}/git/branches` | 列出分支 |
| POST | `/api/v1/sandboxes/{id}/git/branches` | 创建分支 |
| DELETE | `/api/v1/sandboxes/{id}/git/branches/{name}` | 删除分支 |
| POST | `/api/v1/sandboxes/{id}/git/checkout` | 切换分支 |
| POST | `/api/v1/sandboxes/{id}/git/add` | 暂存文件 |
| POST | `/api/v1/sandboxes/{id}/git/commit` | 提交 |
| POST | `/api/v1/sandboxes/{id}/git/push` | 推送 |
| POST | `/api/v1/sandboxes/{id}/git/pull` | 拉取 |
| POST | `/api/v1/sandboxes/{id}/git/fetch` | 获取 |
| POST | `/api/v1/sandboxes/{id}/git/merge` | 合并 |
| POST | `/api/v1/sandboxes/{id}/git/reset` | 重置 |

### 4.2 克隆仓库

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/git/clone
Content-Type: application/json
Authorization: Bearer <token>

{
  "url": "https://github.com/user/repo.git",
  "path": "/app/repo",
  "branch": "main",
  "depth": 1,
  "username": "oauth2",
  "password": "ghp_xxxx"
}
```

**响应** (200 OK):

```json
{
  "path": "/app/repo",
  "branch": "main",
  "commit": "abc1234567890",
  "message": "Initial commit"
}
```

### 4.3 获取状态

**请求**:

```http
GET /api/v1/sandboxes/sbx-abc123/git/status?path=/app/repo
Authorization: Bearer <token>
```

**响应** (200 OK):

```json
{
  "branch": "main",
  "upstream": "origin/main",
  "ahead": 2,
  "behind": 0,
  "published": true,
  "staged": [
    {"path": "src/index.ts", "status": "M"}
  ],
  "unstaged": [
    {"path": "README.md", "status": "M"}
  ],
  "untracked": ["new-file.txt"],
  "conflicted": [],
  "clean": false,
  "merging": false,
  "rebasing": false
}
```

### 4.4 提交

**请求**:

```http
POST /api/v1/sandboxes/sbx-abc123/git/commit
Content-Type: application/json
Authorization: Bearer <token>

{
  "path": "/app/repo",
  "message": "Add new feature\n\nDetailed description here",
  "author": "John Doe",
  "email": "john@example.com"
}
```

**响应** (200 OK):

```json
{
  "sha": "abc1234567890def1234567890abc1234567890ab",
  "shortSha": "abc1234",
  "message": "Add new feature\n\nDetailed description here",
  "summary": "Add new feature",
  "author": "John Doe",
  "authorEmail": "john@example.com",
  "authorDate": "2024-01-15T10:30:00Z",
  "committer": "John Doe",
  "committerEmail": "john@example.com",
  "commitDate": "2024-01-15T10:30:00Z",
  "parents": ["def4567890abc1234567890def4567890abc1234"],
  "isMerge": false
}
```

---

## 5. 使用示例

### 5.1 TypeScript 示例

```typescript
import { WorkspaceClient } from '@workspace-sdk/typescript'

async function gitWorkflow() {
  const client = new WorkspaceClient({ apiUrl: 'https://api.example.com' })
  const sandbox = await client.sandbox.create({ template: 'node:20' })

  const repoPath = '/app/repo'

  try {
    // ========== 克隆仓库 ==========
    console.log('Cloning repository...')
    await sandbox.git.clone('https://github.com/user/repo.git', {
      path: repoPath,
      depth: 1
    })

    // ========== 查看状态 ==========
    const status = await sandbox.git.status(repoPath)
    console.log(`Branch: ${status.branch}`)
    console.log(`Clean: ${status.clean}`)

    // ========== 创建新分支 ==========
    console.log('\nCreating feature branch...')
    await sandbox.git.createBranch(repoPath, 'feature-new', {
      checkout: true
    })

    // ========== 修改文件 ==========
    await sandbox.fs.write(`${repoPath}/src/new-feature.ts`, `
export function newFeature() {
  return 'Hello, World!'
}
`)

    // ========== 暂存和提交 ==========
    console.log('\nCommitting changes...')
    await sandbox.git.add(repoPath, ['src/new-feature.ts'])

    const commit = await sandbox.git.commit(repoPath, {
      message: 'Add new feature',
      author: 'AI Assistant',
      email: 'ai@example.com'
    })
    console.log(`Created commit: ${commit.shortSha}`)

    // ========== 查看差异 ==========
    const diff = await sandbox.git.diff(repoPath, {
      commits: ['main', 'feature-new']
    })
    console.log(`Changes: +${diff.stats.insertions} -${diff.stats.deletions}`)

    // ========== 查看日志 ==========
    const logs = await sandbox.git.log(repoPath, { limit: 5 })
    console.log('\nRecent commits:')
    for (const log of logs) {
      console.log(`  ${log.shortSha} ${log.summary}`)
    }

    // ========== 推送（需要认证） ==========
    // await sandbox.git.push(repoPath, {
    //   username: 'oauth2',
    //   password: process.env.GITHUB_TOKEN
    // })

  } finally {
    await sandbox.delete()
  }
}

gitWorkflow().catch(console.error)
```

### 5.2 Python 示例

```python
from workspace_sdk import WorkspaceClient

def git_workflow():
    client = WorkspaceClient()

    with client.sandbox.create(template='node:20') as sandbox:
        repo_path = '/app/repo'

        # 克隆仓库
        print('Cloning repository...')
        sandbox.git.clone(
            'https://github.com/user/repo.git',
            repo_path,
            depth=1
        )

        # 查看状态
        status = sandbox.git.status(repo_path)
        print(f'Branch: {status.branch}')
        print(f'Clean: {status.clean}')

        # 创建新分支
        print('\nCreating feature branch...')
        sandbox.git.create_branch(repo_path, 'feature-new', checkout=True)

        # 修改文件
        sandbox.fs.write(f'{repo_path}/src/new-feature.ts', '''
export function newFeature() {
  return 'Hello, World!'
}
''')

        # 暂存和提交
        print('\nCommitting changes...')
        sandbox.git.add(repo_path, ['src/new-feature.ts'])

        commit = sandbox.git.commit(repo_path, message='Add new feature')
        print(f'Created commit: {commit.short_sha}')

        # 查看日志
        logs = sandbox.git.log(repo_path, limit=5)
        print('\nRecent commits:')
        for log in logs:
            print(f'  {log.short_sha} {log.summary}')

if __name__ == '__main__':
    git_workflow()
```

---

## 6. 错误处理

### 6.1 错误码

| 错误码 | 名称 | HTTP 状态码 | 描述 |
|-------|------|------------|------|
| 5000 | GIT_ERROR | 500 | Git 操作��误 |
| 5001 | GIT_AUTH_FAILED | 401 | 认证失败 |
| 5002 | GIT_CLONE_FAILED | 500 | 克隆失败 |
| 5003 | GIT_PUSH_FAILED | 500 | 推送失败 |
| 5004 | GIT_MERGE_CONFLICT | 409 | 合并冲突 |
| 5005 | GIT_REPO_NOT_FOUND | 404 | 仓库不存在 |
| 5006 | GIT_BRANCH_NOT_FOUND | 404 | 分支不存在 |
| 5007 | GIT_BRANCH_EXISTS | 409 | 分支已存在 |
| 5008 | GIT_NOTHING_TO_COMMIT | 400 | 没有可提交的内容 |
| 5009 | GIT_PUSH_REJECTED | 409 | 推送被拒绝 |
| 5010 | GIT_CHECKOUT_CONFLICT | 409 | 切换冲突 |

### 6.2 错误处理示例

```typescript
import {
  GitAuthFailedError,
  MergeConflictError,
  NothingToCommitError
} from '@workspace-sdk/typescript'

async function safeGitOps(sandbox: Sandbox, repoPath: string) {
  try {
    await sandbox.git.pull(repoPath)
    await sandbox.git.commit(repoPath, { message: 'Auto commit' })
    await sandbox.git.push(repoPath)

  } catch (error) {
    if (error instanceof GitAuthFailedError) {
      console.error('Authentication failed. Check credentials.')
      return
    }

    if (error instanceof MergeConflictError) {
      console.error('Merge conflict detected!')
      const status = await sandbox.git.status(repoPath)
      console.log('Conflicted files:', status.conflicted)
      // 处理冲突...
      return
    }

    if (error instanceof NothingToCommitError) {
      console.log('Nothing to commit, working tree clean')
      return
    }

    throw error
  }
}
```

---

## 附录

### A. 认证方式

| 方式 | URL 格式 | 参数 |
|------|---------|------|
| HTTPS 用户名/密码 | `https://github.com/user/repo.git` | username, password |
| HTTPS 令牌 | `https://github.com/user/repo.git` | username: "oauth2", password: token |
| SSH | `git@github.com:user/repo.git` | sshKey, sshKeyPassword |

### B. 常用 Git 配置

```typescript
// 设置 Git 配置
await sandbox.process.run('git config user.name "AI Assistant"', {
  cwd: repoPath
})
await sandbox.process.run('git config user.email "ai@example.com"', {
  cwd: repoPath
})
```

### C. 限制

| 资源 | 限制 |
|------|------|
| 仓库大小 | 10 GB |
| 单文件大小 | 100 MB |
| 克隆超时 | 30 分钟 |
| 推送/拉取超时 | 10 分钟 |
