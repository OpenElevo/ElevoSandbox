# ElevoWorkspace 对接 ElevoOne 统一登录方案

## 1. 背景与目标

### 1.1 现状

**ElevoWorkspace** 当前的认证体系：

| 认证方式 | 使用场景 | 原理 |
|---------|---------|------|
| 管理员密码 | Admin 后台登录 | `POST /api/v1/auth/login` → HS256 JWT |
| API Key (sk_*) | SDK/程序化访问 | `Bearer sk_xxx` → SHA-256 hash 查库 |
| Dev Mode | 开发环境 | 无密码直接 admin |

- **无用户系统**：只有一个全局 admin 身份（密码登录）和 tenant 身份（API Key 登录）
- **无 users 表**：tenant 既是组织单元也是命名空间边界
- **Admin 前端**：React SPA，密码表单登录，localStorage 存 JWT
- **技术栈**：Rust/Axum 0.8 + jsonwebtoken (HS256) + PostgreSQL
- **现有中间件约束**：`verify_jwt()` 硬编码 `sub="admin"` 校验，只有 `Identity::Admin`（HS256 JWT）和 `Identity::Tenant`（API Key）两条认证路径

**ElevoOne** 提供的能力：

| 能力 | 说明 |
|------|------|
| OIDC Provider | 标准 OpenID Connect 1.0，Authorization Code Flow + PKCE |
| JWKS | `/.well-known/jwks.json` 暴露 RS256 公钥 |
| 多种社交登录 | 钉钉、微信、企微、飞书、Google、LDAP |
| 组织级 SSO | 用户按组织隔离，支持 org_id、org_role 等 claims（通过 ID Token 自定义 claims 传递） |
| Refresh Token | 支持轮换机制，有效期 7 天 |
| Token Exchange | RFC 8693 OBO，支持产品间传递用户身份 |
| Client Credentials | RFC 6749 M2M，支持后端间组织级访问 |
| Single Logout | `/oauth/end_session` 清除 SSO 会话 |

ElevoOne 已对接"elevo"产品（Go-Zero），有成熟的 OIDC 集成模式和参考实现。

### 1.2 目标

1. **管理后台**：Admin 用户通过 ElevoOne SSO 登录，替代手动输入密码
2. **外部系统接入**：外部系统通过 ElevoOne Client Credentials（后台服务/M2M）或 Token Exchange（用户驱动）获取 Workspace 专属 token 后，可直接调用 Workspace API（无需 API Key）
3. **双轨运行**：保留现有密码登录和 API Key 机制作为降级/兼容方案
4. **权限映射**：ElevoOne `org_role=admin` → Workspace admin；`org_role=member` → Workspace tenant

> **关于"管理后台 Tenant 登录"的说明**：
> 当前管理后台（Admin Console）是纯管理员界面，所有路由都需要 `Identity::Admin`。ElevoOne 中 `org_role=member` 的用户通过 OIDC 登录后，**不会进入管理后台**。但 member 用户访问 SSO 登录页时，ElevoOne 会自动创建 `user_product_associations`（Token Exchange 前置条件），相当于一次"激活"。之后 member 用户可通过 ElevoOne Token Exchange 获取 Workspace token，直接调用 Workspace API。首期方案中，管理后台 OIDC 登录仅对 `org_role=admin` 的用户开放。如果后续需要 Tenant 用户登录管理后台（查看有限视图），需要扩展前端权限模型。
>
> **行为依赖说明**：member 用户的"激活"机制依赖 ElevoOne 在 authorization_code 流程中自动创建 `user_product_associations` 的内部行为。此依赖需与 ElevoOne 团队确认为稳定的 API 契约。

---

## 2. 架构总览

```
                          ┌──────────────────────────┐
                          │       ElevoOne            │
                          │   (OIDC Provider)         │
                          │                          │
                          │  /oauth/authorize         │
                          │  /oauth/token             │
                          │  /.well-known/jwks.json   │
                          │  /oauth/end_session       │
                          │  /oauth/userinfo          │
                          └──────┬───────┬────────────┘
                                 │       │
                    authorize    │       │  JWKS / token exchange
                    redirect     │       │
                                 │       │
┌────────────────────────────────┼───────┼──────────────────────────┐
│           ElevoWorkspace       │       │                          │
│                                │       │                          │
│  ┌─────────────────────┐       │       │   ┌──────────────────┐  │
│  │   Admin Frontend     │       │       │   │  Auth Middleware  │  │
│  │   (React SPA)        │───────┘       │   │                  │  │
│  │                      │               │   │ 1. Admin JWT     │  │
│  │  /admin/login        │               │   │    (HS256,现有)  │  │
│  │  /admin/login/success│               │   │    sub="admin"   │  │
│  └─────────────────────┘               │   │ 2. API Key       │  │
│                                        │   │    (sk_*, 现有)   │  │
│  ┌─────────────────────┐               │   │ 3. ElevoOne Token│  │
│  │   OIDC Handler       │               │   │    (RS256,新增)   │  │
│  │                      │               │   │    alg=RS256     │  │
│  │  /api/v1/auth/oidc/  │               │   └──────────────────┘  │
│  │    authorize         │               │                        │
│  │    callback          │               │   ┌──────────────────┐  │
│  │    session           │               │   │  Tenant Mapping  │  │
│  │    logout            │               │   │                  │  │
│  │    refresh           │               │   │  elevoone_org_id │  │
│  │    config            │               │   │  ↔ tenants 表    │  │
│  └─────────────────────┘               │   └──────────────────┘  │
│                                        │                        │
│  ┌─────────────────────┐               │   ┌──────────────────┐  │
│  │  OIDC Config Handler │               │   │  OidcService     │  │
│  │                      │               │   │                  │  │
│  │  /api/v1/system/     │               │   │  JWKS 缓存/刷新  │  │
│  │    oidc-config       │               │   │  Token 验证      │  │
│  │                      │               │   │  Code 换 Token   │  │
│  └─────────────────────┘               │   └──────────────────┘  │
└───────────────────────────────────────────────────────────────────┘
```

### 2.1 三种认证方式并存

| 认证方式 | 触发场景 | Token 类型 | 验证方式 | 身份 |
|---------|---------|-----------|---------|------|
| 管理员密码 | 密码登录 / OIDC 登录(org_role=admin) | HS256 JWT (本地签发, `sub="admin"`) | 本地 secret 验证 | `Identity::Admin { session_id }` |
| API Key | SDK 程序化调用 | `sk_xxx` | SHA-256 hash 查库 | `Identity::Tenant { id, name }` |
| ElevoOne Token | 外部系统通过 ElevoOne Client Credentials 或 Token Exchange 获取的 Workspace 专属 token | RS256 JWT (ElevoOne 签发, `alg: RS256`) | JWKS 公钥验证 | `Identity::Tenant { id, name }` |

> **关于 aud 校验**：外部系统（如 elevo 产品）持有的 ElevoOne access_token 的 `aud` 是该产品的 client_id，不是 Workspace 的 client_id。直接传给 Workspace 会被 `aud` 校验拒绝。必须先通过 ElevoOne 的 Client Credentials（后台服务）或 Token Exchange（用户驱动）获取 `aud=workspace_client_id` 的专属 token，详见 3.10 节。

---

## 3. 详细设计

### 3.1 配置管理

#### 3.1.1 环境变量

OIDC 功能运行时需要的环境变量只有一个——用于加密 client_secret 的对称密钥：

| 环境变量 | 必填 | 默认值 | 说明 |
|---------|------|--------|------|
| `OIDC_SECRET_ENCRYPTION_KEY` | 否（生产环境推荐） | 从 `JWT_SECRET` 派生 | AES-256-GCM 加密密钥（≥32 字节） |

> OIDC 的 issuer_url / client_id / client_secret / redirect_uri 等业务配置全部通过 Admin 后台页面管理，存入数据库，修改即时生效，无需重启服务。这样管理员可以在不重启的情况下调整 SSO 配置。

**密钥派生规则**：当 `OIDC_SECRET_ENCRYPTION_KEY` 未配置时，使用 HKDF-SHA256 从 `JWT_SECRET` 派生 32 字节 AES 密钥：
```
info = "elevo-oidc-secret-encryption-v1"
salt = SHA-256("elevo-oidc-salt")
AES_KEY = HKDF-SHA256(salt=16bytes, IKM=JWT_SECRET, info=info, L=32)
```
> 为什么用 HKDF 而不是直接截取？`JWT_SECRET` 是 HMAC 密钥，长度和分布不一定适合直接用作 AES 密钥。HKDF 是标准的密钥派生函数，能从任意长度的输入密钥材料安全地派生出指定长度的密钥。

#### 3.1.2 数据库配置（oidc_config 表）

采用 singleton 模式（全局只有一条记录）：

```sql
CREATE TABLE oidc_config (
    id INT PRIMARY KEY DEFAULT 1 CHECK (id = 1),  -- 全局单行
    enabled BOOLEAN NOT NULL DEFAULT FALSE,         -- 是否启用
    issuer_url VARCHAR(500) NOT NULL,               -- ElevoOne base URL
    client_id VARCHAR(255) NOT NULL,                -- ElevoOne product_key
    client_secret_encrypted TEXT,                   -- AES-256-GCM 加密存储
    redirect_uri VARCHAR(500),                      -- 回调地址（可自动推导）
    jwks_refresh_interval_secs INT NOT NULL DEFAULT 3600,
    disable_password_login BOOLEAN NOT NULL DEFAULT FALSE,  -- 是否禁用密码登录
    auto_create_tenant BOOLEAN NOT NULL DEFAULT FALSE,      -- 是否自动创建 tenant
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
INSERT INTO oidc_config (id, enabled, issuer_url, client_id, client_secret_encrypted, redirect_uri)
VALUES (1, FALSE, '', '', '', '');
```

> **为什么用 singleton（CHECK id = 1）而不是通用 key-value 表**：
> OIDC 配置是一组强类型字段（URL、密钥等），singleton 模式能保证类型安全和字段完整性，比 `system_settings(key, value)` 更不容易出错。如果后续有更多全局配置需求，再抽为通用方案。

**client_secret 加密**：

| 项目 | 说明 |
|------|------|
| 加密算法 | AES-256-GCM |
| 加密密钥 | 从 `OIDC_SECRET_ENCRYPTION_KEY` 或 HKDF 派生 |
| 存储格式 | `base64(iv || ciphertext || tag)` |
| 读取 | 后端启动时解密到内存，OidcService 持有明文 |
| 写入 | 管理员保存时加密后写入 DB |
| 页面展示 | 密钥字段始终显示为 `••••••••`，仅修改时填写新值 |

