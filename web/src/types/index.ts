export interface Tenant {
  id: string;
  name: string;
  description: string;
  is_active: boolean;
  storage_type: string;
  storage_config: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  share_count?: number;
  active_api_key_count?: number;
}

export interface CreateTenantParams {
  name: string;
  description?: string;
  storage_type?: string;
  storage_config?: Record<string, unknown>;
  initial_api_key?: { name: string; expires_at?: string };
}

export interface UpdateTenantParams {
  name?: string;
  description?: string;
}

export interface ApiKey {
  id: string;
  tenant_id: string;
  name: string;
  token_prefix: string;
  is_active: boolean;
  expires_at: string | null;
  last_used_at: string | null;
  created_at: string;
}

export interface Share {
  id: string;
  owner_tenant_id: string;
  owner_tenant_name?: string;
  name: string;
  source_path: string;
  description: string;
  visibility: 'public' | 'private';
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface CreateShareParams {
  owner_tenant_id: string;
  name: string;
  source_path: string;
  description?: string;
  visibility?: 'public' | 'private';
}

export interface UpdateShareParams {
  name?: string;
  description?: string;
  visibility?: 'public' | 'private';
}

export type PermissionLevel = 'read' | 'write' | 'execute' | 'admin';

export interface SharePermission {
  tenant_id: string;
  share_id: string;
  share_name?: string;
  permission: PermissionLevel;
  created_at: string;
  tenant_name?: string;
}

export interface Sandbox {
  id: string;
  namespace_id: string;
  name: string;
  template: string;
  state: string;
  container_id: string | null;
  root_path: string;
  env: Record<string, string>;
  metadata: Record<string, unknown>;
  error_message: string | null;
  timeout: number;
  created_at: string;
  updated_at: string;
  mounts?: SandboxMount[];
}

export interface SandboxMount {
  sandbox_id: string;
  share_id: string;
  mount_path: string;
}

export interface AuditLog {
  id: string;
  actor_type: 'admin' | 'tenant';
  actor_id: string | null;
  action: string;
  resource_type: string;
  resource_id: string;
  resource_name: string;
  detail: Record<string, unknown>;
  ip_address: string | null;
  created_at: string;
}

export interface AuditFilter {
  action?: string[];
  actor_type?: string;
  actor_id?: string;
  resource_type?: string;
  from?: string;
  to?: string;
  page?: number;
  page_size?: number;
}

export interface DashboardStats {
  tenants: { total: number; active: number };
  shares: { total: number };
  sandboxes: { running: number };
  api_keys: { active: number };
}

export interface FileInfo {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: string;
}

export interface PaginatedResponse<T> {
  data: T[];
  total: number;
}
