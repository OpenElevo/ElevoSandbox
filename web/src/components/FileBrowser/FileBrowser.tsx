import { useState } from 'react';
import { Row, Col, Tree, Card, Typography, Spin, Empty } from 'antd';
import { FolderOutlined, FileOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import client from '@/api/client';
import type { FileInfo } from '@/types';
import FilePreview from './FilePreview';

interface FileBrowserProps {
  namespaceId?: string;
  shareId?: string;
}

interface TreeNode {
  title: string;
  key: string;
  isLeaf: boolean;
  icon: React.ReactNode;
  children?: TreeNode[];
  fileInfo?: FileInfo;
}

async function fetchFiles(namespaceId?: string, shareId?: string, path = '/'): Promise<FileInfo[]> {
  if (shareId) {
    const res = await client.get(`/shares/${shareId}/files/list`, { params: { path } });
    return res.data.files ?? res.data ?? [];
  }
  if (namespaceId) {
    const res = await client.get(`/namespaces/${namespaceId}/files/list`, { params: { path } });
    return res.data.files ?? res.data ?? [];
  }
  return [];
}

function filesToTreeNodes(files: FileInfo[], parentPath: string): TreeNode[] {
  return files.map((f) => {
    const fullPath = parentPath === '/' ? `/${f.name}` : `${parentPath}/${f.name}`;
    return {
      title: f.name,
      key: fullPath,
      isLeaf: !f.is_dir,
      icon: f.is_dir ? <FolderOutlined /> : <FileOutlined />,
      fileInfo: f,
    };
  });
}

export default function FileBrowser({ namespaceId, shareId }: FileBrowserProps) {
  const [selectedFile, setSelectedFile] = useState<{ path: string; info: FileInfo } | null>(null);
  const [treeData, setTreeData] = useState<TreeNode[]>([]);
  const [loadedKeys, setLoadedKeys] = useState<Set<string>>(new Set());

  const { isLoading } = useQuery({
    queryKey: ['files-root', namespaceId, shareId],
    queryFn: async () => {
      const files = await fetchFiles(namespaceId, shareId, '/');
      setTreeData(filesToTreeNodes(files, '/'));
      return files;
    },
  });

  const onLoadData = async (node: TreeNode) => {
    if (loadedKeys.has(node.key)) return;
    const files = await fetchFiles(namespaceId, shareId, node.key);
    const children = filesToTreeNodes(files, node.key);

    setTreeData((prev) => {
      const updateChildren = (nodes: TreeNode[]): TreeNode[] =>
        nodes.map((n) => {
          if (n.key === node.key) {
            return { ...n, children: children.length > 0 ? children : [] };
          }
          if (n.children) {
            return { ...n, children: updateChildren(n.children) };
          }
          return n;
        });
      return updateChildren(prev);
    });
    setLoadedKeys((prev) => new Set(prev).add(node.key));
  };

  const onSelect = (_: unknown, info: { node: TreeNode }) => {
    const node = info.node;
    if (!node.isLeaf || !node.fileInfo) return;
    setSelectedFile({ path: node.key, info: node.fileInfo });
  };

  if (isLoading) return <Spin />;

  return (
    <Row gutter={16}>
      <Col span={8}>
        <Card size="small" title="文件" style={{ height: 500, overflow: 'auto' }}>
          {treeData.length === 0 ? (
            <Empty description="空目录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
          ) : (
            <Tree
              showIcon
              treeData={treeData as never}
              loadData={onLoadData as never}
              onSelect={onSelect as never}
              blockNode
            />
          )}
        </Card>
      </Col>
      <Col span={16}>
        {selectedFile ? (
          <FilePreview
            namespaceId={namespaceId}
            shareId={shareId}
            path={selectedFile.path}
            info={selectedFile.info}
          />
        ) : (
          <Card size="small" style={{ height: 500, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
            <Typography.Text type="secondary">选择文件预览</Typography.Text>
          </Card>
        )}
      </Col>
    </Row>
  );
}
