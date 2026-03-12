import type { PermissionLevel } from '@/types';

export const PERMISSION_LEVELS: PermissionLevel[] = ['read', 'write', 'execute', 'admin'];

export const PERMISSION_LABELS: Record<PermissionLevel, string> = {
  read: '读取',
  write: '写入',
  execute: '执行',
  admin: '管理',
};

export const PERMISSION_DISPLAY: Record<PermissionLevel, string> = {
  read: '读取 (read)',
  write: '写入 (write)',
  execute: '执行 (execute)',
  admin: '管理 (admin)',
};

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

export const SANDBOX_STATE_LABELS: Record<string, string> = {
  running: '运行中',
  starting: '启动中',
  stopping: '停止中',
  stopped: '已停止',
  error: '错误',
};

export const AUDIT_ACTION_GROUPS: Record<string, string[]> = {
  '租户': ['tenant.create', 'tenant.update', 'tenant.delete'],
  'API Key': ['api_key.create', 'api_key.revoke'],
  '共享': ['share.create', 'share.update', 'share.delete'],
  '权限': ['permission.grant', 'permission.update', 'permission.revoke'],
};

export const AUDIT_ACTION_LABELS: Record<string, string> = {
  'tenant.create': '创建租户',
  'tenant.update': '更新租户',
  'tenant.delete': '删除租户',
  'api_key.create': '创建 API Key',
  'api_key.revoke': '撤销 API Key',
  'share.create': '创建共享',
  'share.update': '更新共享',
  'share.delete': '删除共享',
  'permission.grant': '授予权限',
  'permission.update': '更新权限',
  'permission.revoke': '撤销权限',
};

export const RESOURCE_TYPES = ['tenant', 'api_key', 'share', 'permission'];

export const RESOURCE_TYPE_LABELS: Record<string, string> = {
  tenant: '租户',
  api_key: 'API Key',
  share: '共享',
  permission: '权限',
};
