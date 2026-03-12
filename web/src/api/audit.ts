import client from './client';
import type { AuditLog, AuditFilter } from '@/types';

export async function listAuditLogs(filter?: AuditFilter) {
  const res = await client.get('/audit-logs', { params: filter });
  return res.data as { logs: AuditLog[]; total: number };
}
