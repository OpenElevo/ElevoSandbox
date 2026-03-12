# Namespace 管理与 Admin 后台设计

日期：2026-03-11（修订 v7 — 技术评审修订）

## 概述

为 Elevo Workspace Server 引入 Namespace + Share 架构、多租户认证和 Admin 管理后台。

核心理念：每个租户拥有一个独立的 **Namespace**（完整的存储空间），租户可以将 Namespace 内的任意子目录 **Share**（共享）给其他租户，类似 Windows 共享文件夹。Sandbox 在 Namespace 的指定子路径下运行，并可额外挂载 Share。

同时将数据库从 SQLite 迁移到 PostgreSQL，获得原生 UUID、TIMESTAMPTZ、JSONB 类型支持，以及完整的外键约束、更好的并发性能和未来水平扩展能力。

**数据迁移说明：** 当前处于早期开发阶段，无需保留的生产数据，本次为全量重建，不提供 SQLite → PostgreSQL 数据迁移脚本。

## 分阶段交付计划

本设计涉及数据库迁移、Namespace/Share 模型、权限系统、Admin 前端、审计日志等多个重大变更。为降低风险、支持增量验证和独立回滚，分三个阶段交付：

### Phase 1：数据库迁移（SQLite → PostgreSQL）

**目标：** 完成底层数据库切换，不改变任何业务逻辑和对外 API 行为。

**范围：**
- 新增 PostgreSQL Docker Compose 服务
- 创建全新 PostgreSQL 初始化迁移（仅已有表的类型升级：TEXT → UUID/TIMESTAMPTZ/JSONB）
- Rust 代码：`SqlitePool` → `PgPool`，参数绑定 `?` → `$1`，移除 PRAGMA WAL，时间函数适配
- `workspace_leases` 表从代码动态建表移入迁移文件
- 环境变量 `WORKSPACE_DATABASE_URL` 默认值更新
- 所有现有集成测试通过

**交付标准：** 所有现有 HTTP/gRPC API 行为与迁移前完全一致，测试无回归。

### Phase 2a：Tenant + Namespace + 认证基础

**目标：** 建立多租户基础架构和认证体系。

**范围：**
- 新增 `tenants`（含 Namespace 存储配置）、`api_keys` 表
- `sandboxes` 表重构：`workspace_id` → `namespace_id` + `root_path`
- `workspace_leases` → `namespace_leases`
- Namespace 物理目录管理（创建/删除租户时同步管理 `namespaces/<tenant_id>/`）
- StorageRouter 从 workspace_id 路由改为 namespace_id 路由
- Axum 认证中间件（Admin JWT + 租户 API Key 双路径）
- Tonic gRPC 拦截器
- 租户管理、API Key 管理 REST API
- Namespace 文件操作 API
- 租户自助接口（`/me`）
- ClientStorageService 认证升级，作用于 Namespace 级别
- 开发模式（无 `ADMIN_PASSWORD` 时关闭认证）
- 提供 CLI 管理脚本（`scripts/admin-cli.sh`），方便测试验证租户和 API Key 的 CRUD
- **Proto 文件变更**：`sandbox.proto` 中 `Sandbox` 消息的 `workspace_id` → `namespace_id` + `root_path` + `mounts`；`workspace.proto` → `namespace.proto`（或保留文件名仅修改内容）；新增 Share 相关消息定义留待 Phase 2b

**交付标准：** 租户/Namespace/API Key/认证端到端可用，现有 Sandbox 功能在新 Namespace 架构下正常工作。

### Phase 2b：Share + 权限 + Sandbox 挂载 + SDK

**目标：** 引入 Share/权限模型，完成 Sandbox 挂载重构和 SDK 适配。

**范围：**
- 新增 `shares`、`share_permissions`、`sandbox_mounts` 表
- 权限检查逻辑（基于 Share）
- Share 管理、权限管理 REST API（含 Share 所有者自管理权限）
- Share 文件操作 API
- FileSystemService 认证升级，支持 Namespace 挂载和 Share 挂载两种模式
- Sandbox 创建时的 Share 挂载逻辑
- SDK 更新（新实体模型 + `api_key` 参数）

**交付标准：** Share/权限端到端可用，Sandbox 可挂载 Share，SDK 兼容。

### Phase 3：Admin 前端 + 审计日志

**目标：** 提供 Web 管理界面和操作审计能力。

**范围：**
- 新增 `audit_logs` 表
- 审计日志写入逻辑（嵌入关键业务操作路径）
- 审计日志查询 API
- Admin 前端项目搭建（React + Ant Design + Vite）
- 前端所有页面：登录、仪表盘、租户管理（含 Namespace 浏览）、Share 管理、Sandbox 管理、审计日志
- 前端构建产物嵌入 Rust 服务静态文件

**交付标准：** Admin 可通过 Web 界面完成所有管理操作，关键操作有审计记录。

### 阶段依赖关系

```
Phase 1 → Phase 2a → Phase 2b → Phase 3
```

每个阶段独立可验证、可回滚。Phase 2a 完成后可通过 CLI 脚本验证租户和认证功能，无需等待前端。

## 核心概念

### 概念模型

```
Tenant (租户)
  └── Namespace (1:1，租户的整个存储空间)
        ├── /projects/app1/                    ← 可以在这里跑 Sandbox
        ├── /projects/shared-lib/  ──┐
        └── /data/                   │ 共享
                                     │
Share (共享目录)                      │
  ├── "shared-lib" ←────────────────┘
  │   source_path: /projects/shared-lib
  │   visibility: private
  │   permissions: [{tenant_b, read}]
  └── "public-data"
      source_path: /data
      visibility: public

Sandbox (隔离容器)
  ├── namespace_id: <tenant_a>
  ├── root_path: /projects/app1
  └── mounts: [{share_id, /ext/shared-lib}]
```

### 术语定义

| 概念 | 定义 | 类比 |
|------|------|------|
| **Tenant** | 一个使用服务的组织/团队/账户，拥有一个 Namespace | Windows 用户账户 |
| **Namespace** | 租户的完整存储空间，与租户 1:1 绑定，创建租户时自动创建 | 一台电脑的整个磁盘 |
| **Share** | 租户将自己 Namespace 内的某个子目录共享给其他租户 | `\\computer\sharename`（Windows 共享文件夹） |
| **Sandbox** | 在 Namespace 的某个子路径下运行的隔离容器，可额外挂载 Share | 在某个目录下运行的进程 |
| **API Key** | 归属于租户的认证凭证 | 访问令牌 |

### 关键规则

1. 每个租户有且仅有一个 Namespace，创建租户时自动创建物理目录
2. Share 只能共享 Namespace 内已存在的目录
3. 同一 Namespace 内同一路径只能创建一个 Share（唯一约束）
4. Sandbox 归属于租户的 Namespace，以 `root_path` 指定工作根目录
5. Sandbox 可以额外挂载 Share（自己的或别人授权的），通过 `sandbox_mounts` 声明
6. Admin 无 Namespace，纯管理角色，可管理所有租户的资源

### API Key

API Key 是归属于租户的凭证。一个租户可拥有多个 Key（如 CI 服务一个、IDE 插件一个）。Key 是认证手段，权限绑定在**租户**而非单个 Key 上。

### 可见性（Visibility）

Share 可见性控制可发现性和默认访问权限：
- **private（私有）**：仅被显式授权的租户能看到和访问
- **public（公开）**：所有活跃租户均可在列表中看到，并拥有隐式 `read` 权限（包括读取文件、列出目录、查看关联 Sandbox）。`write`/`execute`/`admin` 仍需显式授权

## 数据模型

### `tenants` 表（承担 Namespace 角色）

Namespace 与 Tenant 1:1，不单独建表：

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK DEFAULT gen_random_uuid() | 主键，同时也是 namespace_id |
| name | VARCHAR(255) NOT NULL | 可读名称（如"Team Alpha"），大小写不敏感唯一（通过函数索引 `lower(name)` 实现） |
| description | TEXT NOT NULL DEFAULT '' | 用途描述 |
| is_active | BOOLEAN NOT NULL DEFAULT true | 是否启用 |
| storage_type | VARCHAR(16) NOT NULL DEFAULT 'managed' | `managed` 或 `remote`（整个 Namespace 的存储后端） |
| storage_config | JSONB NOT NULL DEFAULT '{}' | 远程存储配置（仅 `remote` 类型使用） |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT now() | 创建时间 |
| updated_at | TIMESTAMPTZ NOT NULL DEFAULT now() | 更新时间 |

