-- Add storage_type and storage_config to workspaces table
-- storage_type: 'managed' (Server-managed local/S3) or 'remote' (Client-provided)
-- storage_config: JSON config for remote storage (transport, nfs_url, switch state)
ALTER TABLE workspaces ADD COLUMN storage_type TEXT NOT NULL DEFAULT 'managed';
ALTER TABLE workspaces ADD COLUMN storage_config TEXT NOT NULL DEFAULT '{}';
