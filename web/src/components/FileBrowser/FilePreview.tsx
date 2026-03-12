import { useMemo } from 'react';
import { Card, Descriptions, Typography, Spin } from 'antd';
import { useQuery } from '@tanstack/react-query';
import client from '@/api/client';
import type { FileInfo } from '@/types';

const MAX_PREVIEW_LINES = 500;

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

function TruncatedContent({ content }: { content: string }) {
  const { displayText, truncated, totalLines } = useMemo(() => {
    const lines = content.split('\n');
    if (lines.length <= MAX_PREVIEW_LINES) {
      return { displayText: content, truncated: false, totalLines: lines.length };
    }
    return {
      displayText: lines.slice(0, MAX_PREVIEW_LINES).join('\n'),
      truncated: true,
      totalLines: lines.length,
    };
  }, [content]);

  return (
    <>
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
        {displayText}
      </pre>
      {truncated && (
        <Typography.Text type="secondary" style={{ display: 'block', marginTop: 4 }}>
          显示前 {MAX_PREVIEW_LINES} 行，共 {totalLines} 行
        </Typography.Text>
      )}
    </>
  );
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
        <Descriptions.Item label="大小">{formatSize(info.size)}</Descriptions.Item>
        <Descriptions.Item label="路径">{path}</Descriptions.Item>
      </Descriptions>
      {tooLarge ? (
        <Typography.Text type="secondary">文件过大，无法预览（{formatSize(info.size)}）</Typography.Text>
      ) : !isText ? (
        <Typography.Text type="secondary">二进制文件，无法预览</Typography.Text>
      ) : isLoading ? (
        <Spin />
      ) : (
        <TruncatedContent content={content ?? ''} />
      )}
    </Card>
  );
}
