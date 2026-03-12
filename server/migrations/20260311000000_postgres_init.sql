-- PostgreSQL initial schema
-- Replaces the 3 SQLite migrations with PostgreSQL-native types

-- sandboxes
CREATE TABLE sandboxes (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          VARCHAR(255),
    template      VARCHAR(255) NOT NULL,
    state         VARCHAR(16) NOT NULL DEFAULT 'starting',
    container_id  VARCHAR(64),
    env           JSONB NOT NULL DEFAULT '{}',
    metadata      JSONB NOT NULL DEFAULT '{}',
    nfs_url       TEXT,
    timeout       INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- processes
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

-- ptys
CREATE TABLE ptys (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sandbox_id UUID NOT NULL REFERENCES sandboxes(id) ON DELETE CASCADE,
    cols       INTEGER NOT NULL DEFAULT 80,
    rows       INTEGER NOT NULL DEFAULT 24,
    state      VARCHAR(16) NOT NULL DEFAULT 'running',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- workspace_leases (moved from runtime-created table into migration)
CREATE TABLE workspace_leases (
    workspace_id UUID PRIMARY KEY,
    holder_id    VARCHAR(255) NOT NULL,
    acquired_at  TIMESTAMPTZ NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL,
    renewed_at   TIMESTAMPTZ NOT NULL
);

-- Indexes
CREATE INDEX idx_sandboxes_state ON sandboxes(state);
CREATE INDEX idx_processes_sandbox_id ON processes(sandbox_id);
CREATE INDEX idx_ptys_sandbox_id ON ptys(sandbox_id);
CREATE INDEX idx_workspace_leases_expires ON workspace_leases(expires_at);
