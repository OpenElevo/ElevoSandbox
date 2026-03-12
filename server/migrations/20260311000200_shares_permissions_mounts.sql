-- Phase 2b: Shares, permissions, and sandbox mounts

-- shares table
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

-- share_permissions table
CREATE TABLE share_permissions (
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    share_id   UUID NOT NULL REFERENCES shares(id) ON DELETE CASCADE,
    permission VARCHAR(16) NOT NULL CHECK(permission IN ('read','write','execute','admin')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, share_id)
);
CREATE INDEX idx_sp_share_id ON share_permissions(share_id);

-- sandbox_mounts table
CREATE TABLE sandbox_mounts (
    sandbox_id UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    share_id   UUID NOT NULL REFERENCES shares(id) ON DELETE RESTRICT,
    mount_path TEXT NOT NULL,
    PRIMARY KEY (sandbox_id, share_id),
    UNIQUE (sandbox_id, mount_path)
);

-- Clean up legacy columns from sandboxes
ALTER TABLE sandboxes DROP COLUMN IF EXISTS workspace_id;
ALTER TABLE sandboxes DROP COLUMN IF EXISTS nfs_url;
ALTER TABLE sandboxes ALTER COLUMN namespace_id SET NOT NULL;

-- Drop legacy workspace tables
DROP TABLE IF EXISTS workspaces;
DROP INDEX IF EXISTS idx_sandboxes_workspace_id;
