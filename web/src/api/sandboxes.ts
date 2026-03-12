import client from './client';
import type { Sandbox } from '@/types';

export async function listSandboxes(params?: Record<string, string | number>) {
  const res = await client.get('/sandboxes', { params });
  return res.data as { sandboxes: Sandbox[]; total: number };
}

export async function getSandbox(id: string) {
  const res = await client.get(`/sandboxes/${id}`);
  return res.data.sandbox as Sandbox;
}

export async function deleteSandbox(id: string) {
  await client.delete(`/sandboxes/${id}`);
}

export interface BatchDeleteParams {
  ids?: string[];
  filter?: { state?: string; namespace_id?: string };
}

export interface BatchDeleteResult {
  deleted: number;
  failed: number;
  errors?: Array<{ id: string; error: string }>;
}

export async function batchDeleteSandboxes(params: BatchDeleteParams): Promise<BatchDeleteResult> {
  const res = await client.post('/sandboxes/batch-delete', params);
  return res.data as BatchDeleteResult;
}
