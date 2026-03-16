-- Elevo Workspace - PostgreSQL initial schema (consolidated)

-- ============================================================
-- Tenants & Auth
-- ============================================================
CREATE TABLE tenants (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name           VARCHAR(255) NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    is_active      BOOLEAN NOT NULL DEFAULT true,
    storage_type   VARCHAR(16) NOT NULL DEFAULT 'managed'
                   CHECK(storage_type IN ('managed', 'remote')),
    storage_config JSONB NOT NULL DEFAULT '{}',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX idx_tenants_name_lower ON tenants(lower(name));

CREATE TABLE api_keys (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name            VARCHAR(255) NOT NULL,
    token_hash      VARCHAR(64) UNIQUE NOT NULL,
    token_prefix    VARCHAR(16) NOT NULL,
    token_plaintext TEXT NOT NULL DEFAULT '',
    is_active       BOOLEAN NOT NULL DEFAULT true,
    expires_at      TIMESTAMPTZ,
    last_used_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);
CREATE INDEX idx_api_keys_tenant_id ON api_keys(tenant_id);

-- ============================================================
-- Workspaces
-- ============================================================
CREATE TABLE workspaces (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name           VARCHAR(255),
    nfs_url        TEXT,
    storage_type   VARCHAR(16) NOT NULL DEFAULT 'managed'
                   CHECK(storage_type IN ('managed', 'remote')),
    storage_config JSONB NOT NULL DEFAULT '{}',
    metadata       JSONB NOT NULL DEFAULT '{}',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_workspaces_name ON workspaces(name);

-- ============================================================
-- Sandboxes
-- ============================================================
CREATE TABLE sandboxes (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          VARCHAR(255),
    template      VARCHAR(255) NOT NULL,
    state         VARCHAR(16) NOT NULL DEFAULT 'starting',
    container_id  VARCHAR(64),
    env           JSONB NOT NULL DEFAULT '{}',
    metadata      JSONB NOT NULL DEFAULT '{}',
    timeout       INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    namespace_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    root_path     TEXT NOT NULL DEFAULT '/',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_sandboxes_namespace_id ON sandboxes(namespace_id);
CREATE INDEX idx_sandboxes_namespace_state ON sandboxes(namespace_id, state);

-- ============================================================
-- Processes & PTYs
-- ============================================================
CREATE TABLE processes (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sandbox_id UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    command    TEXT NOT NULL,
    state      VARCHAR(16) NOT NULL DEFAULT 'running',
    pid        INTEGER,
    exit_code  INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_processes_sandbox_id ON processes(sandbox_id);

CREATE TABLE ptys (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sandbox_id UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    cols       INTEGER NOT NULL DEFAULT 80,
    rows       INTEGER NOT NULL DEFAULT 24,
    state      VARCHAR(16) NOT NULL DEFAULT 'running',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_ptys_sandbox_id ON ptys(sandbox_id);

-- ============================================================
-- Namespace Leases
-- ============================================================
CREATE TABLE namespace_leases (
    namespace_id UUID PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    holder_id    VARCHAR(255) NOT NULL,
    acquired_at  TIMESTAMPTZ NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    renewed_at   TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_namespace_leases_expires ON namespace_leases(expires_at);

-- ============================================================
-- Shares & Permissions
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

CREATE TABLE share_permissions (
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    share_id   UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    permission VARCHAR(16) NOT NULL CHECK(permission IN ('read','write','execute','admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, share_id)
);
CREATE INDEX idx_sp_share_id ON share_permissions(share_id);

-- ============================================================
-- Sandbox Mounts
-- ============================================================
CREATE TABLE sandbox_mounts (
    sandbox_id UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    share_id   UUID NOT NULL REFERENCES shares(id) ON DELETE RESTRICT,
    mount_path TEXT NOT NULL,
    PRIMARY KEY (sandbox_id, share_id),
    UNIQUE (sandbox_id, mount_path)
);

-- ============================================================
-- Audit Logs
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
CREATE INDEX idx_audit_logs_query ON audit_logs(created_at, action, actor_type);
