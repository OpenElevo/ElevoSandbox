import { Card, Descriptions, Typography, Spin } from 'antd';
import { useQuery } from '@tanstack/react-query';
import client from '@/api/client';
import type { FileInfo } from '@/types';

interface FilePreviewProps {
  namespaceId?: string;
  shareId?: string;
  path: string;
  info: FileInfo;
}

const TEXT_EXTENSIONS = new Set([
  'txt', 'md', 'json', 'yaml', 'yml', 'toml', 'xml', 'csv', 'log',
  'rs', 'ts', 'tsx', 'js', 'jsx', 'py', 'go', 'java', 'c', 'cpp', 'h',
  'html', 'css', 'scss', 'sql', 'sh', 'bash', 'zsh', 'fish',
  'dockerfile', 'makefile', 'gitignore', 'env', 'cfg', 'ini', 'conf',
]);

function isTextFile(name: string): boolean {
  const ext = name.split('.').pop()?.toLowerCase() ?? '';
  const baseName = name.toLowerCase();
  return TEXT_EXTENSIONS.has(ext) || TEXT_EXTENSIONS.has(baseName);
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function FilePreview({ namespaceId, shareId, path, info }: FilePreviewProps) {
  const isText = isTextFile(info.name);
  const tooLarge = info.size > 1024 * 1024; // 1MB

  const { data: content, isLoading } = useQuery({
    queryKey: ['file-content', namespaceId, shareId, path],
    queryFn: async () => {
      if (shareId) {
        const res = await client.get(`/shares/${shareId}/files`, { params: { path } });
        return typeof res.data === 'string' ? res.data : JSON.stringify(res.data, null, 2);
      }
      if (namespaceId) {
        const res = await client.get(`/namespaces/${namespaceId}/files`, { params: { path } });
        return typeof res.data === 'string' ? res.data : JSON.stringify(res.data, null, 2);
      }
      return '';
    },
    enabled: isText && !tooLarge,
  });

  return (
    <Card size="small" title={info.name} style={{ height: 500, overflow: 'auto' }}>
      <Descriptions size="small" column={3} style={{ marginBottom: 12 }}>
        <Descriptions.Item label="Size">{formatSize(info.size)}</Descriptions.Item>
        <Descriptions.Item label="Path">{path}</Descriptions.Item>
      </Descriptions>
      {tooLarge ? (
        <Typography.Text type="secondary">File too large to preview ({formatSize(info.size)})</Typography.Text>
      ) : !isText ? (
        <Typography.Text type="secondary">Binary file — preview not available</Typography.Text>
      ) : isLoading ? (
        <Spin />
      ) : (
        <pre style={{
          background: '#f5f5f5',
          padding: 12,
          borderRadius: 4,
          fontSize: 12,
          maxHeight: 360,
          overflow: 'auto',
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-all',
        }}>
          {content}
        </pre>
      )}
    </Card>
  );
}
