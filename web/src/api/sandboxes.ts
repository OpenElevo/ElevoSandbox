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