**配置加载与热更新**：

- 服务启动时从 DB 读取 `oidc_config` 表
- 管理员通过 API 修改配置后，`OidcService` 内存状态立即刷新
- `disable_password_login` 和 `auto_create_tenant` 也通过此表管理，即时生效
- JWKS 公钥在启用 OIDC 后首次自动拉取，后续定时刷新

**运行时结构**：

```rust
// server/src/infra/oidc.rs
pub struct OidcService {
    issuer_url: String,
    client_id: String,
    client_secret: String,     // 内存中持有明文
    redirect_uri: String,
    enabled: bool,
    disable_password_login: bool,
    auto_create_tenant: bool,
    jwks: Arc<RwLock<JwksKeySet>>,
    refresh_interval: Duration,
    circuit_breaker: OidcCircuitBreaker,  // 熔断计数器（见 5.5 节）
}
```

#### 3.1.3 Admin 后台配置页面

```
Admin 后台 → 系统设置 → SSO 配置

┌──────────────────────────────────────────┐
│  SSO 单点登录配置                        │
│                                          │
│  启用 SSO:  [■ 开关]                      │
│                                          │
│  Issuer URL:  [https://elevoone.xxx.com ]│
│  Client ID:  [pk_xxxxxxxx            ]   │
│  Client Secret: [••••••••        ] [编辑] │
│  回调地址:   [https://ws.xxx.com/api/v1 │
│               /auth/oidc/callback    ]   │  ← 自动推导，只读
│                                          │
│  [测试连接]              [保存]          │
│                                          │
│  状态: ✅ JWKS 连接正常                    │
│                                          │
│  ───── 高级设置 ─────                     │
│  禁用密码登录: [□ 开关]                   │  ← 启用 SSO 后可选
│  自动创建租户: [□ 开关]                   │  ← org_id 无映射时自动创建
└──────────────────────────────────────────┘
```

- **测试连接**：调用 `GET {issuer_url}/.well-known/openid-configuration` 验证连通性，并预拉取 JWKS
- **回调地址**：根据当前 `WORKSPACE_HTTP_HOST`/`WORKSPACE_HTTP_PORT` 自动推导，管理员也可手动修改（用于反向代理场景）
- **启用/禁用**：开关控制 OIDC 功能启停，禁用后登录页不显示 SSO 按钮
- **禁用密码登录**：仅在 OIDC 已启用时可设置，防止管理员把自己锁在外面
- **自动创建租户**：当 ElevoOne 用户首次通过 OIDC 访问 API，但其 org 无关联 tenant 时，自动创建

### 3.2 数据库变更

#### 3.2.1 oidc_config 表

见 3.1.2 节。

#### 3.2.2 tenants 表新增字段

> **类型说明**：`elevoone_org_id` 使用 `BIGINT` 类型，假设 ElevoOne 的 org_id 为整型 ID（如 snowflake ID）。如果 ElevoOne 的 org_id 实际为 UUID，需改为 `UUID` 类型。此类型需与 ElevoOne 团队确认。

```sql
-- 建立 ElevoOne 组织与本地 tenant 的映射关系
ALTER TABLE tenants ADD COLUMN elevoone_org_id BIGINT;
CREATE UNIQUE INDEX idx_tenants_elevoone_org_id ON tenants(elevoone_org_id) WHERE elevoone_org_id IS NOT NULL;
```

#### 3.2.2a audit_logs 表 actor_type 约束扩展

现有 `audit_logs` 表的 `actor_type` 有 CHECK 约束 `CHECK(actor_type IN ('admin', 'tenant'))`，OIDC 审计事件需要新增 `anonymous` 类型（用于记录未认证的登录失败事件）。需执行以下迁移：

```sql
ALTER TABLE audit_logs DROP CONSTRAINT audit_logs_actor_type_check;
ALTER TABLE audit_logs ADD CONSTRAINT audit_logs_actor_type_check
    CHECK(actor_type IN ('admin', 'tenant', 'anonymous'));
```

#### 3.2.3 oidc_auth_sessions 表（登录流程状态）

存储 OIDC Authorization Code Flow 过程中的临时状态，生命周期短（从授权发起 → 回调完成）：

```sql
CREATE TABLE oidc_auth_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    state VARCHAR(128) NOT NULL,              -- CSRF 防护（后端生成）
    nonce VARCHAR(128) NOT NULL,              -- ID Token 防重放（后端生成）
    code_verifier VARCHAR(128) NOT NULL,      -- PKCE verifier（后端生成）
    consumed BOOLEAN NOT NULL DEFAULT FALSE,  -- 是否已消费（回调验证后标记，替代物理删除）
    ip_address INET,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL           -- 整条记录过期时间（10分钟）
);

CREATE UNIQUE INDEX idx_oidc_auth_sessions_state ON oidc_auth_sessions(state);
```

> **为什么 state/nonce/code_verifier 由后端生成**：
> 本方案采用后端回调（redirect_uri 指向后端），后端需要验证 state。如果 state 由前端生成存在 sessionStorage 中，后端无法获取前端状态来验证。因此必须由后端生成并存储，前端仅负责跳转。

#### 3.2.4 oidc_token_store 表（长期 Token 存储）

存储 OIDC 登录成功后的 ElevoOne tokens，用于 logout 和 token 刷新，生命周期较长：

```sql
CREATE TABLE oidc_token_store (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL,                  -- ElevoOne user ID (sub)
    org_id BIGINT,                             -- ElevoOne organization ID
    org_role VARCHAR(50),                      -- ElevoOne org role (admin/member)
    email VARCHAR(255),
    name VARCHAR(255),
    picture VARCHAR(1024),
    id_token TEXT NOT NULL,                    -- ElevoOne ID Token（用于 logout）
    refresh_token TEXT,                        -- ElevoOne Refresh Token
    access_token TEXT,                         -- ElevoOne Access Token
    local_session_id UUID NOT NULL,            -- 关联的本地 JWT session_id
    -- session_code 相关字段（一次性兑换码，回调成功后生成，前端换取 token 时消费）
    session_code VARCHAR(64),                  -- 随机 32 字节 hex 编码
    session_code_expires_at TIMESTAMPTZ,       -- session_code 过期时间（30 秒）
    session_code_consumed BOOLEAN NOT NULL DEFAULT FALSE,
    ip_address INET,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,          -- 基于 ElevoOne refresh_token 有效期
    last_refreshed_at TIMESTAMPTZ
);

CREATE INDEX idx_oidc_token_store_local_session_id ON oidc_token_store(local_session_id);
CREATE INDEX idx_oidc_token_store_expires_at ON oidc_token_store(expires_at);
CREATE INDEX idx_oidc_token_store_user_org ON oidc_token_store(user_id, org_id);
CREATE UNIQUE INDEX idx_oidc_token_store_session_code ON oidc_token_store(session_code) WHERE session_code IS NOT NULL;
```

> **设计决策 — 拆分为两张表的原因**：
> 1. **职责分离**：`oidc_auth_sessions` 管理登录流程的临时状态（state、code_verifier），`oidc_token_store` 管理长期 token 存储 + 一次性 session_code 兑换
> 2. **清理策略不同**：auth_sessions 短生命周期（10分钟），可以激进清理；token_store 生命周期较长（7天），需要保留用于 refresh
> 3. **数据敏感度不同**：auth_sessions 包含 PKCE verifier，应尽早清理；token_store 包含 refresh_token，需要持久保存
> 4. **session_code 存储在 token_store 中**：session_code 天然关联 token_store 记录（前端用 session_code 换取的就是 token_store 中存储的本地 JWT），因此 session_code 字段放在 token_store 表中比放在 auth_sessions 中更合理。这避免了在 auth_sessions 中用随机占位值填充 state/nonce/code_verifier 的问题
>
> **为什么用 DB 而不是 Redis 存 OIDC session**：
> 1. ElevoWorkspace 当前没有 Redis 依赖，引入 Redis 仅为此功能增加运维复杂度
> 2. OIDC session 数据量不大（用户量级），PostgreSQL 完全可以承载
> 3. 需要与 tenants 表做 JOIN 查询（org_id → tenant 映射），DB 更方便
> 4. session 有明确的创建/过期时间，可定期清理

#### 3.2.5 users 表（可选，按需新增）

> 如果后续需要独立于 tenant 的用户管理能力，可以新增 users 表。首期方案不新增，用户身份完全依赖 ElevoOne 的 ID Token claims。

### 3.3 OIDC 登录流程（管理后台）

> 管理后台 OIDC 登录仅对 `org_role=admin` 的用户开放。`org_role=member` 的用户通过 ElevoOne access_token 直接调用 API（见 3.4 节中间件改造）。

#### 3.3.1 前端发起登录

```
用户点击 "SSO 登录" 按钮
  │
  ├─ 前端调用: POST /api/v1/auth/oidc/authorize
  │
  ├─ 后端处理:
  │   1. 检查 OIDC 是否启用
  │   2. 生成 state（随机字符串，CSRF 防护）
  │   3. 生成 nonce（随机字符串，ID Token 防重放）
  │   4. 生成 code_verifier（PKCE S256）
  │   5. 计算 code_challenge = BASE64URL(SHA256(code_verifier))
  │   6. 将 state/nonce/code_verifier 存入 oidc_auth_sessions 表
  │   7. 拼接授权 URL
  │   8. 返回: { authorize_url: "https://elevoone.xxx/oauth/authorize?..." }
  │
  └─ 前端执行 window.location.href = authorize_url
     用户在 ElevoOne 完成认证
```

> **为什么由后端生成 state/nonce/code_verifier**：
> 本方案采用后端回调（redirect_uri 指向后端 `/api/v1/auth/oidc/callback`），后端需要验证 state 和 code_verifier。如果由前端生成存入 sessionStorage，后端无法读取前端状态。因此必须由后端生成、存储、验证。

#### 3.3.2 后端回调处理

