import client from './client';
import type { AuditLog, AuditFilter } from '@/types';

export async function listAuditLogs(filter?: AuditFilter) {
  const res = await client.get('/audit-logs', { params: filter });
  const raw = res.data as { logs?: AuditLog[]; items?: AuditLog[]; total: number };
  return { logs: raw.logs ?? raw.items ?? [], total: raw.total };
}