**Namespace 生命周期：**
- 创建租户时，自动创建物理目录 `<workspace_dir>/namespaces/<tenant_id>/`
- 删除租户时的物理目录处理策略：
  1. 删除 API 调用后，先将租户标记为 `is_active = false`（停用）
  2. 数据库记录（租户、API Key、Share、权限）级联删除
  3. 物理目录重命名为 `<workspace_dir>/namespaces/.trash/<tenant_id>_<timestamp>/`（软删除）
  4. 后台定时任务在 7 天后清理 `.trash` 目录中的过期数据。通过 `tokio::spawn` 在服务启动时启动清理任务，使用 `tokio::time::interval` 每小时扫描一次 `.trash` 目录，删除超过保留期的子目录
  5. 可通过环境变量 `NAMESPACE_TRASH_RETENTION_DAYS`（默认 7）配置保留天数
- `storage_type`/`storage_config` 决定整个 Namespace 的存储后端，所有 Share 和 Sandbox 继承此配置

### `api_keys` 表

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK DEFAULT gen_random_uuid() | 主键 |
| tenant_id | UUID FK → tenants(id) ON DELETE CASCADE NOT NULL | 所属租户 |
| name | VARCHAR(255) NOT NULL | Key 名称（如"CI Service"、"IDE Plugin"） |
| token_hash | VARCHAR(64) UNIQUE NOT NULL | API Token 的 SHA-256 哈希（hex 编码，固定 64 字符） |
| token_prefix | VARCHAR(16) NOT NULL | 前 8 位用于展示（如"sk_a1b2..."） |
| is_active | BOOLEAN NOT NULL DEFAULT true | 是否启用 |
| expires_at | TIMESTAMPTZ | 过期时间，NULL 表示永不过期 |
| last_used_at | TIMESTAMPTZ | 最后一次认证使用时间 |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT now() | 创建时间 |

约束：
- `UNIQUE(tenant_id, name)` — 同一租户下 Key 名称唯一

**Token 生命周期：**
- 格式：`sk_{32位随机字符}`（总长度 35 字符）
- `token_prefix` 固定存储 Token 前 12 字符（含 `sk_` 前缀 + 8 字符随机部分 + `...`），用于 Admin 后台展示辨识。示例：`sk_a1b2c3d4...`
- Token 仅在创建时展示一次，数据库只存储 `SHA-256(token)`
- 认证流程：`SHA-256(传入token)` → 通过 `token_hash` 直接查找（O(1)，无需逐条验证）
- 撤销某个 Key 不影响同一租户的其他 Key

**`last_used_at` 更新优化：** 为避免高频认证请求造成数据库写放大，`last_used_at` 采用内存缓存 + 定时刷盘策略：在内存中记录每个 API Key 的最后使用时间，仅当距上次数据库更新超过 60 秒时才写入。服务优雅关闭时 flush 所有待更新记录。

**为什么用 SHA-256 而非 Argon2：** API Key 是高熵随机值（32 字符 ≈ 192 位），无论哈希速度如何都无法暴力破解。SHA-256 支持直接通过哈希值查表，认证性能为 O(1)。Argon2 专为低熵密码设计，用在高熵 API Key 上是不必要的性能浪费。

**过期机制：** 当 `expires_at` 已设置且当前时间超过该值时，该 Key 视为失效。Admin 后台展示过期状态并支持 Key 轮换。

### `shares` 表

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK DEFAULT gen_random_uuid() | 主键 |
| owner_tenant_id | UUID NOT NULL FK → tenants(id) ON DELETE RESTRICT | 共享来源租户（Namespace 所有者），RESTRICT 确保有 Share 时不可删除租户 |
| name | VARCHAR(255) NOT NULL | 共享名称（如 "shared-lib"） |
| source_path | TEXT NOT NULL | Namespace 内的路径（如 `/projects/shared-lib`） |
| description | TEXT NOT NULL DEFAULT '' | 描述 |
| visibility | VARCHAR(16) NOT NULL DEFAULT 'private' | `public` 或 `private` |
| metadata | JSONB NOT NULL DEFAULT '{}' | 扩展元数据 |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT now() | |
| updated_at | TIMESTAMPTZ NOT NULL DEFAULT now() | |

约束：
- `UNIQUE(owner_tenant_id, source_path)` — 同一 Namespace 下同一路径只能共享一次
- `UNIQUE(owner_tenant_id, name)` — 同一租户下 Share 名称唯一

**Share 创建验证：** 创建 Share 时，服务端验证 `source_path` 对应的物理目录在 Namespace 中存在。如不存在，返回 `400 VALIDATION_ERROR`。

### 路径安全校验

所有路径类字段（`source_path`、`root_path`、`mount_path`、文件操作路径）在写入数据库和执行文件操作前必须经过统一的路径规范化处理：

1. **规范化**：解析并消除 `.`、`..`、重复 `/`，转换为规范绝对路径
2. **前缀校验**：确保最终路径仍在预期的 Namespace 目录内（防止路径穿越攻击）
3. **符号链接**：不解析符号链接（`source_path` 可以是符号链接目标），但最终物理路径必须在 Namespace 根目录内
4. **字符限制**：禁止 NULL 字节（`\0`）；仅允许 UTF-8 合法字符

```rust
/// 规范化路径并验证其仍在指定根目录内。
/// 返回规范化后的相对路径（相对于 root）。
fn sanitize_path(root: &Path, user_path: &str) -> Result<PathBuf, ApiError>;

/// Share 文件操作专用：在 sanitize_path 基础上增加 source_path 前缀限制。
/// 确保路径不逃逸 Share 的 source_path 范围。
fn sanitize_share_path(
    namespace_root: &Path,
    source_path: &str,
    user_path: &str,
) -> Result<PathBuf, ApiError>;
```

### `share_permissions` 表

| 字段 | 类型 | 说明 |
|------|------|------|
| tenant_id | UUID NOT NULL FK → tenants(id) ON DELETE CASCADE | 被授权的租户 |
| share_id | UUID NOT NULL FK → shares(id) ON DELETE CASCADE | 被授权的 Share |
| permission | VARCHAR(16) NOT NULL CHECK(IN 'read','write','execute','admin') | 权限级别 |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT now() | 授权时间 |

主键：`(tenant_id, share_id)`

### `sandboxes` 表

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK DEFAULT gen_random_uuid() | 主键 |
| name | VARCHAR(255) | Sandbox 名称 |
| namespace_id | UUID NOT NULL FK → tenants(id) ON DELETE RESTRICT | 所属 Namespace（即 tenant_id，阻止删除有 Sandbox 的租户） |
| root_path | TEXT NOT NULL DEFAULT '/' | Namespace 内的工作根路径 |
| template | VARCHAR(255) NOT NULL | Docker 镜像模板 |
| state | VARCHAR(16) NOT NULL DEFAULT 'starting' | 状态（starting/running/stopping/stopped/error） |
| container_id | VARCHAR(64) | Docker 容器 ID |
| env | JSONB NOT NULL DEFAULT '{}' | 环境变量 |
| metadata | JSONB NOT NULL DEFAULT '{}' | 扩展元数据 |
| timeout | INTEGER NOT NULL DEFAULT 0 | 超时秒数 |
| error_message | TEXT | 错误信息 |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT now() | 创建时间 |
| updated_at | TIMESTAMPTZ NOT NULL DEFAULT now() | 更新时间 |

**与旧 `sandboxes` 表的差异：**
- `workspace_id` → `namespace_id`（FK 指向 `tenants.id`）
- 新增 `root_path`（Namespace 内的子路径作为容器工作根目录）
- 移除 `nfs_url`（由 Namespace 存储配置推导）

### `sandbox_mounts` 表（新增）

| 字段 | 类型 | 说明 |
|------|------|------|
| sandbox_id | UUID NOT NULL FK → sandboxes(id) ON DELETE CASCADE | |
| share_id | UUID NOT NULL FK → shares(id) ON DELETE RESTRICT | 挂载的 Share（阻止删除被活跃 Sandbox 挂载的 Share） |
| mount_path | TEXT NOT NULL | 容器内挂载点（如 `/ext/shared-lib`） |

主键：`(sandbox_id, share_id)`
唯一约束：`UNIQUE(sandbox_id, mount_path)` — 同一 Sandbox 内挂载点不能重复