```
ElevoOne 回调: GET /api/v1/auth/oidc/callback?code=xxx&state=yyy
  │
  ├─ 1. 查询 oidc_auth_sessions 表，验证 state 是否存在且未过期且未消费
  │     ├─ state 不存在或已过期 → 返回错误（可能是 CSRF 攻击或过期）
  │     ├─ state 已消费（consumed=true）→ 返回错误（防重放）
  │     └─ 标记 consumed=true（原子操作：UPDATE ... SET consumed=true WHERE state=? AND NOT consumed RETURNING ...）
  │        影响 0 行 → 并发竞争，返回错误
  │        影响 1 行 → 消费成功
  │        > **为什么用 consumed 标记而非物理删除**：
  │        > 物理删除 state 后，如果 ElevoOne 因网络原因重试回调（同一 code + state），
  │        > 第二次请求会因为 state 不存在而直接失败。虽然标准 OIDC 流程中 code 只能用一次，
  │        > 但 state 删除和 code 消费之间有时间窗口。使用 consumed 标记可以：
  │        > 1. 防止重放（consumed=true 的记录不可复用）
  │        > 2. 保留审计记录（可追溯异常回调）
  │        > 3. 避免 state 删除与 code 使用之间的竞态问题
  │
  ├─ 2. 向 ElevoOne 交换 token:
  │     POST {issuer_url}/oauth/token
  │       grant_type=authorization_code
  │       code={authorization_code}
  │       redirect_uri={redirect_uri}
  │       client_id={client_id}
  │       client_secret={client_secret}
  │       code_verifier={auth_session.code_verifier}
  │
  │   响应: { access_token, id_token, refresh_token }
  │
  ├─ 3. 验证 ID Token（RS256 + JWKS）
  │     - 验证签名（通过缓存的 JWKS 公钥）
  │     - 验证 iss（等于 issuer_url）
  │     - 验证 aud（等于 client_id）
  │     - 验证 exp（未过期）
  │     - 验证 nonce（等于 auth_session.nonce，防重放）
  │     - 提取 claims: sub, email, name, picture, org_id, org_role
  │
  ├─ 4. 判断身份（仅 admin 可进入管理后台）:
  │     ├─ org_role = "admin" → 签发本地 admin JWT (sub="admin")
  │     │   - 与现有 create_admin_token 一致，session_id 关联到 token_store
  │     │   - 存储 ElevoOne tokens 到 oidc_token_store 表
  │     │
  │     └─ org_role = "member" → 不签发本地 token，不存储 session
  │         但仍重定向到激活成功页（见下方说明）
  │
  │     > **member 用户"激活"机制**：
  │     > ElevoOne 在 authorization_code 流程中（步骤 2 code 换 token 时）会自动创建
  │     > `user_product_associations` 记录。这意味着 member 用户即使被管理后台拒绝，
  │     > 其 `user_product_associations` 记录也已经创建。之后该用户可以通过 ElevoOne
  │     > Token Exchange（RFC 8693）获取 Workspace 专属 token，进而调用 Workspace API。
  │     > 这是一个一次性的"激活"步骤——member 用户只需访问一次 SSO 登录页，
  │     > 之后无需再重复。
  │     >
  │     > **行为依赖说明**：此"激活"机制依赖 ElevoOne 在 authorization_code 流程中自动
  │     > 创建 `user_product_associations` 的内部行为。如果 ElevoOne 未来修改此行为，
  │     > Workspace 的 member 激活机制会静默失效。建议与 ElevoOne 团队确认此行为是否为
  │     > 稳定的 API 契约（而非内部实现细节），并在 ElevoOne 变更日志中关注相关改动。
  │     >
  │     > 完整的 Token Exchange 前置条件（详见 3.10.2 节）：
  │     > 1. 用户的 org 在 ElevoOne 后台开通了 Workspace 产品（`organization_products` 表，`status='active'`）
  │     > 2. 用户是该 org 的成员（`organization_members` 表）
  │     > 3. 用户已通过 SSO 登录过 Workspace（`user_product_associations` 表）← 步骤 2 自动创建
  │     > 4. org_id 在 Workspace 中有 tenant 映射（`tenants.elevoone_org_id`）
  │
  ├─ 5. 生成一次性 session_code（写入 oidc_token_store 表）
  │     - 更新第 4 步创建的 oidc_token_store 记录：
  │       session_code = 随机 32 字节 hex 编码
  │       session_code_expires_at = NOW() + 30s
  │       session_code_consumed = false
  │     - 前端有 30 秒窗口消费此 code
  │
  └─ 6. 重定向: 307 → /admin/login/success?code={session_code}
```

#### 3.3.3 前端获取 Token

```
/admin/login/success 页面加载
  │
  ├─ 前端从 URL 提取 code 参数
  │
  ├─ 前端调用: GET /api/v1/auth/oidc/session?code={session_code}
  │
  ├─ 后端处理:
  │   1. 查询 oidc_token_store，验证 session_code：
  │      - code 不存在 → 返回错误
  │      - code 已消费（session_code_consumed=true）→ 返回错误（防重放）
  │      - code 已过期（session_code_expires_at < NOW）→ 返回错误
  │   2. 原子消费：UPDATE ... SET session_code_consumed = true WHERE session_code = ? AND NOT session_code_consumed RETURNING ...
  │      - 影响 0 行 → 并发竞争，返回错误
  │      - 影响 1 行 → 消费成功，同时获取完整的 oidc_token_store 记录
  │   3. 返回: { token: "本地JWT", user: { name, email, picture, is_admin: true } }
  │
  ├─ 前端存储 token 到 localStorage
  └─ 前端导航到 /admin/dashboard
```

> 为什么不直接把本地 JWT 放在 URL 参数里？
> - JWT 可能较长，放在 URL 中可能超出长度限制
> - URL 中的 token 会被浏览器历史记录、Referer 头、日志等记录，有泄露风险
> - 一次性 session_code 更安全（30 秒过期、一次性消费、原子操作防并发）

#### 3.3.4 错误处理

回调过程中的各类错误，统一重定向到前端错误页，附带错误码：

| 场景 | 重定向 | 前端展示 |
|------|--------|---------|
| state 不存在或已过期 | `/admin/login?error=invalid_state` | "登录请求已过期，请重试" |
| state 已消费（重放攻击） | `/admin/login?error=invalid_state` | "登录请求已过期，请重试" |
| code 换 token 失败 | `/admin/login?error=token_exchange_failed` | "认证服务通信失败，请稍后重试" |
| ID Token 验证失败 | `/admin/login?error=invalid_token` | "身份验证失败，请联系管理员" |
| nonce 不匹配 | `/admin/login?error=invalid_token` | "身份验证失败，请重试" |
| org_role=member | `/admin/login?activated=true` | "账号已激活，可通过 ElevoOne Token 访问 Workspace API" |
| session_code 过期/已消费 | `/admin/login/success?error=session_expired` | "登录会话已过期，请重新登录" |
| ElevoOne 返回 error 参数 | `/admin/login?error=sso_error&desc={description}` | 展示 ElevoOne 返回的错误描述 |

### 3.4 Auth 中间件改造

#### 3.4.1 现有中间件分析

当前 `auth_middleware`（`server/src/api/http/auth.rs`）的分发逻辑：

```
Token 前缀判断:
  "sk_" → API Key 认证 → Identity::Tenant
  其他   → 本地 HS256 JWT 认证（仅接受 sub="admin"）→ Identity::Admin
```

关键约束：`verify_jwt()` 硬编码 `validation.sub = Some("admin".to_string())`，只有 `sub="admin"` 的 HS256 JWT 才能通过验证。这个约束**不需要修改**，因为 OIDC 登录的 admin 用户也会获得 `sub="admin"` 的本地 JWT。

#### 3.4.2 改造后的分发逻辑

由于本地 JWT (HS256, `alg: "HS256"`) 和 ElevoOne JWT (RS256, `alg: "RS256"`) 都是 `eyJ` 开头，需要先区分算法再选择验证路径。

**方案：按 JWT header 的 `alg` 字段区分**

```rust
// 在 auth_middleware 中，JWT 路径的处理改为：
// 1. 先解码 JWT header（base64url 解码第一段，不验证签名）
// 2. 根据 alg 字段选择验证路径
//    - "HS256" → 本地 secret 验证（现有 verify_jwt 逻辑不变）
//    - "RS256" → ElevoOne JWKS 验证（新增路径）
//    - 其他 → AuthError::InvalidToken
```

> **为什么选择按 `alg` 区分而不是"先尝试 HS256 失败再 RS256"**：
> 1. **性能**：按 `alg` 分发只需一次 base64url 解码（纯内存操作），而"先尝试后失败"方案在遇到 ElevoOne token 时会先做一次无效的 HS256 验证
> 2. **确定性**：每种 token 走明确的路径，不会因为本地 secret 泄露导致 ElevoOne token 被错误验证
> 3. **延迟可控**：RS256 验证可能触发 JWKS 网络请求，不应作为 fallback 路径
>
> **安全说明**：JWT header 的 `alg` 字段本身不可信（任何人可以伪造），但当 header 声称 HS256 却验证失败，或声称 RS256 却 JWKS 验证失败时，统一返回 `AuthError::InvalidToken("invalid or expired token")`，不泄露具体验证路径。

完整分发逻辑：

```
Token 前缀判断:
  "sk_"                        → API Key 认证 → Identity::Tenant
  "eyJ..." + alg=HS256         → 本地 JWT 认证 → Identity::Admin
  "eyJ..." + alg=RS256         → ElevoOne Token 认证 → Identity::Tenant
  其他                         → AuthError::InvalidToken
```

#### 3.4.3 ElevoOne Token 认证路径

```rust
async fn authenticate_elevoone_token(
    state: &AppState,
    token: &str,
    ip_address: Option<IpAddr>,
) -> Result<AuthContext, AuthError> {
    // 1. 检查 OIDC 是否已启用
    let oidc_service = state.oidc_service.as_ref()
        .ok_or(AuthError::InvalidToken("OIDC not enabled".into()))?;

    // 2. 通过 JWKS 验证 RS256 签名 + 标准 claims
    let claims = oidc_service.verify_access_token(token)?;

    // 3. 检查 aud 是否匹配本产品的 client_id
    if claims.aud != oidc_service.client_id() {
        return Err(AuthError::InvalidToken("invalid or expired token".into()));
    }

    // 4. 通过 org_id 查找关联的 tenant
    let tenant = state.tenant_repository
        .find_by_elevoone_org_id(claims.org_id)
        .await
        .map_err(|e| AuthError::Internal(format!("DB error: {}", e)))?
        .ok_or(AuthError::InvalidToken("invalid or expired token".into()))?;

    if !tenant.is_active {
        return Err(AuthError::TenantDeactivated);
    }

    // 5. 构造 AuthContext
    Ok(AuthContext {
        identity: Identity::Tenant {
            id: tenant.id,
            name: tenant.name,
        },
        ip_address,
    })
}
```

