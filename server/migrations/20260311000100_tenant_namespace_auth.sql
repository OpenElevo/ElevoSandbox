-- Phase 2a: Tenant, Namespace, and Auth tables

-- tenants table (serves as Namespace, 1:1 relationship)
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

-- api_keys table
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

-- sandboxes: add namespace_id and root_path columns
ALTER TABLE sandboxes ADD COLUMN namespace_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT;
ALTER TABLE sandboxes ADD COLUMN root_path TEXT NOT NULL DEFAULT '/';
CREATE INDEX idx_sandboxes_namespace_id ON sandboxes(namespace_id);

-- Composite index on (namespace_id, state) for efficient namespace-scoped state filtering.
-- Replaces the single-column idx_sandboxes_state from the initial migration.
DROP INDEX IF EXISTS idx_sandboxes_state;
CREATE INDEX idx_sandboxes_namespace_state ON sandboxes(namespace_id, state);

-- Rename workspace_leases to namespace_leases
ALTER TABLE workspace_leases RENAME TO namespace_leases;
ALTER TABLE namespace_leases RENAME COLUMN workspace_id TO namespace_id;
ALTER TABLE namespace_leases ADD CONSTRAINT fk_namespace_leases_tenant
    FOREIGN KEY (namespace_id) REFERENCES tenants(id) ON DELETE CASCADE;
