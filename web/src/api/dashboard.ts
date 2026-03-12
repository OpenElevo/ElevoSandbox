import client from './client';
import type { DashboardStats } from '@/types';

export async function getDashboardStats() {
  const res = await client.get('/dashboard/stats');
  return res.data as DashboardStats;
}