> **错误信息统一化**：步骤 3 和 5 中的错误都返回 `AuthError::InvalidToken("invalid or expired token")`，不暴露具体原因（如 aud mismatch、no tenant mapping），防止信息泄露帮助攻击者探测系统配置。

#### 3.4.4 中间件改造对现有代码的影响

| 改动点 | 影响 |
|--------|------|
| `auth_middleware` 函数 | 新增 JWT header 解析和 alg 分支，提取为独立的 `route_jwt` 函数 |
| `verify_jwt()` | **不需要修改**，继续只接受 `sub="admin"` |
| `authenticate_api_key()` | **不需要修改** |
| `create_admin_token()` | **不需要修改**，OIDC admin 用户使用同一函数签发 JWT |
| `AuthError` 枚举 | **不需要修改**，复用现有的 `InvalidToken`、`TenantDeactivated` 等 |

#### 3.4.5 gRPC 认证层同步改造

gRPC 认证层（`server/src/api/grpc/auth.rs`）是独立于 HTTP 的并行实现，也需要同步支持 ElevoOne Token。两者的认证路由逻辑完全对称：

**当前 gRPC 路由**：
```
sk_*         → API Key → GrpcIdentity::Tenant
其他         → HS256 JWT (verify_jwt_public) → GrpcIdentity::Admin
```

**改造后 gRPC 路由**：
```
sk_*              → API Key → GrpcIdentity::Tenant
eyJ... + alg=HS256 → 本地 JWT → GrpcIdentity::Admin
eyJ... + alg=RS256 → ElevoOne Token → GrpcIdentity::Tenant
```

**改动内容**：

1. `GrpcAuthLayer` / `GrpcAuthService` 新增 `oidc_service: Option<Arc<OidcService>>` 字段
2. `GrpcAuthLayer::new()` 新增 `oidc_service` 参数
3. `call()` 中 JWT 路径改为按 `alg` 分发（与 HTTP auth_middleware 一致）
4. RS256 分支调用 `OidcService::verify_and_resolve_tenant()`（公共方法，与 HTTP 层共用）

```rust
// GrpcAuthService::call() 中 RS256 分支
Ok("RS256") => {
    let oidc = match oidc_service.as_ref() {
        Some(s) => s,
        None => return Ok(unauthenticated_response("OIDC not enabled")),
    };
    match oidc.verify_and_resolve_tenant(token, &tenant_repo).await {
        Ok(tenant_id) => {
            req.extensions_mut()
                .insert(GrpcIdentity::Tenant { tenant_id });
            return inner.call(req).await;
        }
        Err(_) => return Ok(unauthenticated_response("invalid or expired token")),
    }
}
```

5. `main.rs` 初始化 `GrpcAuthLayer` 时传入 `state.oidc_service.clone()`

**认证逻辑复用**：

将 RS256 验证 + org_id 查 tenant 的逻辑提取为 `OidcService` 的公共方法，HTTP 和 gRPC 共用：

```rust
// server/src/infra/oidc.rs
impl OidcService {
    /// 验证 ElevoOne token 并解析为本地 tenant 身份
    /// HTTP auth_middleware 和 gRPC GrpcAuthService 都调用此方法
    pub async fn verify_and_resolve_tenant(
        &self,
        token: &str,
        tenant_repo: &TenantRepository,
    ) -> Result<Uuid, OidcError> {
        let claims = self.verify_access_token(token).await?;
        // aud 匹配 + org_id 查 tenant + 激活检查
        // 返回 tenant.id
    }
}
```

同样，JWT header `alg` 解析的 `extract_jwt_alg()` 工具函数也提取为公共函数，HTTP 和 gRPC 共用。

**不需要额外处理的事**：

| 事项 | 原因 |
|------|------|
| `GrpcIdentity` 枚举不需要新增变体 | ElevoOne Token 认证后解析为 `Tenant { tenant_id }`，与 API Key 结果一致 |
| `GrpcIdentity::Admin` 不需要 `session_id` | gRPC 的 `GrpcIdentity::Admin` 本身就没有 `session_id` 字段（与 HTTP 的 `Identity::Admin { session_id }` 不同），gRPC 不处理 logout/refresh 等 session 相关操作，因此不需要 `session_id` |
| `GrpcIdentity::Tenant` 没有 `name` 字段 | gRPC 的 `GrpcIdentity::Tenant` 本身只有 `tenant_id`，没有 `name`，这与现有设计一致，OIDC 不需要改变这一点 |
| gRPC Token 刷新 | gRPC 只做认证，不管刷新。access_token 过期后由调用方（SDK/后端服务）自行通过 ElevoOne 重新获取 |
| gRPC Logout | gRPC 是无状态的，没有 session 概念，logout 只在 HTTP 层处理 |
| SDK 改动 | SDK 只是把 token 字符串放进 `Authorization: Bearer` header，不解析 token，无需任何改动 |

### 3.5 JWKS 密钥管理

```rust
// server/src/infra/oidc.rs

pub struct OidcService {
    issuer_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    enabled: bool,
    jwks: Arc<RwLock<JwksKeySet>>,     // 缓存 JWKS 公钥
    jwks_last_refresh: Arc<AtomicI64>,  // 上次刷新时间戳
    refresh_interval: Duration,
    circuit_breaker: OidcCircuitBreaker,
}

impl OidcService {
    /// 异步刷新 JWKS 公钥（带并发保护）
    pub async fn refresh_jwks_if_needed(&self) -> Result<(), OidcError> {
        let now = Utc::now().timestamp();
        let last_refresh = self.jwks_last_refresh.load(Ordering::Relaxed);
        let interval = self.refresh_interval.as_secs() as i64;

        // 上次刷新距今不足一半间隔 → 不需要刷新
        if now - last_refresh < interval / 2 {
            return Ok(());
        }

        // 使用 try_write 避免阻塞：如果已有写锁则跳过（其他任务正在刷新）
        {
            let mut jwks = match self.jwks.try_write() {
                Ok(guard) => guard,
                Err(_) => return Ok(()),  // 其他任务正在刷新，等待其完成即可
            };

            let url = format!("{}/.well-known/jwks.json", self.issuer_url);
            let resp = reqwest::get(&url).await?;
            let new_jwks: JwksKeySet = resp.json().await?;
            *jwks = new_jwks;
            self.jwks_last_refresh.store(now, Ordering::Relaxed);
        }

        Ok(())
    }

    /// 验证 ElevoOne access_token（RS256 + JWKS）
    pub async fn verify_access_token(
        &self,
        token: &str,
    ) -> Result<ElevoOneClaims, OidcError> {
        // 1. 解析 JWT header 获取 kid
        let kid = extract_kid(token)?;

        // 2. 检查 kid 是否在缓存中，不在则触发刷新
        {
            let jwks = self.jwks.read().await;
            if !jwks.contains_kid(&kid) {
                drop(jwks);  // 释放读锁
                self.force_refresh_jwks().await?;
            }
        }

        // 3. 重新获取读锁（可能已被上面的刷新更新）
        let jwks = self.jwks.read().await;
        let key = jwks.get_key_by_kid(&kid)
            .ok_or(OidcError::KidNotFound(kid))?;

        let claims = jsonwebtoken::decode::<ElevoOneClaims>(
            token,
            &jsonwebtoken::DecodingKey::from_rsa_components(key.n, key.e)?,
            &validation,
        )?.claims;

        Ok(claims)
    }

    /// 强制刷新 JWKS（kid 不存在时触发）
    async fn force_refresh_jwks(&self) -> Result<(), OidcError> {
        // 使用 try_write 避免并发请求同时刷新
        let mut jwks = match self.jwks.try_write() {
            Ok(guard) => guard,
            Err(_) => {
                // 其他请求正在刷新，短暂等待后重新获取读锁尝试验证
                // 而非直接返回 Ok(())，因为 kid 可能确实不在旧缓存中，
                // 直接返回会导致本次 token 验证不必要地失败
                tokio::time::sleep(Duration::from_millis(100)).await;
                // 验证逻辑在 verify_access_token 中会重新获取读锁并尝试验证，
                // 此时其他任务的刷新应该已经完成
                return Ok(());
            }
        };

        let url = format!("{}/.well-known/jwks.json", self.issuer_url);
        let resp = reqwest::get(&url).await?;
        let new_jwks: JwksKeySet = resp.json().await?;
        *jwks = new_jwks;
        self.jwks_last_refresh.store(Utc::now().timestamp(), Ordering::Relaxed);

        Ok(())
    }

    /// 用 authorization code 换取 token
    pub async fn exchange_code(&self, code: &str, code_verifier: &str) -> Result<TokenResponse, OidcError> {
        // POST {issuer}/oauth/token
    }

    /// 刷新 access_token
    pub async fn refresh_elevoone_token(&self, refresh_token: &str) -> Result<TokenResponse, OidcError> {
        // POST {issuer}/oauth/token (grant_type=refresh_token)
    }
}
```

**JWKS 刷新策略**：

| 触发条件 | 行为 |
|---------|------|
| 服务启动 + OIDC 已启用 | 首次获取 JWKS |
| 定时任务（默认 1 小时） | 后台刷新（`refresh_jwks_if_needed`） |
| 验证 token 时 kid 不在缓存中 | 立即强制刷新（`force_refresh_jwks`） |
| 刷新失败 | 记录告警日志，使用旧缓存（降级），不影响本地 JWT 和 API Key 认证 |

**并发控制**：
- 定时刷新使用 `try_write`，如果已有写锁（说明其他任务正在刷新），直接跳过
- 按需刷新（kid 不存在时）使用 `force_refresh_jwks`，获取写锁后刷新
- `jwks_last_refresh` 使用 `AtomicI64` 记录刷新时间，用于减少不必要的锁竞争

### 3.6 前端改造

#### 3.6.1 登录页（Login.tsx）

```
┌──────────────────────────────┐
│      Elevo 管理后台           │
│                              │
│   ┌──────────────────────┐   │
│   │  SSO 登录             │   │  ← 新增：ElevoOne OIDC 登录按钮
│   │  (点击跳转到统一登录)  │   │     仅当 OIDC 已启用时显示
│   └──────────────────────┘   │
│                              │
│   ───── 或密码登录 ─────      │  ← 保留：仅在 disable_password_login=false 时显示
│                              │
│   [ 管理员密码 __________ ]   │
│   [        登录        ]     │
└──────────────────────────────┘
```

- **SSO 登录按钮**：页面加载时调用 `GET /api/v1/auth/oidc/config` 获取配置状态，`enabled=true` 时显示
- **密码登录**：根据 `config.disable_password_login` 决定是否显示
- 前端不生成 state/nonce/code_verifier，全部由后端管理

