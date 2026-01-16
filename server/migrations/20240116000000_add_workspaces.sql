-- Create workspaces table
CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT,
    nfs_url TEXT,
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Add workspace_id column to sandboxes table
ALTER TABLE sandboxes ADD COLUMN workspace_id TEXT REFERENCES workspaces(id);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_workspaces_created_at ON workspaces(created_at);
CREATE INDEX IF NOT EXISTS idx_sandboxes_workspace_id ON sandboxes(workspace_id);
