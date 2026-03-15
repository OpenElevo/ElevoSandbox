-- Add workspaces table for storage management

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