#### 3.6.2 新增路由

| 路由 | 说明 |
|------|------|
| `/admin/login` | 登录页（已有，添加 SSO 按钮） |
| `/admin/login/success` | OIDC 登录成功页（从 URL 提取 code 参数，调用后端换取 token） |
| `/admin/settings` | 系统设置页（新增，包含 SSO 配置） |

> **不再需要 `/admin/oidc-callback` 前端路由**：回调直接由后端处理（`GET /api/v1/auth/oidc/callback`），后端 307 重定向到 `/admin/login/success?code=xxx`。前端只需处理 success 页面。

#### 3.6.3 前端 OIDC 登录 API 调用

```typescript
// web/src/api/auth.ts 新增

// 获取 OIDC 配置状态（登录页判断是否显示 SSO 按钮）
export async function getOidcConfig(): Promise<{
  enabled: boolean;
  disable_password_login: boolean;
}> { ... }

// 发起 OIDC 授权（后端生成 state/nonce/code_verifier，返回授权 URL）
export async function authorizeOidc(): Promise<{ authorize_url: string }> { ... }

// 用一次性 session_code 换取本地 JWT
export async function exchangeSessionCode(code: string): Promise<{
  token: string;
  user: { name: string; email: string; picture: string; is_admin: boolean };
}> { ... }
```

#### 3.6.4 登录页显示逻辑

```typescript
const config = await getOidcConfig();

const showSsoButton = config.enabled;
const showPasswordLogin = !config.disable_password_login;

if (!showSsoButton && !showPasswordLogin) {
  // 理论上不应发生（后端校验会阻止 disable_password_login 在 OIDC 未启用时设置 true）
  showError("系统配置错误：没有可用的登录方式，请联系管理员");
}
```

### 3.7 Logout 流程

```
用户点击 "退出登录"
  │
  ├─ 1. 前端调用 POST /api/v1/auth/logout
  │     - Header: Authorization: Bearer <本地_JWT>
  │     - 后端从 JWT 中提取 session_id
  │     - 查询 oidc_token_store 找到关联的 ElevoOne tokens
  │     - 删除 oidc_token_store 记录
  │     - 返回 { idp_logout_url: "ElevoOne end_session URL" } 或 {}
  │
  ├─ 2. 前端清除本地状态 (localStorage)
  │
  └─ 3. 如果返回了 idp_logout_url，前端重定向到该 URL:
        GET {issuer_url}/oauth/end_session?
          id_token_hint={elevoone_id_token}&
          post_logout_redirect_uri={workspace_url}/admin/login&
          client_id={client_id}

     如果没有返回 idp_logout_url（密码登录或 OIDC 记录已过期）：
        直接导航到 /admin/login
```

### 3.8 Token 自动刷新

ElevoOne access_token 有效期 300 秒（5 分钟），refresh_token 有效期 7 天。

**管理后台的刷新策略**：

- 本地 admin JWT 采用滑动刷新机制（已有，24 小时有效期，剩余 1/3 时通过 `X-Refreshed-Token` header 自动刷新）
- ElevoOne tokens 的刷新对管理后台用户是**静默的**：前端定时调用刷新接口，保持 ElevoOne tokens 有效（用于 logout 和审计）

新增接口：

```
POST /api/v1/auth/oidc/refresh
Authorization: Bearer <本地_admin_JWT>
  │
  ├─ 从 JWT 提取 session_id
  ├─ 查询 oidc_token_store（通过 local_session_id）
  ├─ 使用 ElevoOne refresh_token 换新 tokens
  ├─ 更新 oidc_token_store 记录
  └─ 返回成功（本地 JWT 不变，ElevoOne tokens 静默更新）
```

前端每 4 分钟定时调用一次（在 ElevoOne access_token 5 分钟有效期到期前）。

**前端刷新容错策略**：

- **成功时**：重置定时器为 4 分钟
- **失败时**：使用指数退避（30s → 60s → 120s → 4min），避免 ElevoOne 不可用时频繁重试
- **页面恢复前台时**：监听 `visibilitychange` 事件，页面从后台恢复时立即触发一次刷新（浏览器会节流后台标签页的 `setInterval`，可能导致刷新间隔被拉长到超过 token 有效期）
- **网络断开时**：使用 `navigator.onLine` 监听网络恢复事件，网络恢复后立即触发一次刷新

> **对于直接使用 ElevoOne Token 调用 API 的 Tenant 用户**：Token 刷新由调用方（如 SDK、后端服务）自行管理。Workspace 不参与 ElevoOne Token 的刷新逻辑。Token Exchange 得到的 access_token 过期后，调用方需要重新用原始 ElevoOne token 做一次 Token Exchange（详见 3.10.2 节）。

### 3.9 Tenant-Org 映射管理

#### 3.9.1 手动创建映射

管理员在 Admin 后台创建 tenant 时，可以关联一个 ElevoOne org_id：

```
创建 Tenant:
{
  "name": "示例公司",
  "description": "...",
  "elevoone_org_id": 456    ← 新增字段
}
```

#### 3.9.2 API 端点新增

```
GET  /api/v1/tenants?elevoone_org_id=456     ← 按 org_id 查询 tenant
PUT  /api/v1/tenants/:id                      ← 更新时支持修改 org_id
```

#### 3.9.3 自动创建（可选，页面配置控制）

在 SSO 配置页面开启「自动创建租户」开关后：

- 当 ElevoOne Token 认证路径中，用户 org_id 在 tenants 表中无映射时
- 自动创建 tenant（name 取 ID Token 或 userinfo 中的 org_name，如无 org_name 则取用户 email 的 `@` 前缀）
- 自动关联 `elevoone_org_id`
- `storage_type` 默认为 `managed`，`storage_config` 默认为 `{}`
  - **前提**：`managed` 类型需要后端存储（Docker volume path 等）已配置好。自动创建 tenant 前，应检查当前存储后端是否可用（如 Docker API 是否可达）。如果存储不可用，自动创建应失败并记录错误日志，而不是创建一个无法正常工作的 tenant
- 自动创建 API Key（方便 SDK 使用）
- 记录审计日志
- **并发保护**：`elevoone_org_id` 有唯一索引，并发创建时第二个事务会因唯一约束冲突失败，此时改为查询已存在的 tenant

> **关于 org_name 的来源**：org_name 不在 access_token 的标准 claims 中。管理后台 OIDC 登录流程可以从 ID Token 的自定义 claims 或 `/oauth/userinfo` 端点获取。Token Exchange 后的新 token 是否包含 org_name 需与 ElevoOne 团队确认。如果不可用，则退化为使用 email 前缀作为 tenant name，管理员后续可在后台修改。

---

### 3.10 外部系统访问 Workspace API

外部系统（elevo 产品、后台服务等）需要通过 ElevoOne 认证后才能调用 Workspace API。根据调用方类型，有两种方案：

| 调用方类型 | 推荐方案 | 适用场景 |
|-----------|---------|---------|
| 后台服务（M2M，无用户上下文） | **Client Credentials**（3.10.1） | 定时任务、数据同步、内部服务间调用 |
| 用户驱动的系统（浏览器/有用户登录态） | **Token Exchange**（3.10.2） | 用户在 elevo 产品中操作，需要访问 Workspace 数据 |

两种方案最终都是获取一个 `aud=workspace_client_id` 的 ElevoOne token，然后作为 `apiKey` 传入 Workspace SDK。SDK 本身无需任何改动。

> **为什么不跳过 aud 校验**：`aud` 是 JWT 最基本的防误用机制。跳过它意味着 ElevoOne 上任何产品的 token 都能访问 Workspace，无法区分哪些产品有权限，违反 OIDC 安全最佳实践。

#### 3.10.1 Client Credentials（M2M，后台服务）

后台服务没有浏览器、没有用户登录态，使用 ElevoOne 的 Client Credentials（RFC 6749）直接以**组织身份**获取 token。

**流程**：

```
后台服务                         ElevoOne                     Workspace
    │                                │                            │
    │── POST /oauth/token ──────────>│                            │
    │   grant_type=                  │                            │
    │     client_credentials          │                            │
    │   client_id=elevo_product_key  │                            │
    │   client_secret=elevo_secret   │                            │
    │   organization_id=456          │                            │
    │   audience=                    │                            │
    │     workspace_client_id        │                            │
    │                                │                            │
    │   ElevoOne 验证:               │                            │
    │   1. client_id/secret 合法      │                            │
    │   2. org 存在且 status=active  │                            │
    │   3. org-product M2M 授权      │                            │
    │      (allow_m2m=true)           │                            │
    │   4. 签发 access_token          │                            │
    │      (aud=workspace_client_id, │                            │
    │       包含 org_id)              │                            │
    │                                │                            │
    │<── { access_token, ... } ──────│                            │
    │                                │                            │
    │── gRPC/HTTP ──────────────────────────────────────────────>│
    │   Authorization: Bearer <token>                              │
    │                                │                            │
    │<── 200 OK ──────────────────────────────────────────────────│
```

**前置条件**（仅需一条，在 ElevoOne 后台配置一次）：

| 条件 | 对应的 ElevoOne 表 | 谁来配置 |
|------|-------------------|---------|
| org 开通 Workspace 产品且允许 M2M | `organization_products`（`status='active' AND allow_m2m=true`） | ElevoOne 管理员 |

> **与 Token Exchange 的区别**：Client Credentials 不需要用户参与，不需要 `user_product_associations`，不需要 SSO "激活"。它是纯组织级别的服务间授权。

**Go 示例**：

```go
// 后台服务启动时获取 token
resp, _ := http.PostForm(elevoneOneURL+"/oauth/token", url.Values{
    "grant_type":    {"client_credentials"},
    "client_id":     {elevoProductKey},
    "client_secret": {elevoSecret},
    "organization_id": {"456"},
    "audience":      {workspaceClientID},
})

var result struct {
    AccessToken string `json:"access_token"`
    ExpiresIn   int    `json:"expires_in"`
}
json.NewDecoder(resp.Body).Decode(&result)

// 直接使用，和 API Key 完全一样
client, _ := workspace.NewClient("workspace:9090", workspace.ClientOptions{
    APIKey: result.AccessToken,
})
```

**TypeScript 示例**：

```typescript
const resp = await fetch(`${elevoOneUrl}/oauth/token`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
  body: new URLSearchParams({
    grant_type: 'client_credentials',
    client_id: elevoProductKey,
    client_secret: elevoSecret,
    organization_id: '456',
    audience: workspaceClientID,
  }),
});
const { access_token } = await resp.json();

const client = new WorkspaceClient('workspace:9090', {
  apiKey: access_token,
});
```

