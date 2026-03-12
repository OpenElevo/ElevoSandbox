import client from './client';
import type { Tenant, CreateTenantParams, UpdateTenantParams, ApiKey } from '@/types';

export async function listTenants(params?: Record<string, string | number | boolean>) {
  const res = await client.get('/tenants', { params });
  return res.data as { tenants: Tenant[]; total: number };
}

export async function getTenant(id: string) {
  const res = await client.get(`/tenants/${id}`);
  return res.data.tenant as Tenant;
}

export async function createTenant(params: CreateTenantParams) {
  const res = await client.post('/tenants', params);
  return res.data as { tenant: Tenant; api_key?: { key: ApiKey; token: string } };
}

export async function updateTenant(id: string, params: UpdateTenantParams) {
  const res = await client.put(`/tenants/${id}`, params);
  return res.data.tenant as Tenant;
}

export async function activateTenant(id: string) {
  const res = await client.post(`/tenants/${id}/activate`);
  return res.data.tenant as Tenant;
}

export async function deactivateTenant(id: string) {
  const res = await client.post(`/tenants/${id}/deactivate`);
  return res.data.tenant as Tenant;
}

export async function deleteTenant(id: string, force = false) {
  await client.delete(`/tenants/${id}`, { params: { force } });
}

export async function listApiKeys(tenantId: string) {
  const res = await client.get(`/tenants/${tenantId}/keys`);
  return res.data.keys as ApiKey[];
}

export async function createApiKey(tenantId: string, params: { name: string; expires_at?: string }) {
  const res = await client.post(`/tenants/${tenantId}/keys`, params);
  return res.data as { key: ApiKey; token: string };
}

export async function revokeApiKey(tenantId: string, keyId: string) {
  await client.delete(`/tenants/${tenantId}/keys/${keyId}`);
}

export async function listTenantPermissions(tenantId: string) {
  const res = await client.get(`/tenants/${tenantId}/permissions`);
  return res.data.permissions as import('@/types').SharePermission[];
}