**挂载点约束：**
- `mount_path` 必须是绝对路径
- 不能与 `/workspace` 冲突（`/workspace` 保留给 root_path 挂载）
- 推荐使用 `/ext/` 前缀（如 `/ext/shared-lib`）

### `processes` 和 `ptys` 表

类型升级：主键和外键从 TEXT 改为 UUID，时间戳从 TEXT 改为 TIMESTAMPTZ。`ptys` 表新增 `updated_at` 字段。业务逻辑不变，仍通过 `sandbox_id` 关联。

### `namespace_leases` 表（原 `workspace_leases` 重命名）

| 字段 | 类型 | 说明 |
|------|------|------|
| namespace_id | UUID PK FK → tenants(id) ON DELETE CASCADE | Namespace ID |
| holder_id | VARCHAR(255) NOT NULL | 锁持有者标识 |
| acquired_at | TIMESTAMPTZ NOT NULL | 获取时间 |
| expires_at | TIMESTAMPTZ NOT NULL | 过期时间 |
| renewed_at | TIMESTAMPTZ NOT NULL | 最后续期时间 |

### `audit_logs` 表

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK DEFAULT gen_random_uuid() | 主键 |
| actor_type | VARCHAR(16) NOT NULL CHECK(IN 'admin', 'tenant') | `admin` 或 `tenant` |
| actor_id | UUID | 租户为 tenant_id；Admin 为 NULL |
| action | VARCHAR(64) NOT NULL | 操作类型（点分格式，见下表） |
| resource_type | VARCHAR(32) NOT NULL | `tenant`、`share`、`api_key`、`permission` |
| resource_id | UUID NOT NULL | 被操作资源的 ID |
| resource_name | VARCHAR(255) NOT NULL DEFAULT '' | 被操作资源名称快照（写入时记录，资源删除后仍可读） |
| detail | JSONB NOT NULL DEFAULT '{}' | 操作详情 |
| ip_address | INET | 客户端 IP 地址 |
| created_at | TIMESTAMPTZ NOT NULL DEFAULT now() | 记录时间 |

**需审计的操作：**

| 操作 | 说明 |
|------|------|
| `tenant.create` | 创建租户（含 Namespace 目录创建） |
| `tenant.update` | 更新租户信息或状态 |
| `tenant.delete` | 删除租户（含 Namespace 清理） |
| `api_key.create` | 创建 API Key（详情包含 Key 名称，不含 Token） |
| `api_key.revoke` | 撤销 API Key |
| `share.create` | 创建 Share |
| `share.update` | 更新 Share 设置（name、description、visibility） |
| `share.delete` | 删除 Share |
| `permission.grant` | 授予租户 Share 权限 |
| `permission.revoke` | 撤销租户 Share 权限 |

**权限操作的审计字段约定：** `permission.grant` / `permission.revoke` 操作中，`resource_type` 为 `permission`，`resource_id` 存储 `share_id`，`resource_name` 存储 Share 名称，`detail` JSONB 中记录 `tenant_id`、`tenant_name`、`permission` 级别。例如：
```json
{
  "tenant_id": "uuid",
  "tenant_name": "Team Beta",
  "permission": "write"
}
```

审计日志仅追加、不删除。Admin 后台提供可筛选的日志查看界面。

### 物理存储布局

```
<workspace_dir>/
├── namespaces/
│   ├── <tenant-a-uuid>/              ← Tenant A 的 Namespace
│   │   ├── projects/
│   │   │   ├── app1/                 ← Sandbox root_path 目标
│   │   │   └── shared-lib/           ← Share "shared-lib" 的 source_path
│   │   └── data/
│   └── <tenant-b-uuid>/              ← Tenant B 的 Namespace
│       └── ...
└── system/                            ← 预留（系统级文件）
```

**Sandbox 容器内视图**（Tenant A 的 Sandbox，root_path=/projects/app1，挂载了 Tenant X 的 Share）：

```
/workspace/              ← bind mount: namespaces/<tenant-a>/projects/app1/
/ext/shared-lib/         ← bind mount: namespaces/<tenant-x>/<share-source-path>/
```

容器内 `/workspace` 是 root_path 对应的目录，`/ext/*` 是显式挂载的 Share。

## 权限模型

### 四级权限

| 级别 | 包含 | 允许的操作 |
|------|------|-----------|
| `read` | — | 查看 Share 信息、读取文件、列出目录、查看关联 Sandbox |
| `write` | `read` | 写入文件、创建/删除目录、创建/删除 Sandbox |
| `execute` | `write` | PTY 终端操作、运行进程、终止进程 |
| `admin` | `execute` | 删除 Share、管理权限、更新 Share 设置 |

层级关系：`admin` > `execute` > `write` > `read`（高级别包含所有低级别权限）。

**设计说明：** 这种层级包含是第一版的有意简化。这意味着不能实现"可写但不可执行终端"的场景。如未来需要细粒度权限组合，可将 permission 字段演进为 JSON 数组。

### 权限规则

1. **Namespace 自身空间**：租户对自己的 Namespace 拥有完全控制权，无需权限检查
2. **私有 Share**：所有访问都需要 `share_permissions` 表中的显式授权
3. **公开 Share**：所有活跃租户拥有隐式 `read` 权限；`write`/`execute`/`admin` 需显式授权
4. **Share 所有者**（`owner_tenant_id`）对自己的 Share 自动拥有 `admin` 权限（不存储在权限表中，代码层面强制执行）
5. **Admin 用户**（通过密码登录）对所有资源拥有无限制访问权
6. **Sandbox 权限检查**：
   - 在自己 Namespace 内创建 Sandbox → 直接允许（无需权限检查）
   - 挂载别人的 Share 到 Sandbox → 检查对该 Share 的权限
   - 操作 Sandbox（进程、PTY）→ 检查 Sandbox 所属 Namespace 的所有权

### 权限检查与资源可见性

| 场景 | 响应 |
|------|------|
| 私有 Share，租户无权限 | `404 Not Found`（隐藏资源存在性） |
| 公开 Share，租户权限不足 | `403 Forbidden`（资源存在性已知） |
| 资源确实不存在 | `404 Not Found` |

此策略防止私有资源的信息泄露。

### 权限检查实现

权限判断逻辑必须集中封装为统一函数：

```rust
/// 检查租户是否对指定 Share 拥有所需权限级别。
/// 权限层级：admin > execute > write > read。
/// 所有者自动拥有 admin 权限，公开 Share 隐含 read 权限。
async fn check_share_permission(
    pool: &PgPool,
    tenant_id: Uuid,
    share_id: Uuid,
    required: PermissionLevel,
) -> Result<(), ApiError>;

/// 检查租户是否为 Namespace 的所有者。
/// 用于 Namespace 内部操作（文件读写、Sandbox 创建）。
fn check_namespace_ownership(
    auth: &AuthContext,
    namespace_id: Uuid,
) -> Result<(), ApiError>;
```

### 租户停用规则

停用租户（`POST /tenants/{id}/deactivate`，设置 `is_active = false`）：
- **立即阻断**该租户的所有 API Key 认证（中间件校验 `tenant.is_active`）
- **不影响**该租户已运行的 Sandbox（容器继续运行，但无法通过 API 操作）
- **不影响**其他租户对该租户 Share 的已有挂载（已运行的 Sandbox 挂载不受影响，但新的挂载请求会因租户停用而被拒绝）
- **不删除**任何数据，启用后恢复正常
- 该租户的 Share 对其他租户不再可见（`is_active` 参与 Share 可见性判断）

### 租户删除规则

`DELETE /api/v1/tenants/{id}?force=true`：

- **阻止删除**：租户有活跃 Share（被其他租户依赖）时，需先删除所有 Share
- **阻止删除**：租户有活跃 Sandbox（running/starting/stopping 状态）时，需先停止
- **自动清理**：删除时在同一事务中自动清理 stopped/error 状态的 Sandbox 记录及其关联的 `sandbox_mounts`（因为 `sandboxes.namespace_id` 使用 `ON DELETE RESTRICT`，不清理这些记录会导致 FK 约束阻止删除）
- **警告确认**：租户拥有活跃 API Key（`is_active = true` 且未过期）时，API 返回 `409 Conflict`（附带活跃 Key 数量信息），要求请求中携带 `?force=true` 查询参数确认
- **允许删除**：无上述阻碍条件（或已确认 force），在同一事务中依序清理：
  1. 删除该租户所有 Share 的权限记录（`share_permissions`）
  2. 删除 stopped/error 状态 Sandbox 的挂载记录（`sandbox_mounts`）
  3. 删除 stopped/error 状态的 Sandbox 记录（`sandboxes`，FK RESTRICT 要求显式清理）
  4. 删除该租户的所有 Share（`shares`，FK RESTRICT 要求显式清理）
  5. 删除 `tenants` 记录（`api_keys` 和 `namespace_leases` 通过 FK CASCADE 自动级联删除）