**Token 刷新**：Client Credentials 签发的 access_token 有效期 1 小时（3600 秒），过期后重新请求即可，无需 refresh_token。

**错误处理**：

| ElevoOne 返回 | 含义 | 应对 |
|--------------|------|------|
| `invalid_client` | client_id/client_secret 错误 | 检查产品配置 |
| `invalid_grant` | org 不存在或未激活，或 org 未授权 M2M | 在 ElevoOne 后台开通 org-product M2M |
| `rate_limit_exceeded` | 请求频率超限 | 等待后重试 |

#### 3.10.2 Token Exchange（用户驱动场景）

当调用方代表一个已登录的 ElevoOne 用户操作时（如用户在 elevo 产品中点击"打开 Workspace"），使用 Token Exchange（RFC 8693）将用户的 elevo access_token 换成 Workspace 专属 token。

> **适用条件**：调用方持有用户的 ElevoOne access_token（用户已通过 SSO 登录到调用方系统）。如果调用方是纯后台服务、没有用户上下文，应使用 3.10.1 的 Client Credentials。

**流程**：

```
调用方（elevo 产品，代表用户）       ElevoOne                     Workspace
    │                                │                            │
    │  持有用户的 access_token       │                            │
    │  (aud=elevo_product_key)       │                            │
    │                                │                            │
    │── POST /oauth/token ──────────>│                            │
    │   grant_type=                  │                            │
    │     urn:ietf:params:oauth:     │                            │
    │     grant-type:token-exchange  │                            │
    │   subject_token=<access_token> │                            │
    │   subject_token_type=          │                            │
    │     urn:ietf:params:oauth:     │                            │
    │     token-type:access_token    │                            │
    │   audience=                    │                            │
    │     workspace_client_id        │                            │
    │   client_id=elevo_client_id    │                            │
    │   client_secret=elevo_secret   │                            │
    │   organization_id=456          │                            │
    │                                │                            │
    │   ElevoOne 验证:               │                            │
    │   1. subject_token 合法性       │                            │
    │   2. user_product_associations │                            │
    │   3. 用户是 org 成员            │                            │
    │   4. org-product 关联           │                            │
    │   5. 签发新 access_token        │                            │
    │      (aud=workspace_client_id, │                            │
    │       包含 org_id/org_role)    │                            │
    │                                │                            │
    │<── { access_token, ... } ──────│                            │
    │                                │                            │
    │── gRPC/HTTP ──────────────────────────────────────────────>│
    │   Authorization: Bearer <新token>                           │
    │                                │                            │
    │<── 200 OK ──────────────────────────────────────────────────│
```

**前置条件**（调用方和用户需确保以下条件全部满足）：

| # | 前置条件 | 对应的 ElevoOne 表 | 谁来配置 | 失败错误 |
|---|---------|-------------------|---------|---------|
| 1 | 用户的 org 开通了 Workspace 产品 | `organization_products`（`status='active'`） | ElevoOne 管理员 | `access_denied` |
| 2 | 用户是 org 的成员 | `organization_members` | ElevoOne 管理员 | `access_denied` |
| 3 | 用户已通过 SSO 登录过 Workspace | `user_product_associations` | 用户首次 SSO 登录时自动创建（见 3.3.2 步骤 4 说明） | `access_denied` |
| 4 | org_id 在 Workspace 中有 tenant 映射 | `tenants.elevoone_org_id` | Workspace 管理员 | Workspace 返回 401 |

> **条件 3 的"鸡和蛋"**：`user_product_associations` 只在 authorization_code flow（SSO 登录）中自动创建，Token Exchange 不会自动创建。但这恰好与 Workspace 的 member 用户激活机制配合：member 用户访问一次 Workspace SSO 登录页 → ElevoOne 完成认证并创建 `user_product_associations` → Workspace 回调检测到 `org_role=member`，不签发本地 token，但 `user_product_associations` 已存在。之后该用户即可正常 Token Exchange。
>
> 注意：调用方做 Token Exchange 时**必须携带 `organization_id` 参数**，否则 ElevoOne 只检查条件 3，不检查条件 1 和 2。建议始终携带 `organization_id` 以确保组织级权限校验。

**Go 示例**：

```go
// 1. 调用方已有的 ElevoOne access_token（用户已登录到 elevo 产品）
elevoToken := getElevoOneAccessToken()

// 2. 通过 ElevoOne Token Exchange 获取 Workspace 专属 token
wsToken, err := elevoone.ExchangeToken(elevoone.ExchangeTokenRequest{
    SubjectToken:     elevoToken,
    SubjectTokenType: "urn:ietf:params:oauth:token-type:access_token",
    Audience:         workspaceClientID,
    ClientID:         elevoClientID,
    ClientSecret:     elevoClientSecret,
    OrganizationID:   456,  // 必须携带
})
if err != nil {
    log.Fatal(err)
}

// 3. 和现在完全一样使用 SDK
client, _ := workspace.NewClient("workspace:9090", workspace.ClientOptions{
    APIKey: wsToken.AccessToken,
})
```

**TypeScript 示例**：

```typescript
const wsToken = await elevoone.exchangeToken({
  subjectToken: elevoToken,
  subjectTokenType: "urn:ietf:params:oauth:token-type:access_token",
  audience: workspaceClientID,
  clientId: elevoClientID,
  clientSecret: elevoClientSecret,
  organizationId: 456,  // 必须携带
});

const client = new WorkspaceClient("workspace:9090", {
  apiKey: wsToken.access_token,
});
```

**Token 刷新**：Token Exchange 得到的 access_token 有效期 5 分钟，过期后需要重新用原始 ElevoOne access_token 做一次 exchange。如果原始 token 也过期了，需先通过 ElevoOne refresh_token 刷新原始 token。**Workspace 不参与**调用方的 token 刷新链路。

**错误处理**：

| ElevoOne 返回 | 含义 | 调用方应做的 |
|--------------|------|------------|
| `invalid_grant` | subject_token 无效或过期 | 用 refresh_token 刷新原始 token 后重试 |
| `invalid_client` | client_id/client_secret 错误 | 检查产品配置 |
| `access_denied` | 用户/组织未授权访问 Workspace | 条件 1-3 不满足，引导用户先通过 SSO "激活"或联系管理员 |
| `rate_limit_exceeded` | Token Exchange 频率超限 | 等待后重试（默认 100 次/分钟） |
| `invalid_target` | audience（workspace_client_id）不存在或未激活 | 检查 Workspace 产品配置 |

#### 3.10.3 两种方案对比

| 维度 | Client Credentials | Token Exchange |
|------|-------------------|---------------|
| 适用场景 | 后台服务（M2M） | 用户驱动的系统 |
| 是否需要用户 | 否 | 是（需已登录 ElevoOne） |
| 是否需要浏览器 | 否 | 否（但首次需用户 SSO 激活） |
| 前置条件 | org-product M2M 授权（1 条） | org-product + org-member + user-product 关联（3 条） |
| Token 有效期 | 1 小时 | 5 分钟 |
| Token 中是否有 org_role | 否（仅 org_id） | 是（org_id + org_role） |
| Token 中是否有 user 信息 | 否 | 是（sub = user_id） |
| SDK 改动 | 无 | 无 |
| Workspace 侧改动 | 无 | 无（同一套 RS256 验证） |

---

## 4. 改动文件清单

### 4.1 后端（Rust）

| 文件 | 操作 | 说明 |
|------|------|------|
| `server/src/config.rs` | 修改 | 新增 `OIDC_SECRET_ENCRYPTION_KEY` 配置及其 HKDF 派生逻辑 |
| `server/src/domain/auth.rs` | **不修改** | Identity 枚举和 JwtClaims 不变，中间件通过路由分发解决 |
| `server/src/domain/tenant.rs` | 修改 | 新增 `elevoone_org_id: Option<i64>` 字段 |
| `server/src/infra/oidc.rs` | **新增** | OidcService：JWKS 缓存/刷新、token 验证、code 换 token、ElevoOne Token 刷新、熔断计数器；`verify_and_resolve_tenant()` 公共方法（HTTP + gRPC 共用）；`extract_jwt_alg()` 工具函数 |
| `server/src/infra/oidc_config_repository.rs` | **新增** | OIDC 配置 DB CRUD（singleton 模式）、client_secret 加解密 |
| `server/src/infra/oidc_auth_session_repository.rs` | **新增** | OIDC 登录流程状态（state/nonce/code_verifier/consumed）的 DB 操作 |
| `server/src/infra/oidc_token_store_repository.rs` | **新增** | OIDC Token 存储（ElevoOne tokens/refresh）及 session_code 兑换的 DB 操作 |
| `server/src/infra/mod.rs` | 修改 | 添加 oidc、oidc_config_repository、oidc_auth_session_repository、oidc_token_store_repository 模块 |
| `server/src/infra/tenant_repository.rs` | 修改 | 新增 `find_by_elevoone_org_id` 查询；行映射新增 `elevoone_org_id` 字段 |
| `server/src/api/http/oidc_handler.rs` | **新增** | OIDC 授权、回调、session 获取、refresh、logout、config 接口 |
| `server/src/api/http/oidc_config_handler.rs` | **新增** | OIDC 配置管理 API（GET/PUT + 测试连接），Admin 路由 |
| `server/src/api/http/auth.rs` | 修改 | auth_middleware 新增 JWT header 解析和 alg 分支（RS256 → ElevoOne 验证路径） |
| `server/src/api/http/auth_handler.rs` | 修改 | 新增 `POST /api/v1/auth/logout` 接口（OIDC logout 支持） |
| `server/src/api/http/mod.rs` | 修改 | 注册 OIDC 相关路由和配置管理路由 |
| `server/src/api/grpc/auth.rs` | 修改 | GrpcAuthLayer/GrpcAuthService 新增 `oidc_service` 字段；`call()` 新增 RS256 分支 |
| `server/src/main.rs` | 修改 | 初始化 OidcService（从 DB 加载配置）、启动 JWKS 刷新定时任务、oidc session 清理定时任务；GrpcAuthLayer 初始化传入 oidc_service |
| `server/Cargo.toml` | 修改 | 新增依赖: `reqwest` (HTTP client, rustls-tls)、`aes-gcm` + `hkdf` + `base64` (client_secret 加密) |
| `server/migrations/20260401000000_add_oidc.sql` | **新增** | DB 迁移：oidc_config、oidc_auth_sessions、oidc_token_store 表、tenants 新增 elevoone_org_id、audit_logs actor_type 约束扩展 |

