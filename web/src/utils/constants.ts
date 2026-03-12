import type { PermissionLevel } from '@/types';

export const PERMISSION_LEVELS: PermissionLevel[] = ['read', 'write', 'execute', 'admin'];

export const PERMISSION_COLORS: Record<PermissionLevel, string> = {
  read: 'blue',
  write: 'green',
  execute: 'orange',
  admin: 'red',
};

export const SANDBOX_STATE_COLORS: Record<string, string> = {
  running: 'green',
  starting: 'blue',
  stopping: 'orange',
  stopped: 'default',
  error: 'red',
};

export const AUDIT_ACTION_GROUPS: Record<string, string[]> = {
  'Tenant': ['tenant.create', 'tenant.update', 'tenant.delete'],
  'API Key': ['api_key.create', 'api_key.revoke'],
  'Share': ['share.create', 'share.update', 'share.delete'],
  'Permission': ['permission.grant', 'permission.update', 'permission.revoke'],
};

export const RESOURCE_TYPES = ['tenant', 'api_key', 'share'];
