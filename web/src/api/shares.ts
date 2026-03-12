import client from './client';
import type { Share, CreateShareParams, UpdateShareParams, SharePermission, PermissionLevel } from '@/types';

export async function listShares(params?: Record<string, string | number>) {
  const res = await client.get('/shares', { params });
  return res.data as { shares: Share[]; total: number };
}

export async function getShare(id: string) {
  const res = await client.get(`/shares/${id}`);
  return res.data.share as Share;
}

export async function createShare(params: CreateShareParams) {
  const res = await client.post('/shares', params);
  return res.data.share as Share;
}

export async function updateShare(id: string, params: UpdateShareParams) {
  const res = await client.put(`/shares/${id}`, params);
  return res.data.share as Share;
}

export async function deleteShare(id: string) {
  await client.delete(`/shares/${id}`);
}

// Share permissions
export async function listSharePermissions(shareId: string) {
  const res = await client.get(`/shares/${shareId}/permissions`);
  return res.data.permissions as SharePermission[];
}

export async function grantPermission(shareId: string, tenantId: string, permission: PermissionLevel) {
  const res = await client.post(`/shares/${shareId}/permissions`, { tenant_id: tenantId, permission });
  return res.data.permission as SharePermission;
}

export async function updatePermission(shareId: string, tenantId: string, permission: PermissionLevel) {
  const res = await client.put(`/shares/${shareId}/permissions/${tenantId}`, { permission });
  return res.data.permission as SharePermission;
}

export async function revokePermission(shareId: string, tenantId: string) {
  await client.delete(`/shares/${shareId}/permissions/${tenantId}`);
}

// Share file operations
export async function listShareFiles(shareId: string, path = '/') {
  const res = await client.get(`/shares/${shareId}/files/list`, { params: { path } });
  return res.data;
}

export async function readShareFile(shareId: string, path: string) {
  const res = await client.get(`/shares/${shareId}/files`, { params: { path } });
  return res.data;
}
