-- Sandboxes table
CREATE TABLE IF NOT EXISTS sandboxes (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT,
    template TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'starting',
    container_id TEXT,
    env TEXT NOT NULL DEFAULT '{}',
    metadata TEXT NOT NULL DEFAULT '{}',
    nfs_url TEXT,
    timeout INTEGER NOT NULL DEFAULT 0,
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Processes table (for tracking running processes)
CREATE TABLE IF NOT EXISTS processes (
    id TEXT PRIMARY KEY NOT NULL,
    sandbox_id TEXT NOT NULL,
    command TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'running',
    pid INTEGER,
    exit_code INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (sandbox_id) REFERENCES sandboxes(id) ON DELETE CASCADE
);

-- PTYs table (for tracking active PTY sessions)
CREATE TABLE IF NOT EXISTS ptys (
    id TEXT PRIMARY KEY NOT NULL,
    sandbox_id TEXT NOT NULL,
    cols INTEGER NOT NULL DEFAULT 80,
    rows INTEGER NOT NULL DEFAULT 24,
    state TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (sandbox_id) REFERENCES sandboxes(id) ON DELETE CASCADE
);

-- Indexes
CREATE INDEX IF NOT EXISTS idx_sandboxes_state ON sandboxes(state);
CREATE INDEX IF NOT EXISTS idx_sandboxes_created_at ON sandboxes(created_at);
CREATE INDEX IF NOT EXISTS idx_processes_sandbox_id ON processes(sandbox_id);
CREATE INDEX IF NOT EXISTS idx_ptys_sandbox_id ON ptys(sandbox_id);
