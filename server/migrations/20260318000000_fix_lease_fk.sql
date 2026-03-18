-- Fix namespace_leases FK: tenants(id) → workspaces(id)
-- The lease mechanism is workspace-level concurrency control; all code paths
-- pass workspace_id as the lease key, so the FK must reference workspaces(id).
--
-- Existing databases have a hand-named FK "fk_namespace_leases_tenant" referencing
-- tenants(id).  Fresh databases (from the updated init migration) get an auto-named
-- FK "namespace_leases_namespace_id_fkey" referencing workspaces(id).  We drop
-- whichever exists and (re-)add an explicitly-named constraint so both upgrade
-- and fresh-install paths converge on the same constraint name.

ALTER TABLE namespace_leases DROP CONSTRAINT IF EXISTS fk_namespace_leases_tenant;
ALTER TABLE namespace_leases DROP CONSTRAINT IF EXISTS namespace_leases_namespace_id_fkey;
ALTER TABLE namespace_leases ADD CONSTRAINT fk_namespace_leases_workspace
    FOREIGN KEY (namespace_id) REFERENCES workspaces(id) ON DELETE CASCADE;
