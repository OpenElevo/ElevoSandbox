# 共享 Workspace 使用指南

## 前提条件

- 管理员已在后台为你创建租户并生成 API Key（`token` 以 `sk_` 开头）
- 对方已创建 Share 并授予你的租户权限（public Share 自动获得 Read 权限，private Share 需要显式授权）

## 权限说明

| 级别 | 能力 |
|------|------|
| **read** | 读取共享目录中的文件 |
| **write** | 读写文件 |
| **execute** | 读写并执行文件 |
| **admin** | 完全管理权限（包括修改 Share 配置、管理权限） |

---

## 一、NFS 挂载

### 托管型 Workspace（storage_type = managed）

服务端内嵌 NFS v3 服务器。创建 Workspace 后，服务端返回 `nfs_url`（格式：`nfs://{host}:{port}/{workspace_id}`），客户端可直接挂载：

```bash
# 安装 NFS 客户端（如未安装）
# Ubuntu/Debian: apt install nfs-common
# CentOS/RHEL: yum install nfs-utils

# 挂载
mount -t nfs -o nfsvers=3,tcp,nolock,port=2049,mountport=2049 \
  <nfs_host>:2049/<workspace_id> /mnt/workspace

# 验证
ls /mnt/workspace
```

### 远程型 Workspace（storage_type = remote）

远程租户的文件由 StorageProvider 提供。当 StorageProvider 通过 gRPC 连接后，服务端会为该租户创建 FUSE 挂载。StorageProvider 也可以切换为 NFS 传输通道以提高性能——由 StorageProvider 侧导出 NFS，服务端自动挂载。

### NFS 配置项

| 环境变量 | 默认值 | 说明 |
|---------|--------|------|
| `WORKSPACE_NFS_MODE` | `embedded` | NFS 模式：`embedded`（内嵌服务器）或 `system`（系统 nfs-kernel-server） |
| `WORKSPACE_NFS_PORT` | `2049` | NFS 服务端口 |
| `WORKSPACE_NFS_HOST` | `127.0.0.1` | 返回给客户端的 NFS 主机地址 |
| `WORKSPACE_NFS_ALLOWED_CIDRS` | `[]` | 远程 NFS 挂载的 CIDR 白名单（为空则拒绝所有远程挂载） |

---

## 二、SDK 使用

### 安装

**TypeScript SDK：**

```bash
npm install @elevo/workspace-sdk
```

**Go SDK：**

```bash
go get github.com/OpenElevo/ElevoWorkspace/sdk-go
```

### 连接与认证

所有 SDK 请求通过 gRPC，认证方式为在请求头中附带 API Key：

**TypeScript：**

```typescript
import { WorkspaceClient } from '@elevo/workspace-sdk';

const client = new WorkspaceClient('localhost:9090', {
  apiKey: 'sk_your_api_key_token',
});
```

**Go：**

```go
import workspace "github.com/OpenElevo/ElevoWorkspace/sdk-go"

client := workspace.NewClient("localhost:9090", workspace.ClientOptions{
    APIKey: "sk_your_api_key_token",
})
```

### 查看 Workspace

创建 Workspace 后，服务端返回包含 `nfs_url` 的 Workspace 对象：

**TypeScript：**

```typescript
// 查询自己的 Workspace 列表
const workspaces = await client.workspace.list();
console.log(workspaces);
// 每个 workspace 包含: id, name, nfs_url, storage_type ...
```

**Go：**

```go
workspaces, err := client.Workspace.List(ctx)
```

### 作为 StorageProvider 共享本地文件（远程租户）

如果你的租户是 `storage_type = remote`，需要在你的机器上运行 StorageProvider，将本地目录暴露给服务端：

**TypeScript：**

```typescript
const provider = client.newStorageProvider({
  localDir: '/path/to/your/local/data',  // 本地共享目录
  workspaceId: 'your-tenant-id',          // 租户 ID（即 namespace ID）
  token: 'sk_your_api_key_token',
});

// 启动 StorageProvider，保持运行
await provider.share(AbortSignal.timeout(3600_000));
```

**Go：**

