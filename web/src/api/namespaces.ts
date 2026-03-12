import client from './client';
import type { FileInfo } from '@/types';

export async function listNamespaceFiles(namespaceId: string, path = '/') {
  const res = await client.get(`/namespaces/${namespaceId}/files/list`, { params: { path } });
  return res.data.files as FileInfo[];
}

export async function readNamespaceFile(namespaceId: string, path: string) {
  const res = await client.get(`/namespaces/${namespaceId}/files`, { params: { path } });
  return res.data;
}

export async function getNamespaceFileInfo(namespaceId: string, path: string) {
  const res = await client.get(`/namespaces/${namespaceId}/files/info`, { params: { path } });
  return res.data as FileInfo;
}