- 删除后 Namespace 物理目录移入 `.trash`（见上方 Namespace 生命周期）

### Share 删除规则

**删除前置检查：**
- 如果有 `running`、`starting` 或 `stopping` 状态的 Sandbox 正在挂载该 Share（通过 `sandbox_mounts` 关联），**阻止删除**，返回 `409 Conflict`，响应中列出相关 Sandbox 信息
- 仅当所有挂载该 Share 的 Sandbox 均为 `stopped` 或 `error` 状态时，才允许删除

**删除执行顺序（在同一事务中）：**
1. 应用层先 `DELETE FROM sandbox_mounts WHERE share_id = $1`（清理 stopped/error 状态 Sandbox 的挂载记录，因为 `sandbox_mounts.share_id` 使用 `ON DELETE RESTRICT`，不先清理会被 FK 约束阻止）
2. `DELETE FROM shares WHERE id = $1`（触发 `share_permissions` 的 `ON DELETE CASCADE` 自动级联删除权限记录）
3. 物理目录**不删除**（目录仍属于所有者的 Namespace）

## 认证架构

```
HTTP/gRPC 请求
  │
  ├─ Authorization: Bearer <jwt-token>
  │    → JWT 验证 → Admin 身份（完全访问）
  │
  ├─ Authorization: Bearer sk_xxxxx
  │    → SHA-256(token) → 查找 api_keys.token_hash
  │    → 检查 api_key.is_active 且未过期 且 tenant.is_active
  │    → 更新 api_keys.last_used_at
  │    → 解析 tenant_id → 租户身份
  │    → Handler 检查 share_permissions 或 namespace 所有权
  │
  └─ 无 Authorization 头 → 401 Unauthorized
```

### HTTP 中间件（Axum）

Axum 中间件层提取 `Authorization: Bearer <token>`：
- Token 以 `sk_` 开头 → 租户认证路径（SHA-256 哈希 → 查找 api_keys → 关联 tenants）
- 其他 → JWT 验证，Admin 身份
- 注入 `AuthContext` 到请求扩展中：

```rust
enum Identity {
    Admin { session_id: Uuid },
    Tenant { id: Uuid, name: String },
}

struct AuthContext {
    identity: Identity,
    ip_address: Option<IpAddr>,
}
```

Handler 层使用 `AuthContext` 进行权限校验。

**性能：** SHA-256 哈希 + 单次索引 DB 查询对每次请求来说足够快。如有需要，可增加 LRU 缓存（token_hash → tenant_id 映射），设置较短 TTL（如 60 秒），Key 撤销/租户停用时使缓存失效。

### gRPC 认证

所有对外 gRPC 服务使用 Tonic 拦截器，执行相同的双路径认证。

| 服务 | 调用方 | 认证方式 |
|------|--------|---------|
| AgentService | Sandbox Agent → Server | **内部通信**，无需租户认证 |
| ClientStorageService | 客户端存储提供者 → Server | 需要租户 API Key |
| NamespaceService | 外部 → Server | 需要租户/Admin 认证 |
| ShareService | 外部 → Server | 需要租户/Admin 认证 |
| SandboxService | 外部 → Server | 需要租户/Admin 认证 |
| ProcessService | 外部 → Server | 需要租户/Admin 认证 |
| PtyService | 外部 → Server | 需要租户/Admin 认证 |
| FileSystemService | FUSE 客户端 → Server | 需要租户 API Key |

### AgentService 安全边界

AgentService 不执行租户认证，依赖以下信任模型：

- **网络隔离**：Agent 运行在 Server 创建的 Docker 容器内，通过 Docker bridge 网络连接 Server gRPC 端口
- **身份验证**：Agent 在 `Handshake` 消息中携带 `sandbox_id`，Server 验证该 Sandbox 确实存在且处于运行状态
- **操作范围限制**：Agent 只能操作自身所属 Sandbox 的资源（进程、PTY），无法跨 Sandbox 访问

**假设**：gRPC 端口（9090）不直接暴露到外部网络。

### ClientStorageService 认证

ClientStorageService 使用现有 proto 定义中的 `StorageHandshake.token` 字段传递认证凭证：

- 客户端在双向流的首条 `StorageHandshake` 消息中设置 `token = "sk_xxx"`
- 服务端验证 API Key → 解析租户身份 → 确认是该 Namespace 的所有者
- 认证失败时关闭流并返回 gRPC `UNAUTHENTICATED` / `PERMISSION_DENIED` 状态码

**注意：** ClientStorageService 现在作用于 Namespace 级别（而非单个 Workspace），一个连接提供整个 Namespace 的存储后端。

### FileSystemService 认证

全局 `fs_api_token` **替换**为基于租户的 API Key 认证：

1. FUSE 客户端在 gRPC metadata 中携带 `Authorization: Bearer sk_xxx`
2. 服务端验证 API Key → 解析租户身份
3. 两种挂载模式：
   - **Namespace 挂载**：挂载自己的 Namespace → 验证是 Namespace 所有者 → 完全读写
   - **Share 挂载**：挂载别人的 Share → 检查 Share 权限 → 按权限级别控制读写
4. 逐操作权限校验：
   - 读操作（`Stat`、`ReadFile`、`ReadAt`、`ListDir`、`ReadLink`、`StatFs`）：`read`
   - 写操作（`WriteFile`、`WriteAt`、`Create`、`Mkdir`、`RemoveFile`、`RemoveDir`、`Rename`、`SetAttr`、`Symlink`）：`write`

`WORKSPACE_FS_API_TOKEN` 环境变量已移除，不再支持。

### MCP 认证

MCP 端点（`/mcp/*`）受相同认证中间件保护：
- MCP 工具调用访问 Share 或 Namespace 时，经过相同的权限检查
- Bearer Token 从建立 MCP 会话的 HTTP 请求中提取
- 未认证的 MCP 请求返回 401

### Admin 认证

- 通过 `ADMIN_PASSWORD` 环境变量配置（启用 Admin 后台时必填）
- `POST /api/v1/auth/login` 提交 `{ "password": "..." }` 返回 JWT
- **登录接口独立限流**：`POST /api/v1/auth/login` 不受全局速率限制控制，而是使用独立的更严格限制：每个 IP 每分钟最多 10 次尝试，超限返回 `429 Too Many Requests`
- JWT 使用 `JWT_SECRET` 环境变量签名（**必填**，最少 32 字节，必须独立配置，不从 ADMIN_PASSWORD 派生）。启动时校验长度，不满足则拒绝启动并输出明确错误信息
- JWT 过期时间通过 `JWT_EXPIRATION_HOURS` 配置（默认：24 小时）
- JWT payload 中携带 `session_id`（随机 UUID）和 `login_ip`（登录时的客户端 IP）
- **Token 续期（Sliding Window）**：每次认证通过的请求，若 JWT 剩余有效期不足总时长的 1/3，自动在响应头 `X-Refreshed-Token` 中返回新 Token。前端 Axios 响应拦截器检测到此头后自动替换 localStorage 中的 Token，对用户无感。**并发处理**：前端在更新 Token 时需使用互斥标记（如全局 `isRefreshing` 变量），确保多个并发响应同时携带新 Token 时只更新一次，避免竞态

**说明：** 单密码 Admin 认证是本版本的有意简化方案。多管理员支持后续迭代。审计日志通过 JWT 中的 `session_id` 关联同一登录会话的所有操作。

## API 设计

### 通用约定

#### 分页

所有列表接口支持通过查询参数分页：

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `page` | 1 | 页码（从 1 开始） |
| `page_size` | 20 | 每页条数（最大 100） |

分页接口的响应格式：

```json
{
  "items": [],
  "total": 100,
  "page": 1,
  "page_size": 20
}
```

#### 错误响应格式

所有错误响应遵循统一格式：

```json
{
  "error": {
    "code": "PERMISSION_DENIED",
    "message": "You do not have 'write' permission on this share"
  }
}
```

#### 时间格式