```go
provider := client.NewStorageProvider(workspace.StorageProviderConfig{
    LocalDir:    "/path/to/your/local/data",
    WorkspaceID: "your-tenant-id",
    Token:       "sk_your_api_key_token",
})

// 启动并阻塞
provider.Share(ctx)
```

StorageProvider 启动后会通过 gRPC 双向流连接到服务端，完成握手认证后，服务端对该租户的所有文件操作都会通过这个流转发到你的本地 `localDir`。

### 在 StorageProvider 上注册 NFS 传输（可选）

为了提高文件传输性能，StorageProvider 可以注册 NFS 传输通道替代 gRPC 流：

```go
// Go SDK
err := client.Workspace.RegisterNfsTransport(ctx, &workspace.RegisterNfsTransportParams{
    WorkspaceID: "your-tenant-id",
    NFSUrl:      "nfs://192.168.1.100:2049/your-local-data",
})
```

注册后，服务端会原子切换到 NFS 挂载方式访问你的文件，gRPC 流会被断开。

---

## 三、HTTP API 使用

### 认证

所有 API 请求需要在 Header 中携带 API Key：

```
Authorization: Bearer sk_your_api_key_token
```

### 查看我能访问的 Share

```
GET /api/v1/me/accessible-shares
Authorization: Bearer sk_your_api_key_token
```

响应：

```json
{
  "items": [
    {
      "id": "share-uuid",
      "owner_tenant_id": "owner-uuid",
      "name": "共享数据集",
      "source_path": "datasets/images",
      "visibility": "public",
      "created_at": "2026-03-17T10:00:00Z"
    }
  ],
  "total": 1
}
```

### 查看 Share 详情

```
GET /api/v1/shares/<share-id>
Authorization: Bearer sk_your_api_key_token
```

### 查看我拥有的 Share

```
GET /api/v1/me/shares
Authorization: Bearer sk_your_api_key_token
```

### 文件操作

通过 HTTP API 直接操作 Share 中的文件，无需挂载：

**列出目录内容：**

```
GET /api/v1/shares/<share-id>/files/list?path=/
Authorization: Bearer sk_your_api_key_token
```

响应：

```json
{
  "files": [
    { "name": "data.csv", "path": "data.csv", "type": "file", "size": 1024, "modified_at": "2026-03-17T10:00:00Z" },
    { "name": "models", "path": "models", "type": "directory", "size": 0, "modified_at": "2026-03-17T10:00:00Z" }
  ]
}
```

**读取文件内容：**

```
GET /api/v1/shares/<share-id>/files?path=data.csv
Authorization: Bearer sk_your_api_key_token
```

响应：

```json
{
  "content": "文件内容（文本）"
}
```

**写入文件（需要 write 权限）：**

```
PUT /api/v1/shares/<share-id>/files?path=output/result.json
Authorization: Bearer sk_your_api_key_token
Content-Type: application/json

{"content": "{\"result\": \"ok\"}"}
```

**删除文件（需要 write 权限）：**

```
DELETE /api/v1/shares/<share-id>/files?path=temp.txt
Authorization: Bearer sk_your_api_key_token
```

删除目录（递归）：

```
DELETE /api/v1/shares/<share-id>/files?path=temp_dir&recursive=true
Authorization: Bearer sk_your_api_key_token
```

### 权限不足

如果是 private Share 且你未被授权，调用 Share 相关 API 会返回 `404 NOT_FOUND`（服务端会隐藏 Share 的存在）。请联系 Share 所有者，让其调用授权接口为你授权：

```
POST /api/v1/shares/<share-id>/permissions
Authorization: Bearer <owner-api-key-token>
Content-Type: application/json

{"tenant_id": "your-tenant-id", "permission": "write"}
```

### 错误响应格式

所有 API 的错误响应统一格式：

```json
{
  "error": {
    "code": "ERROR_CODE",
    "message": "描述信息"
  }
}
```

常见错误码：`UNAUTHORIZED`（未认证）、`FORBIDDEN`（权限不足）、`NOT_FOUND`（资源不存在或无权查看）、`BAD_REQUEST`（参数错误）。