### 4.2 前端（TypeScript/React）

| 文件 | 操作 | 说明 |
|------|------|------|
| `web/src/api/auth.ts` | 修改 | 新增 `getOidcConfig`、`authorizeOidc`、`exchangeSessionCode` |
| `web/src/api/oidcConfig.ts` | **新增** | OIDC 配置管理 API（get/update/test） |
| `web/src/pages/Login.tsx` | 修改 | 添加 SSO 登录按钮（根据配置状态显示/隐藏）、处理 URL 错误参数 |
| `web/src/pages/LoginSuccess.tsx` | **新增** | OIDC 登录成功页（从 URL 提取 code，调用后端换取 token） |
| `web/src/pages/settings/OidcSettings.tsx` | **新增** | SSO 配置管理页面 |
| `web/src/pages/settings/SystemSettings.tsx` | **新增** | 系统设置入口页面（包含 SSO 配置 Tab） |
| `web/src/stores/authStore.ts` | 修改 | 支持 OIDC 登录流程、ElevoOne token 定时刷新 |
| `web/src/router.tsx` | 修改 | 添加 `/admin/login/success`、`/admin/settings` 路由 |
| `web/src/api/client.ts` | 修改 | 401 时清除 OIDC 刷新定时器 |

### 4.3 依赖新增

**Rust (server/Cargo.toml)**:

```toml
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
aes-gcm = "0.10"
hkdf = "0.12"
base64 = "0.22"
# sha2 已存在（用于 API Key hash），无需新增
# jsonwebtoken 已存在，用于 JWKS RS256 验证
```

> **依赖说明**：
> - `reqwest`：新增 HTTP 客户端依赖，用于 JWKS 拉取、code 换 token、ElevoOne token 刷新。使用 `rustls-tls` 而非默认的 `native-tls`（openssl），避免引入重量级的 OpenSSL 依赖。项目已使用 `tokio` 运行时，与 `reqwest` 兼容。
> - `aes-gcm`：client_secret AES-256-GCM 加密/解密
> - `hkdf`：从 JWT_SECRET 派生 AES 加密密钥
> - `base64`：client_secret 加密后的编码存储（格式：`base64(iv || ciphertext || tag)`）
> - 不需要额外的 OIDC 客户端库，ElevoOne 使用标准 OIDC 协议，直接用 HTTP 调用 + JWT 库即可完成所有操作

---

## 5. 安全设计

| 安全点 | 措施 |
|--------|------|
| State 防护 | 后端生成随机 state，存入 `oidc_auth_sessions` 表，回调时验证后标记 `consumed=true`（一次性消费，替代物理删除，避免 ElevoOne 回调重试导致误拒），防 CSRF |
| Nonce 防重放 | 后端生成 nonce，回调时验证 ID Token 中的 nonce 与 auth session 匹配 |
| PKCE | 后端生成 code_verifier，计算 code_challenge 发送给 ElevoOne，回调时用 code_verifier 换 token |
| Client Secret | 仅后端使用，AES-256-GCM 加密存 DB，不暴露到前端 |
| ID Token 验证 | RS256 签名验证（JWKS）+ iss/aud/exp/nonce 全部校验 |
| Session Code | 一次性、30 秒过期、原子消费（`UPDATE ... WHERE NOT consumed RETURNING`），避免 JWT 暴露在 URL 中 |
| JWKS 缓存 | 定时刷新 + kid 触发刷新 + 并发保护，支持密钥轮换 |
| Token 来源区分 | 解析 JWT header 的 `alg` 字段路由到正确的验证路径，统一错误信息不泄露验证细节 |
| gRPC 认证同步 | gRPC auth 层同步支持 RS256 分支，认证逻辑复用 `OidcService::verify_and_resolve_tenant()`（见 3.4.5 节） |
| Logout | 清除本地 session（oidc_token_store）+ 调用 ElevoOne end_session 清除 SSO session |
| Token 刷新 | Refresh token rotation（ElevoOne 侧），本地 session 同步更新 |
| 审计日志 | OIDC 登录事件记录到 audit_logs（复用现有审计机制），见 5.1 节 |
| Client Secret 存储 | AES-256-GCM 加密后存 DB，页面始终显示为 `••••••••`，加密密钥来自环境变量或 HKDF 派生 |
| 密钥派生 | HKDF-SHA256 标准派生，不从 JWT_SECRET 直接截取 |
| Token Exchange | API 访问场景通过 ElevoOne Token Exchange（RFC 8693）获取 Workspace 专属 token，`aud` 严格匹配（见 3.10 节） |

### 5.1 审计日志集成

OIDC 相关事件复用现有 `audit_logs` 表，新增事件：

| 事件 | actor_type | action | resource_type | resource_id | 附加信息 |
|------|-----------|--------|---------------|-------------|---------|
| OIDC 登录成功 | admin | oidc_login | session | session_id | login_method=oidc, org_id, email |
| OIDC 登录失败 | anonymous | oidc_login_failed | session | - | reason, ip_address |
| 密码登录成功 | admin | login | session | session_id | login_method=password |
| OIDC 配置变更 | admin | update_oidc_config | system_config | oidc_config | changed_fields |
| OIDC 配置测试 | admin | test_oidc_config | system_config | oidc_config | success/failure |
| Token 刷新 | admin | oidc_token_refresh | session | session_id | org_id |
| Logout | admin | logout | session | session_id | login_method=oidc |

### 5.2 并发安全

| 场景 | 处理方式 |
|------|---------|
| JWKS 定时刷新并发 | `try_write` 非阻塞写锁，已有写锁时跳过 |
| JWKS 按需刷新（kid 不存在） | 释放读锁后 `try_write` 获取写锁，`try_write` 失败说明其他请求正在刷新，短暂等待（100ms）后返回，由上层 `verify_access_token` 重新获取读锁并使用已更新的缓存验证 |
| session_code 并发消费 | `UPDATE ... SET session_code_consumed=true WHERE session_code=? AND NOT session_code_consumed RETURNING id`，影响 0 行视为失败 |
| auth session state 并发消费 | `UPDATE ... SET consumed=true WHERE state=? AND NOT consumed RETURNING id`，影响 0 行视为失败（替代物理删除，防止 ElevoOne 回调重试导致误拒） |
| 配置热更新竞态 | OidcService 整体替换（`ArcSwap` 或 `RwLock`），正在进行的请求使用旧配置，新请求使用新配置，短暂不一致可接受 |
| 同一用户多次 OIDC 登录 | 每次登录创建新的 auth session 和 token store 记录，旧记录靠 `expires_at` 清理 |
| 自动创建 tenant 并发 | `elevoone_org_id` 唯一索引保护，并发创建时第二个事务唯一约束冲突，改为查询已存在的 tenant |

### 5.3 session 清理策略

| 表 | 保留时间 | 清理方式 |
|------|---------|---------|
| `oidc_auth_sessions` | 10 分钟（登录流程完成后即可清理） | 定时任务每 5 分钟清理 `expires_at < NOW()` 的记录 |
| `oidc_token_store` | ElevoOne refresh_token 有效期（7 天） | 定时任务每 1 小时清理 `expires_at < NOW()` 的记录。已消费的 session_code 不需要单独清理，随 token_store 记录一起过期 |

### 5.4 多实例部署

OIDC 配置存储在 DB 中，多个 Workspace 实例共享同一份配置。需要注意：

| 场景 | 处理方式 |
|------|---------|
| 配置热更新同步 | 管理员在一个实例上修改配置后写入 DB，其他实例在下次读取配置时自动获取最新值。由于配置变更频率低（通常只在初始化或调整 SSO 时），短暂不一致可接受 |
| JWKS 缓存一致性 | 每个实例独立维护 JWKS 缓存，各自按 `refresh_interval` 定时刷新（默认 1 小时）。不同实例的 JWKS 缓存存在最多 1 小时的时间差。但 ElevoOne 密钥轮换有过渡期（旧密钥保留用于验证尚未过期的 token，通常 ≥ 24 小时），因此在过渡期内 JWKS 缓存的时间差不会影响 token 验证。如果 ElevoOne 修改密钥轮换过渡期策略，需相应调整 Workspace 的 JWKS 刷新间隔 |
| 启动时 DB 不可用 | `OidcService` 初始化失败时降级为 `None`，OIDC 功能不可用但本地 JWT 和 API Key 认证不受影响 |

> **是否需要 DB LISTEN/NOTIFY**：当前方案不使用 DB 通知机制。配置变更频率极低（可能一周/一月一次），其他实例在 JWKS 定时刷新周期内（默认 1 小时）自然会感知到配置变化。如果后续发现 1 小时的延迟不可接受，可以加 DB LISTEN/NOTIFY 做即时通知。

### 5.5 disable_password_login 安全熔断

`disable_password_login=true` 意味着密码登录被禁用，管理员只能通过 SSO 登录。但如果 OIDC 配置本身有问题（issuer 不可达、JWKS 过期、client_secret 错误等），管理员可能被完全锁在外面。

**熔断机制**：

- `disable_password_login` 仅在 OIDC 已启用（`enabled=true`）时可设置为 `true`，保存时后端校验
- 如果最近 10 次 OIDC 登录尝试全部失败，`oidc_config_handler` 在读取配置时自动将 `disable_password_login` 临时降级为 `false`，并记录告警日志
- 降级后管理员可以通过密码登录排查 OIDC 配置问题
- 降级状态不写入 DB（仅内存），避免持久化一个不安全的配置

**熔断计数器实现细节**：

```rust
// 内存中的熔断状态（不持久化）
struct OidcCircuitBreaker {
    recent_failures: AtomicUsize,          // 最近连续失败次数
    last_success_time: AtomicI64,          // 上次成功时间（Unix timestamp）
    window_duration_secs: i64,             // 失败计数窗口（默认 300 秒 = 5 分钟）
    failure_threshold: usize,              // 触发熔断的失败次数阈值（默认 10 次）
}
```

- **计数器位置**：`OidcService` 内部的内存结构，不持久化。服务重启后计数器重置为 0（重启后 OIDC 功能恢复正常，无需熔断）
- **计数窗口**：5 分钟。超过窗口时间的失败不计入连续失败次数
- **触发条件**：5 分钟内连续失败 ≥ 10 次（注意是"连续"失败，期间任何一次成功会重置计数器）
- **恢复机制**：自动恢复。一旦 OIDC 登录成功一次，计数器立即重置为 0。下次读取配置时 `disable_password_login` 恢复为 DB 中的原始值
- **告警日志**：触发熔断时记录 `WARN` 级别日志，包含失败次数和窗口时间，格式示例：
  ```
  [WARN] OIDC circuit breaker triggered: 10 consecutive failures in 300s,
         temporarily enabling password login. OIDC config may need investigation.
  ```