- API 请求和响应中的所有时间字段统一使用 **UTC 时区、ISO 8601 格式**（如 `2026-03-11T10:00:00Z`）
- 前端接收 UTC 时间后，转换为浏览器本地时区展示
- 数据库存储使用 `TIMESTAMPTZ`，确保时区信息不丢失

标准错误码：

| HTTP 状态码 | 错误码 | 说明 |
|------------|--------|------|
| 400 | `VALIDATION_ERROR` | 请求体或查询参数无效 |
| 401 | `UNAUTHORIZED` | 缺少或无效的认证凭证 |
| 403 | `PERMISSION_DENIED` | 凭证有效但权限不足 |
| 404 | `NOT_FOUND` | 资源不存在或调用方不可见 |
| 409 | `CONFLICT` | 冲突（如租户名重复、路径已共享） |
| 500 | `INTERNAL_ERROR` | 服务端意外错误 |

#### 响应字段裁剪（租户 vs Admin）

| 接口 | Admin 视角 | 租户视角 |
|------|-----------|---------|
| `GET /shares` | 所有 Share | 有权限的 + 公开的 + 自己的 |
| `GET /shares/{id}` → 权限列表 | 所有租户的权限 | **仅显示调用者自己的权限** |
| `GET /sandboxes` | 所有 Sandbox | 仅自己 Namespace 内的 |
| `GET /tenants` | 所有租户 | **不可访问**（仅 Admin 接口） |
| 审计日志 | 完全访问 | **不可访问**（仅 Admin 接口） |

### 仪表盘聚合接口（仅 Admin）

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/api/v1/dashboard/stats` | Admin | 获取仪表盘统计数据（单次查询） |

**响应体（200 OK）：**

```json
{
  "tenants": { "total": 5, "active": 3 },
  "shares": { "total": 12 },
  "sandboxes": { "running": 28 },
  "api_keys": { "active": 18 }
}
```

后端通过一次 SQL 查询（多个 COUNT 子查询）获取所有统计数据，避免前端发送多个列表请求。

### 认证接口

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| POST | `/api/v1/auth/login` | 无 | Admin 登录 → 返回 JWT |

### 租户管理接口（仅 Admin）

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/api/v1/tenants` | Admin | 列出所有租户（分页，可按 is_active、storage_type 筛选） |
| POST | `/api/v1/tenants` | Admin | 创建租户（自动创建 Namespace 物理目录，可选同时创建 API Key） |
| GET | `/api/v1/tenants/{id}` | Admin | 获取租户详情 |
| PUT | `/api/v1/tenants/{id}` | Admin | 更新租户（name、description） |
| POST | `/api/v1/tenants/{id}/activate` | Admin | 启用租户 |
| POST | `/api/v1/tenants/{id}/deactivate` | Admin | 停用租户（立即阻断所有 API 访问） |
| DELETE | `/api/v1/tenants/{id}?force=true` | Admin | 删除租户（含 Namespace 清理）。有活跃 API Key 时需 `?force=true`，否则返回 409 |

#### `GET /api/v1/tenants` 响应字段补充

租户列表 API 响应中的每个租户对象额外包含聚合字段，以支持前端列表展示：

| 额外字段 | 类型 | 说明 |
|----------|------|------|
| `share_count` | integer | 该租户创建的 Share 数量 |
| `active_api_key_count` | integer | 该租户活跃的 API Key 数量（`is_active = true` 且未过期） |

通过 SQL 子查询或 LEFT JOIN + COUNT 在单次查询中获取，避免 N+1。

#### `POST /api/v1/tenants` 请求与响应

**请求体：**

```json
{
  "name": "Team Alpha",
  "description": "前端开发团队",
  "storage_type": "managed",
  "storage_config": {},
  "initial_api_key": {
    "name": "CI Service",
    "expires_at": "2026-12-31T23:59:59Z"
  }
}
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 1-255 字符，唯一 |
| `description` | 否 | 描述 |
| `storage_type` | 否 | `managed`（默认）或 `remote` |
| `storage_config` | 否 | 远程存储配置，仅 `remote` 时需要 |
| `initial_api_key` | 否 | 可选，同时创建第一个 API Key |
| `initial_api_key.name` | 条件必填 | Key 名称，提供 `initial_api_key` 时必填 |
| `initial_api_key.expires_at` | 否 | 过期时间，不填则永不过期 |

**原子性保证：** 租户创建和 API Key 创建在同一数据库事务中执行。任一步骤失败则整体回滚，不会出现"租户已创建但 Key 创建失败"的中间状态。

**响应体（201 Created）：**

```json
{
  "tenant": {
    "id": "uuid",
    "name": "Team Alpha",
    "description": "前端开发团队",
    "is_active": true,
    "storage_type": "managed",
    "storage_config": {},
    "created_at": "2026-03-11T00:00:00Z",
    "updated_at": "2026-03-11T00:00:00Z"
  },
  "api_key": {
    "id": "uuid",
    "name": "CI Service",
    "token": "sk_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "token_prefix": "sk_xxxx...",
    "expires_at": "2026-12-31T23:59:59Z"
  }
}
```

`api_key` 字段仅在请求中包含 `initial_api_key` 时返回。`token` 为明文 Token，仅此一次返回。

**副作用：** 创建成功后自动创建物理目录 `<workspace_dir>/namespaces/<tenant_id>/`。

### API Key 管理接口（仅 Admin）

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/api/v1/tenants/{id}/keys` | Admin | 列出租户的所有 API Key |
| POST | `/api/v1/tenants/{id}/keys` | Admin | 创建 API Key（Token 仅返回一次） |
| DELETE | `/api/v1/tenants/{id}/keys/{key_id}` | Admin | 撤销 API Key |

