# 使用其他租户共享的 Workspace

## 前提条件

- 管理员已在后台为你创建租户并生成 API Key（包含 `tenant_id` 和 `token`）
- 对方已创建 Share 并授予你的租户权限

## Share 的两种可见性

| 类型 | 说明 |
|------|------|
| **public** | 所有租户自动获得 Read 权限，无需额外授权 |
| **private** | 必须由 Share 所有者显式授予你权限 |

## 步骤 1：查看可用的 Share

```
GET /api/v1/shares
Authorization: Bearer <your-api-key-token>
```

返回你有权访问的所有 Share 列表，每个 Share 包含：

- `id` — Share 的唯一标识
- `name` — Share 名称
- `owner_tenant_id` — 所属租户
- `source_path` — 源目录路径
- `visibility` — `public` 或 `private`

## 步骤 2：创建 Sandbox 时挂载 Share

在创建 Sandbox 的请求中，通过 `mounts` 参数指定要挂载的 Share：

```json
{
  "tenant_id": "<your-tenant-id>",
  "mounts": [
    {
      "share_id": "<share-uuid>",
      "mount_path": "/mnt/shared-data"
    }
  ]
}
```

- `share_id` — 从步骤 1 获取的 Share ID
- `mount_path` — 挂载到 Sandbox 内的目标路径（必须以 `/` 开头）

系统会自动验证你的权限：

- 如果是 **public** Share，所有租户自动通过
- 如果是 **private** Share，需要 Share 所有者通过权限 API 显式授予你 Read/Write/Execute/Admin 权限

## 步骤 3：在 Sandbox 中访问共享文件

挂载成功后，在 Sandbox 内部可以直接通过 `mount_path` 访问共享的文件和目录。所有文件操作（读、写、列出目录等）都通过该挂载路径完成。

## 权限级别

| 级别 | 能力 |
|------|------|
| **Read** | 只读访问共享目录 |
| **Write** | 可读写文件 |
| **Execute** | 可执行文件 |
| **Admin** | 完全管理权限（包括修改 Share 配置） |

## 如果你没有权限（private Share）

需要联系 Share 的所有者，让其调用权限授予 API：

```
POST /api/v1/shares/<share-id>/permissions
{
  "tenant_id": "<your-tenant-id>",
  "level": "read"
}
```

## HTTP API 文件操作

除了通过 Sandbox 挂载，还可以直接通过 HTTP API 操作 Share 中的文件：

```
# 列出目录
GET /api/v1/shares/<share-id>/files?path=/

# 下载文件
GET /api/v1/shares/<share-id>/files?path=/some-file.txt

# 上传文件
PUT /api/v1/shares/<share-id>/files?path=/new-file.txt
```

同样需要 `Authorization: Bearer <token>` 头部。