---

## 6. 新安装管理员引导（密码引导 → SSO 接管）

### 6.1 问题：鸡和蛋

OIDC 登录后通过 `org_role=admin` 判定管理员，但首次部署时：
- 没有配置 OIDC → 无法走 SSO 登录
- 没有创建 tenant-org 映射 → ElevoOne 用户无法匹配到 tenant
- 这些操作本身就需要 admin 权限

### 6.2 方案：密码引导 + 页面配置 SSO 接管

**引导流程**：

```
步骤 1: 全新部署
  │  配置 ADMIN_PASSWORD + JWT_SECRET
  │  启动服务，数据库迁移自动创建 oidc_config 表（enabled=false）
  │
  ▼
步骤 2: 管理员首次登录
  │  使用密码登录（唯一方式，因为 OIDC 未启用）
  │  获得 admin 权限
  │
  ▼
步骤 3: 管理员在页面上配置 SSO
  │  打开「系统设置 → SSO 配置」页面
  │  填写:
  │    Issuer URL: https://elevoone.example.com
  │    Client ID: pk_xxx（在 ElevoOne 后台注册产品后获得）
  │    Client Secret: secret_xxx
  │  点击「测试连接」验证 JWKS 可达
  │  点击「保存」→ 即时生效，无需重启
  │  开启「启用 SSO」开关
  │
  ▼
步骤 4: SSO 登录可用 + 外部系统接入
  │  登录页出现 "SSO 登录" 按钮
  │  管理员创建 tenants 并关联 elevoone_org_id
  │  ElevoOne 中 org_role=admin 的用户可通过 SSO 登录管理后台
  │
  │  ElevoOne 中 org_role=member 的用户（浏览器场景）：
  │    首次访问 Workspace SSO 登录页 → ElevoOne 自动创建 user_product_associations（激活）
  │    → 被管理后台拒绝（预期行为），但已激活
  │    → 之后可通过 ElevoOne Token Exchange 获取 Workspace token 调用 API
  │
  │  后台服务（M2M 场景）：
  │    在 ElevoOne 后台为 org 开通 Workspace 产品并启用 allow_m2m
  │    → 后台服务通过 Client Credentials 直接获取 token，无需用户参与
  │
  │  注意：无论哪种场景，org 都需在 ElevoOne 后台开通 Workspace 产品（organization_products）
  │
  ▼
步骤 5: （可选）禁用密码登录
     在 SSO 配置页面开启「禁用密码登录」开关
     完全切换到 SSO 认证（即时生效，无需重启）
```

**关键约束**：

| 场景 | 行为 |
|------|------|
| 未配置 OIDC | 密码登录是唯一方式，SSO 按钮不显示 |
| 已配置 OIDC + 未禁用密码 | 登录页显示两种方式，用户自行选择 |
| 已配置 OIDC + 已禁用密码 | 仅显示 SSO 登录按钮 |
| disable_password_login=true 但 OIDC 未启用 | 后端校验拒绝保存，防止管理员把自己锁在外面 |

**登录页显示逻辑**：

```
if (oidc_config.enabled && oidc_config.disable_password_login) {
    仅显示 SSO 登录按钮
} else if (oidc_config.enabled) {
    显示 SSO 登录按钮（优先） + 密码登录（分隔线下方）
} else {
    仅显示密码登录
}
```

> **为什么不用环境变量白名单（如 `OIDC_ADMIN_EMAILS`）**：
> 密码引导方案更简单直接，不需要额外的邮箱配置和维护。管理员只需要做一次密码登录来引导配置，后续完全走 SSO。这比维护一个邮箱白名单更不容易出错，也不存在白名单过期的问题。
>
> **为什么 `disable_password_login` 放在数据库而不是环境变量**：
> 与 OIDC 其他配置保持一致，管理员通过页面操作即可生效，无需重启服务。同时后端校验确保不会出现"OIDC 未启用但密码登录已禁用"的自锁情况。

---

## 7. 渐进式上线策略

### 第一阶段：密码引导 + 双轨运行（首期实现）

- 新安装通过 `ADMIN_PASSWORD` 引导首次配置
- 配置 OIDC 后，登录页同时显示 SSO 按钮（优先）和密码登录
- 已有的 API Key 机制完全不受影响
- 管理员通过 OIDC 登录后获得 admin 权限（本地 admin JWT）
- Tenant 用户通过 ElevoOne Token Exchange 获取 Workspace 专属 token 后调用 Workspace API
- 审计日志记录 login_method 区分登录来源

### 第二阶段：引导迁移

- OIDC 登录设为默认/突出显示
- 密码登录弱化（通过 SSO 配置页面隐藏密码登录入口）
- 监控迁移覆盖率（通过 audit_logs 的 login_method 统计）

### 第三阶段：可选完全切换

- 在 SSO 配置页面开启「禁用密码登录」
- 所有认证统一走 ElevoOne
- `ADMIN_PASSWORD` 环境变量在下次重启后可移除（不再生效）

---

## 8. ElevoOne 侧准备

ElevoOne 需要做的准备工作（由 ElevoOne 团队或管理员操作）：

1. **创建产品记录**：在 ElevoOne Admin 后台注册 Workspace 为一个产品
   - 获取 `product_key`（作为 OIDC client_id）
   - 获取 `secret_key`（作为 OIDC client_secret）
   - 配置 `allowed_callback_origins`（Workspace 的域名白名单）

2. **组织开通 Workspace 产品**：对于需要通过 Workspace API 访问数据的组织
   - 在 ElevoOne Admin 后台，为对应组织启用 Workspace 产品（`organization_products` 表，`status='active'`）
   - 如需后台服务 M2M 访问，额外开启 `allow_m2m = true`
   - 这是所有外部系统访问 Workspace API 的前置条件
   - 如未开通，调用方 Token Exchange 会收到 `403 access_denied`

3. **ID Token / Token Exchange Claims 确认**：
   - 确认 ID Token 中包含 `org_id`、`org_role` 自定义 claims（管理后台 OIDC 登录流程使用）
   - 确认 Token Exchange 后的 access_token 中包含 `org_id`、`org_role`（API 访问场景使用）
   - 如不在 Token 中，需确认可通过 `/oauth/userinfo` 端点获取
   - 确认 `org_role` 的取值范围（admin/member）

4. **Token Exchange 端点确认**：
   - 确认 ElevoOne 的 `/oauth/token` 端点支持 `grant_type=urn:ietf:params:oauth:grant-type:token-exchange`（RFC 8693）
   - 确认 exchange 后签发的新 access_token 的 `aud` 为请求中指定的 `audience` 值
   - 确认 exchange 后的 token 包含 `org_id`、`org_role` 等 claims

5. **无需代码改动**：ElevoOne 已有完整的 OIDC Provider + Token Exchange 实现，Workspace 作为标准 OIDC RP 接入即可

---

## 9. 关键依赖

| 依赖 | 说明 | 状态 |
|------|------|------|
| ElevoOne OIDC Provider | authorize/token/userinfo/end_session 端点 | ✅ 已实现 |
| ElevoOne JWKS | RS256 公钥暴露 | ✅ 已实现 |
| ElevoOne PKCE | S256 code_challenge 支持 | ✅ 已实现 |
| ElevoOne Refresh Token Rotation | refresh_token 换新后旧 token 失效 | ✅ 已实现 |
| ElevoOne Single Logout | end_session 端点 | ✅ 已实现 |
| ElevoOne Token Exchange | RFC 8693 token-exchange grant_type，支持跨产品身份传递 | ✅ 已实现 |
| ElevoOne 组织-产品关联 | `organization_products` 表，org 开通产品后才能 Token Exchange 或 Client Credentials | ⚠️ 需管理员配置 |
| ElevoOne Client Credentials | RFC 6749 M2M，后台服务以组织身份获取 token，需 `allow_m2m=true` | ✅ 已实现 |
| ElevoOne admin 后台 | 产品注册和管理 | ✅ 已实现 |
| ElevoOne ID Token 自定义 Claims | org_id、org_role 字段（管理后台登录流程） | ⚠️ 需确认 |
| ElevoOne Token Exchange 后 Claims | exchange 后的 access_token 包含 org_id、org_role（API 访问场景） | ⚠️ 需确认 |
| ElevoOne org_id 类型 | 确认 org_id 是整型（BIGINT）还是 UUID，影响 tenants 表 `elevoone_org_id` 类型 | ⚠️ 需确认 |
| ElevoOne 密钥轮换过渡期 | 确认旧密钥保留时间（≥ JWKS 刷新间隔），影响多实例部署的缓存一致性 | ⚠️ 需确认 |
| ElevoOne 自动创建 user_product_associations | authorization_code 流程中自动创建的内部行为是否为稳定的 API 契约（member 用户激活机制依赖此行为） | ⚠️ 需确认 |

---

## 10. API 端点汇总

### 10.1 公开端点（无需认证）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/auth/login` | 密码登录（已有） |
| POST | `/api/v1/auth/oidc/authorize` | 发起 OIDC 授权（返回授权 URL） |
| GET | `/api/v1/auth/oidc/callback` | OIDC 回调（ElevoOne 重定向到此） |
| GET | `/api/v1/auth/oidc/session` | 用 session_code 换取本地 JWT |
| GET | `/api/v1/auth/oidc/config` | 获取 OIDC 公开配置（enabled、disable_password_login） |

### 10.2 Admin 端点（需要 admin JWT）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/system/oidc-config` | 获取完整 OIDC 配置（含 client_id 等） |
| PUT | `/api/v1/system/oidc-config` | 更新 OIDC 配置 |
| POST | `/api/v1/system/oidc-config/test` | 测试 OIDC 连接（验证 JWKS 可达） |

### 10.3 认证端点（需要 admin JWT 或 ElevoOne Token）

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/auth/logout` | 退出登录（清除 session，返回 IdP logout URL） |
| POST | `/api/v1/auth/oidc/refresh` | 刷新 ElevoOne tokens（仅限通过管理后台 OIDC 登录的 admin 用户） |
| GET | `/api/v1/me` | 获取当前用户信息（已有，扩展返回 OIDC 用户信息） |