### Namespace 文件操作接口

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/api/v1/namespaces/{id}/files/**` | Namespace 所有者 或 Admin | 读取文件/列出目录 |
| PUT | `/api/v1/namespaces/{id}/files/**` | Namespace 所有者 或 Admin | 写入文件 |
| POST | `/api/v1/namespaces/{id}/files/**` | Namespace 所有者 或 Admin | 创建文件/目录 |
| DELETE | `/api/v1/namespaces/{id}/files/**` | Namespace 所有者 或 Admin | 删除文件/目录 |

**说明：** Namespace 文件操作仅限所有者和 Admin，不对外开放。对外共享的文件访问通过 Share 路径进行。

### Share 管理接口

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| POST | `/api/v1/shares` | 任何已认证用户 | 创建 Share（共享自己 Namespace 的子目录） |
| GET | `/api/v1/shares` | 任何已认证用户 | 列出 Share（分页，可按 visibility、owner_tenant_id、name 筛选；Admin：全部；租户：有权限的 + 公开的 + 自己的） |
| GET | `/api/v1/shares/{id}` | read | 获取 Share 详情 |
| PUT | `/api/v1/shares/{id}` | admin | 更新 Share（name、description、visibility） |
| DELETE | `/api/v1/shares/{id}` | admin | 删除 Share |

#### `POST /api/v1/shares` 请求与响应

**请求体：**

```json
{
  "owner_tenant_id": "uuid",
  "name": "shared-lib",
  "source_path": "/projects/shared-lib",
  "description": "公共组件库",
  "visibility": "private"
}
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `owner_tenant_id` | 条件必填 | Admin 调用时必填，指定哪个租户的 Namespace；租户调用时忽略此字段（自动设为调用者） |
| `name` | 是 | Share 名称，同一租户下唯一 |
| `source_path` | 是 | Namespace 内的目录路径，必须已存在 |
| `description` | 否 | 描述 |
| `visibility` | 否 | `private`（默认）或 `public` |

**所有权规则：**
- **租户调用**：`owner_tenant_id` 自动设为调用者（即使传了也忽略），`source_path` 在调用者的 Namespace 内验证
- **Admin 调用**：必须传递 `owner_tenant_id` 指定哪个租户的 Namespace，未传则返回 `400 VALIDATION_ERROR`

**响应体（201 Created）：**

```json
{
  "id": "uuid",
  "owner_tenant_id": "uuid",
  "name": "shared-lib",
  "source_path": "/projects/shared-lib",
  "description": "公共组件库",
  "visibility": "private",
  "metadata": {},
  "created_at": "2026-03-11T00:00:00Z",
  "updated_at": "2026-03-11T00:00:00Z"
}
```

#### Share 文件操作接口

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/api/v1/shares/{id}/files/**` | read | 读取文件/列出目录（范围限制在 source_path 内） |
| PUT | `/api/v1/shares/{id}/files/**` | write | 写入文件 |
| POST | `/api/v1/shares/{id}/files/**` | write | 创建文件/目录 |
| DELETE | `/api/v1/shares/{id}/files/**` | write | 删除文件/目录 |

**路径限制：** 所有文件操作的路径被限制在 Share 的 `source_path` 内，不可通过 `..` 等方式逃逸。

### Share 权限管理接口

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/api/v1/shares/{id}/permissions` | Share 所有者 或 Admin | 列出 Share 的所有权限 |
| POST | `/api/v1/shares/{id}/permissions` | Share 所有者 或 Admin | 授予权限 |
| PUT | `/api/v1/shares/{id}/permissions/{tenant_id}` | Share 所有者 或 Admin | 更新权限级别 |
| DELETE | `/api/v1/shares/{id}/permissions/{tenant_id}` | Share 所有者 或 Admin | 撤销权限 |
| GET | `/api/v1/tenants/{id}/permissions` | Admin | 列出租户在所有 Share 上的权限 |

**所有者权限管理说明：** Share 所有者（`owner_tenant_id`）可以管理自己 Share 的权限，无需 Admin 介入。所有者不能给自己授权（自动拥有 admin 权限）。

### 审计日志接口（仅 Admin）

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/api/v1/audit-logs` | Admin | 列出审计日志（分页，查询参数见下） |

**审计日志查询参数：**

| 参数 | 类型 | 说明 |
|------|------|------|
| `action` | string（多值，逗号分隔） | 按操作类型筛选，如 `action=tenant.create,tenant.update` |
| `actor_type` | string | `admin` 或 `tenant` |
| `actor_id` | UUID | 执行者 ID（用于筛选具体租户的操作） |
| `resource_type` | string | `tenant`、`share`、`api_key`、`permission` |
| `resource_id` | UUID | 具体资源 ID |
| `from` | ISO 8601 | 时间范围起始（含），如 `2026-03-01T00:00:00Z` |
| `to` | ISO 8601 | 时间范围结束（含），如 `2026-03-11T23:59:59Z` |
| `page` | integer | 页码（默认 1） |
| `page_size` | integer | 每页条数（默认 20，最大 100） |

### Sandbox 接口

| 方法 | 路径 | 说明 | 权限 |
|------|------|------|------|
| POST | `/api/v1/sandboxes` | 创建 Sandbox | Namespace 所有者（自己的空间） |
| GET | `/api/v1/sandboxes` | 列出 Sandbox（分页，可按 state、namespace_id、name 筛选） | Admin：全部；租户：自己 Namespace 内的 |
| GET | `/api/v1/sandboxes/{id}` | 获取 Sandbox 详情 | Namespace 所有者 或 Admin |
| DELETE | `/api/v1/sandboxes/{id}` | 停止/删除 Sandbox | Namespace 所有者 或 Admin |
| POST | `/api/v1/sandboxes/batch-delete` | 批量停止/删除 Sandbox | 逐个校验权限（见下） |

**Sandbox 列表响应字段补充：** `GET /api/v1/sandboxes` 响应中的每个 Sandbox 对象额外包含 `namespace_name` 字段（租户名称），通过 JOIN `tenants` 表在单次查询中获取，避免前端 N+1 查询。Sandbox 详情接口同理。

#### `POST /api/v1/sandboxes/batch-delete`

**请求体：**

```json
{
  "ids": ["uuid-1", "uuid-2", "uuid-3"]
}
```

**响应体（200 OK）：**

```json
{
  "succeeded": ["uuid-1", "uuid-2"],
  "failed": [
    { "id": "uuid-3", "error": "Sandbox is in starting state" }
  ]
}
```

**说明：**
- 批量删除为尽力执行（best-effort），每个 Sandbox 独立处理。响应中分别列出成功和失败的条目。单次最多 100 个
- **权限校验**：逐个校验每个 Sandbox 的 Namespace 所有权。租户只能删除自己 Namespace 内的 Sandbox，无权的 ID 返回在 `failed` 列表中（error: "Permission denied"），不影响其他 ID 的处理
- **跨页全选支持**：除 `ids` 模式外，还支持 `filter` 模式，用于前端"选择所有匹配结果"场景：

```json
{
  "filter": {
    "state": "stopped",
    "namespace_id": "uuid"
  }
}
```

`ids` 和 `filter` 二选一，同时传递返回 `400`。`filter` 模式按条件查询所有匹配的 Sandbox 并逐个删除，同样逐个校验权限。

#### `POST /api/v1/sandboxes` 请求体

```json
{
  "name": "dev-sandbox",
  "root_path": "/projects/app1",
  "template": "ubuntu:22.04",
  "env": { "NODE_ENV": "development" },
  "timeout": 3600,
  "mounts": [
    { "share_id": "uuid-of-shared-lib", "mount_path": "/ext/shared-lib" }
  ]
}
```

| 字段 | 必填 | 说明 |
|------|------|------|
| `root_path` | 否 | Namespace 内的工作根路径，默认 `/` |
| `template` | 是 | Docker 镜像 |
| `mounts` | 否 | 额外挂载的 Share 列表 |
| `mounts[].share_id` | 是 | 要挂载的 Share ID |
| `mounts[].mount_path` | 是 | 容器内挂载点（绝对路径，不能与 /workspace 冲突） |

**Namespace 自动确定：** 租户调用时 `namespace_id` 自动设为调用者的 tenant_id。Admin 可指定 `namespace_id`。

**挂载权限检查：** 每个 `mounts` 条目中的 `share_id`，需验证调用者对该 Share 具有至少 `read` 权限。Share 挂载时使用的权限级别（只读/读写）取决于调用者对该 Share 的实际权限：
- 有 `write` 及以上 → 读写挂载
- 仅 `read` → 只读挂载

### 进程与 PTY 接口（通过 Sandbox → Namespace 校验权限）

| 方法 | 路径 | 所需权限 |
|------|------|---------|
| POST | `/api/v1/sandboxes/{id}/process/run` | Namespace 所有者 |
| GET | `/api/v1/sandboxes/{id}/process/run/stream` | Namespace 所有者 |
| POST | `/api/v1/sandboxes/{id}/process/{pid}/kill` | Namespace 所有者 |
| POST | `/api/v1/sandboxes/{id}/pty` | Namespace 所有者 |
| GET | `/api/v1/sandboxes/{id}/pty/{pty_id}` | Namespace 所有者 |
| POST | `/api/v1/sandboxes/{id}/pty/{pty_id}/resize` | Namespace 所有者 |
| DELETE | `/api/v1/sandboxes/{id}/pty/{pty_id}` | Namespace 所有者 |

**说明：** 进程和 PTY 操作仅限 Sandbox 所属 Namespace 的所有者（和 Admin）。Sandbox 运行在租户自己的 Namespace 内，其他租户不能直接操作。

### 租户自助接口

| 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|
| GET | `/api/v1/me` | 任何租户 | 获取当前租户信息（含 Namespace 存储配置） |
| GET | `/api/v1/me/files/**` | 任何租户 | 浏览自己 Namespace 的文件 |
| GET | `/api/v1/me/shares` | 任何租户 | 列出我创建的所有 Share |
| GET | `/api/v1/me/accessible-shares` | 任何租户 | 列出我有权访问的所有 Share（含公开的） |
| GET | `/api/v1/me/sandboxes` | 任何租户 | 列出我 Namespace 内的所有 Sandbox |

## 存储架构

### StorageRouter 变更

当前 StorageRouter 以 `workspace_id` 为 key 路由存储请求。变更为以 `namespace_id`（即 `tenant_id`）为 key：

```rust
// 旧
fn get_backend(&self, workspace_id: &str) -> Arc<dyn StorageBackend>;

// 新
fn get_backend(&self, namespace_id: Uuid) -> Arc<dyn StorageBackend>;
```

- **Managed Namespace**：`LocalStorageBackend` 指向 `<workspace_dir>/namespaces/<namespace_id>/`
- **Remote Namespace**：`RemoteStorageBackend` 连接客户端提供的存储

### Share 文件访问路径

Share 的文件操作在应用层做路径映射，不创建物理 bind mount：

```
Share 文件请求: GET /shares/{share_id}/files/src/lib.rs
  → 查询 Share: { owner_tenant_id: "aaa", source_path: "/projects/shared-lib" }
  → 权限检查: 调用者对该 Share 有 read 权限？
  → 转换为 Namespace 文件操作: namespace_id="aaa", path="/projects/shared-lib/src/lib.rs"
  → StorageRouter.get_backend("aaa").read_file("/projects/shared-lib/src/lib.rs")
```

### Sandbox 容器挂载

Sandbox 创建时，Docker bind mount 配置：

```
主挂载（root_path）:
  host: <workspace_dir>/namespaces/<namespace_id>/<root_path>/
  container: /workspace

Share 挂载（每个 sandbox_mounts 条目）:
  host: <workspace_dir>/namespaces/<share.owner_tenant_id>/<share.source_path>/
  container: <mount_path>
  readonly: 取决于调用者对该 Share 的权限
```

对于 DinD 场景，使用 `WORKSPACE_WORKSPACE_HOST_DIR` 替换主机路径前缀（与现有逻辑一致）。

### Remote Namespace（客户端提供存储）

当 Tenant 的 `storage_type = remote` 时：

1. 客户端通过 `ClientStorageService.Connect` 建立双向流连接
2. 认证：通过 `StorageHandshake.token` 传递 API Key
3. 服务端验证：确认是该 Namespace 的所有者
4. 创建 `RemoteStorageBackend` 注册到 StorageRouter

与旧架构的区别：一个 ClientStorageService 连接现在服务于整个 Namespace（而非单个 Workspace），客户端需要提供 Namespace 根目录下的所有文件访问能力。

### NFS 导出

NFS 导出从 per-workspace 改为 per-namespace：

- NFS 根目录下的子目录为 `namespace_id`（不再是 `workspace_id`）
- 每个 Namespace 导出整个存储空间
- Sandbox 容器可通过 `nfs://<host>:2049/<namespace_id>/<root_path>` 挂载

### Lease 系统

Lease 从 per-workspace 改为 per-namespace。写操作需要持有目标 Namespace 的 lease。

## 前端（Admin 管理后台）

### 技术选型

- React 18 + TypeScript
- Ant Design 组件库
- Vite 构建工具
- 项目目录：monorepo 下的 `web/` 目录
- 通过 REST API 与后端通信

### 前端架构

| 关注点 | 方案 | 说明 |
|--------|------|------|
| 路由 | React Router v7 | 嵌套路由 + 路由守卫 |
| 数据请求 | TanStack Query (React Query) v5 | 自动缓存、失效、重试 |
| 状态管理 | Zustand | 仅用于全局 UI 状态 |
| HTTP 客户端 | Axios | 统一请求拦截器 |
| 认证流 | JWT 存储于 `localStorage` | 401 自动跳转登录页 |
| 代码规范 | ESLint + Prettier | 统一代码风格 |

**目录结构：**

```
web/
├── src/
│   ├── api/          # API 请求函数（tenants.ts, shares.ts, sandboxes.ts, ...）
│   ├── components/   # 通用组件（布局、表格、表单等）
│   ├── hooks/        # 自定义 hooks（useAuth 等）
│   ├── pages/        # 页面组件（按路由组织）
│   ├── stores/       # Zustand stores
│   ├── types/        # TypeScript 类型定义
│   └── utils/        # 工具函数
├── index.html
├── vite.config.ts
├── tsconfig.json
└── package.json
```

### 部署方式

前端构建为静态资源，与后端打包在同一 Docker 镜像中：
- Rust 通过 `tower-http::services::ServeDir` 在 `/admin/*` 路径下提供静态文件
- `/api/*` 请求由 Axum 直接处理

### 页面

| 路由 | 说明 |
|------|------|
| `/login` | Admin 登录页 |
| `/` | 仪表盘 — 租户/Share/Sandbox 数量统计 |
| `/tenants` | 租户列表，支持搜索、筛选（启用/停用、存储类型）、创建 |
| `/tenants/:id` | 租户详情：基本信息（含存储配置）、API Key 列表、Share 列表、Namespace 文件浏览器 |
| `/shares` | Share 列表，支持搜索、筛选（可见性、所有者）、创建 |
| `/shares/:id` | Share 详情：基本信息、权限表、文件浏览器（限定在 source_path 内） |
| `/sandboxes` | 全局 Sandbox 列表，支持搜索、筛选（状态、所属 Namespace）、批量清理 |
| `/audit-logs` | 审计日志查看器 |

### 关键交互

**创建租户流程：**
1. 填写名称 + 描述 + 存储类型 → 提交
2. 租户创建成功（Namespace 自动创建），可选择立即创建第一个 API Key
3. API Key 弹窗展示完整 Token

**Share 管理**（Share 详情页）：
- 权限表格，显示被授权的租户和权限级别
- 添加：选择租户 + 权限级别下拉框
- 编辑：行内修改权限级别
- 移除：撤销权限
- 公开 Share：显示提示"所有活跃租户拥有隐式 read 权限"

**API Key 管理**（租户详情页）：
- 列表展示：名称、前缀（`sk_a1b2...`）、创建时间、过期时间、最后使用时间、状态
- 创建：名称 + 可选过期时间 → 展示 Token 一次
- 撤销：确认对话框 → 立即撤销

## 配置变更

### 新增环境变量

| 变量 | 默认值 | 是否必填 | 说明 |
|------|--------|---------|------|
| `ADMIN_PASSWORD` | — | 是（启用 Admin 后台时） | Admin 登录密码 |
| `JWT_SECRET` | — | 是（启用 Admin 后台时） | JWT 签名密钥 |
| `JWT_EXPIRATION_HOURS` | 24 | 否 | JWT Token 过期时间（小时） |
| `WORKSPACE_RATE_LIMIT_RPS` | 100 | 否 | 每个 IP 每秒最大请求数 |
| `NAMESPACE_TRASH_RETENTION_DAYS` | 7 | 否 | 已删除 Namespace 物理目录保留天数 |

### 变更环境变量

| 变量 | 旧默认值 | 新默认值 | 说明 |
|------|---------|---------|------|
| `WORKSPACE_DATABASE_URL` | `sqlite:data/workspace.db?mode=rwc` | `postgres://elevo:elevo@localhost:5432/elevo` | SQLite → PostgreSQL |

### 移除环境变量

| 变量 | 说明 |
|------|------|
| `WORKSPACE_FS_API_TOKEN` | 由租户 API Key 认证取代 |

## 数据库

### 从 SQLite 迁移到 PostgreSQL

全量重构，不保留旧 SQLite 数据。

**迁移原因：**
- 多租户场景下并发读写压力增大
- 原生 UUID、TIMESTAMPTZ、JSONB、INET 类型
- 完整的 ALTER TABLE 和 FK 约束支持
- 为未来水平扩展奠定基础

**Rust 代码改动范围：**
- `SqlitePool` / `SqlitePoolOptions` → `PgPool` / `PgPoolOptions`
- 绑定参数 `?` → `$1, $2, ...`
- 移除 `PRAGMA journal_mode = WAL`
- `datetime()` 函数 → PostgreSQL 区间运算
- `INSERT OR IGNORE` → `INSERT ... ON CONFLICT DO NOTHING`
- 时间戳字段从 `String` 改为 `chrono::DateTime<Utc>`
- JSON 字段从 `String` 改为 `serde_json::Value`
- Lease 动态建表移入迁移文件

**Cargo.toml 依赖变更：**
```toml
# 旧
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
# 新
sqlx = { version = "0.8", features = ["runtime-tokio", "postgres", "chrono", "uuid"] }
```

### 完整 Schema（PostgreSQL）

删除旧 SQLite 迁移文件，创建全新的 PostgreSQL 初始化迁移 `00000000000000_init.sql`：

```sql
-- ============================================================
-- 租户 + Namespace
-- ============================================================

CREATE TABLE tenants (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            VARCHAR(255) NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    is_active       BOOLEAN NOT NULL DEFAULT true,
    storage_type    VARCHAR(16) NOT NULL DEFAULT 'managed'
                    CHECK(storage_type IN ('managed', 'remote')),
    storage_config  JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- 大小写不敏感的唯一约束，防止 "Team Alpha" 和 "team alpha" 共存
CREATE UNIQUE INDEX idx_tenants_name_lower ON tenants(lower(name));

-- API Key
CREATE TABLE api_keys (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id    UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name         VARCHAR(255) NOT NULL,
    token_hash   VARCHAR(64) UNIQUE NOT NULL,
    token_prefix VARCHAR(16) NOT NULL,
    is_active    BOOLEAN NOT NULL DEFAULT true,
    expires_at   TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);
CREATE INDEX idx_api_keys_tenant_id ON api_keys(tenant_id);

-- ============================================================
-- Share（共享目录）
-- ============================================================

CREATE TABLE shares (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    owner_tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    name            VARCHAR(255) NOT NULL,
    source_path     TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    visibility      VARCHAR(16) NOT NULL DEFAULT 'private'
                    CHECK(visibility IN ('public', 'private')),
    metadata        JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_shares_owner_path ON shares(owner_tenant_id, source_path);
CREATE UNIQUE INDEX idx_shares_owner_name ON shares(owner_tenant_id, name);

-- Share 权限
CREATE TABLE share_permissions (
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    share_id   UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    permission VARCHAR(16) NOT NULL CHECK(permission IN ('read', 'write', 'execute', 'admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, share_id)
);
CREATE INDEX idx_sp_share_id ON share_permissions(share_id);

-- ============================================================
-- Sandbox
-- ============================================================

CREATE TABLE sandboxes (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          VARCHAR(255),
    namespace_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    root_path     TEXT NOT NULL DEFAULT '/',
    template      VARCHAR(255) NOT NULL,
    state         VARCHAR(16) NOT NULL DEFAULT 'starting',
    container_id  VARCHAR(64),
    env           JSONB NOT NULL DEFAULT '{}',
    metadata      JSONB NOT NULL DEFAULT '{}',
    timeout       INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_sandboxes_namespace_id ON sandboxes(namespace_id);
CREATE INDEX idx_sandboxes_state ON sandboxes(state);

-- Sandbox 挂载
CREATE TABLE sandbox_mounts (
    sandbox_id UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    share_id   UUID NOT NULL REFERENCES shares(id) ON DELETE RESTRICT,
    mount_path TEXT NOT NULL,
    PRIMARY KEY (sandbox_id, share_id),
    UNIQUE (sandbox_id, mount_path)
);

-- ============================================================
-- 进程 / PTY
-- ============================================================

CREATE TABLE processes (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sandbox_id  UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    command     TEXT NOT NULL,
    state       VARCHAR(16) NOT NULL DEFAULT 'running',
    pid         INTEGER,
    exit_code   INTEGER,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_processes_sandbox_id ON processes(sandbox_id);

CREATE TABLE ptys (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sandbox_id  UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    cols        INTEGER NOT NULL DEFAULT 80,
    rows        INTEGER NOT NULL DEFAULT 24,
    state       VARCHAR(16) NOT NULL DEFAULT 'running',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_ptys_sandbox_id ON ptys(sandbox_id);

-- ============================================================
-- Namespace Lease
-- ============================================================

CREATE TABLE namespace_leases (
    namespace_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    holder_id    VARCHAR(255) NOT NULL,
    acquired_at  TIMESTAMPTZ NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    renewed_at   TIMESTAMPTZ NOT NULL
);

-- ============================================================
-- 审计日志
-- ============================================================

CREATE TABLE audit_logs (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_type    VARCHAR(16) NOT NULL CHECK(actor_type IN ('admin', 'tenant')),
    actor_id      UUID,
    action        VARCHAR(64) NOT NULL,
    resource_type VARCHAR(32) NOT NULL,
    resource_id   UUID NOT NULL,
    resource_name VARCHAR(255) NOT NULL DEFAULT '',
    detail        JSONB NOT NULL DEFAULT '{}',
    ip_address    INET,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_logs_created_at ON audit_logs(created_at);
CREATE INDEX idx_audit_logs_actor ON audit_logs(actor_type, actor_id);
CREATE INDEX idx_audit_logs_action ON audit_logs(action);
CREATE INDEX idx_audit_logs_resource ON audit_logs(resource_type, resource_id);
-- 覆盖常见组合筛选查询（操作类型 + 执行者类型 + 时间范围）
CREATE INDEX idx_audit_logs_query ON audit_logs(created_at, action, actor_type);
```

### 索引说明

| 索引 | 用途 |
|------|------|
| `tenants(lower(name))` UNIQUE | 租户名称大小写不敏感唯一 |
| `shares(owner_tenant_id, source_path)` UNIQUE | 同一 Namespace 下路径唯一 |
| `shares(owner_tenant_id, name)` UNIQUE | 同一租户下 Share 名称唯一 |
| `api_keys.token_hash` UNIQUE | 认证主查找路径 |
| `api_keys.tenant_id` | 按租户列出 Key |
| `share_permissions.share_id` | 按 Share 列出权限 |
| `sandboxes.namespace_id` | 按 Namespace 列出 Sandbox |
| `sandboxes.state` | 按状态查询 |
| `audit_logs.created_at` | 时间范围查询 |
| `audit_logs.action` | 按操作类型筛选 |
| `audit_logs(resource_type, resource_id)` | 按资源类型和 ID 筛选 |
| `audit_logs(created_at, action, actor_type)` | 覆盖常见组合筛选查询 |
| `api_keys(tenant_id, name)` UNIQUE | 同一租户下 Key 名称唯一 |

### Docker Compose 部署

```yaml
services:
  postgres:
    image: postgres:17-alpine
    environment:
      POSTGRES_USER: elevo
      POSTGRES_PASSWORD: elevo
      POSTGRES_DB: elevo
    volumes:
      - pg_data:/var/lib/postgresql/data
    ports:
      - "5432:5432"
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U elevo"]
      interval: 5s
      timeout: 3s
      retries: 5

  workspace-server:
    depends_on:
      postgres:
        condition: service_healthy
    environment:
      WORKSPACE_DATABASE_URL: postgres://elevo:elevo@postgres:5432/elevo

volumes:
  pg_data:
```

## 开发模式

若未设置 `ADMIN_PASSWORD`，认证中间件关闭，所有请求视为 Admin 身份——方便本地开发调试。

**安全防护：** 开发模式启动时，日志中输出醒目的 WARNING 信息：
```
⚠️  WARNING: ADMIN_PASSWORD is not set. Authentication is DISABLED. All requests are treated as Admin.
⚠️  DO NOT use this mode in production.
```
启动日志中每隔 60 秒重复输出此警告，直到设置 `ADMIN_PASSWORD`。

## SDK 更新说明

**这是破坏性变更**。已有的 TypeScript、Python、Go SDK 需要适配新的 Namespace/Share 模型：

### 主要变化

1. **认证**：客户端构造函数新增 `api_key` 参数
2. **实体模型**：Workspace 相关类型替换为 Namespace + Share
3. **Sandbox 创建**：从 `workspace_id` 改为 `root_path` + 可选 `mounts`
4. **文件操作**：从 `workspace_id` + path 改为 `namespace_id` + path 或 `share_id` + path
5. **StorageProvider**（Go SDK）：从 per-workspace 改为 per-namespace

### SDK 类型映射

| 旧概念 | 新概念 |
|--------|--------|
| `Workspace` | `Share`（共享目录）或 Namespace 文件操作 |
| `WorkspaceService` | `ShareService` + `NamespaceService` |
| `Sandbox.workspace_id` | `Sandbox.namespace_id` + `Sandbox.root_path` |
| `StorageProvider(workspace_id)` | `StorageProvider()`（服务整个 Namespace） |

## 基础速率限制

- 通过 `tower_governor` crate 实现 per-IP 速率限制（注意：`tower::limit::RateLimitLayer` 是全局限流，不支持 per-IP，不适用）
- 默认：每个 IP 每秒 100 次请求（`WORKSPACE_RATE_LIMIT_RPS`）
- 超限返回 `429 Too Many Requests`，响应头包含 `Retry-After` 秒数
- 登录接口 `POST /api/v1/auth/login` 使用独立的更严格限制：每个 IP 每分钟 10 次
- gRPC 暂不限流（Agent 心跳等内部通信不应被限制）

## 已知限制与未来迭代

| 限制 | 说明 | 未来可能的方案 |
|------|------|--------------|
| 权限层级锁定 | 无法实现"可写但不可执行终端" | 权限位掩码或 JSON 数组 |
| 单密码 Admin | 无独立管理员账户 | 多管理员账户 + RBAC |
| 全局速率限制 | 仅 IP 级别限流 | 基于租户的限流 + 配额 |
| 无资源配额 | 租户的 Share、Sandbox、存储量无限制 | 配额表 + 强制执行 |
| 无 API Key 轮换 | 需撤销旧 Key + 创建新 Key | Key 轮换接口 |
| Share 不支持子路径权限 | 整个 Share 统一权限 | 扩展为路径级 ACL |
| Namespace 存储类型不可变 | 创建后不能从 managed 改为 remote | 存储迁移工具 |
