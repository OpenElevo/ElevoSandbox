import { useState, useMemo } from 'react';
import {
  Table, Button, Input, Space, Tag, Typography, Drawer, Form, Select, App,
  Modal, Tree, Spin, Empty,
} from 'antd';
import { PlusOutlined, SearchOutlined, FolderOpenOutlined, FolderOutlined } from '@ant-design/icons';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listShares, createShare, deleteShare } from '@/api/shares';
import { listTenants } from '@/api/tenants';
import client from '@/api/client';
import type { Share, CreateShareParams, FileInfo } from '@/types';
import { formatTime } from '@/utils/time';
import { useDebounce } from '@/hooks/useDebounce';

// ─── Directory browser modal ──────────────────────────────────────────────────

interface DirNode {
  title: string;
  key: string;
  isLeaf: boolean;
  icon: React.ReactNode;
  children?: DirNode[];
}

async function fetchDirs(namespaceId: string, path = '/'): Promise<FileInfo[]> {
  const res = await client.get(`/namespaces/${namespaceId}/files/list`, { params: { path } });
  const files: FileInfo[] = res.data.files ?? res.data ?? [];
  return files.filter((f) => f.is_dir);
}

function dirsToNodes(files: FileInfo[], parentPath: string): DirNode[] {
  return files.map((f) => {
    const fullPath = parentPath === '/' ? `/${f.name}` : `${parentPath}/${f.name}`;
    return {
      title: f.name,
      key: fullPath,
      isLeaf: false,
      icon: <FolderOutlined />,
    };
  });
}

interface DirBrowserModalProps {
  open: boolean;
  namespaceId: string | null;
  onSelect: (path: string) => void;
  onClose: () => void;
}

function DirBrowserModal({ open, namespaceId, onSelect, onClose }: DirBrowserModalProps) {
  const [treeData, setTreeData] = useState<DirNode[]>([]);
  const [loadedKeys, setLoadedKeys] = useState<Set<string>>(new Set());
  const [selectedPath, setSelectedPath] = useState<string>('');

  const { isLoading } = useQuery({
    queryKey: ['dirs-root', namespaceId],
    queryFn: async () => {
      if (!namespaceId) return [];
      const dirs = await fetchDirs(namespaceId, '/');
      // Always include root
      const rootChildren = dirsToNodes(dirs, '/');
      setTreeData(rootChildren);
      setLoadedKeys(new Set());
      setSelectedPath('');
      return dirs;
    },
    enabled: open && !!namespaceId,
  });

  const onLoadData = async (node: DirNode) => {
    if (!namespaceId || loadedKeys.has(node.key)) return;
    const dirs = await fetchDirs(namespaceId, node.key);
    const children = dirsToNodes(dirs, node.key);

    setTreeData((prev) => {
      const updateChildren = (nodes: DirNode[]): DirNode[] =>
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

  const handleConfirm = () => {
    onSelect(selectedPath || '/');
    onClose();
  };

  return (
    <Modal
      title="浏览目录"
      open={open}
      onOk={handleConfirm}
      onCancel={onClose}
      okText="选择"
      okButtonProps={{ disabled: !selectedPath }}
      width={480}
    >
      <Typography.Text type="secondary" style={{ display: 'block', marginBottom: 8 }}>
        请选择源路径目录（当前选择：{selectedPath || '未选择'}）
      </Typography.Text>
      {!namespaceId ? (
        <Empty description="请先选择所属租户" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : isLoading ? (
        <div style={{ textAlign: 'center', padding: 24 }}><Spin /></div>
      ) : treeData.length === 0 ? (
        <Empty description="该命名空间暂无子目录" image={Empty.PRESENTED_IMAGE_SIMPLE} />
      ) : (
        <div
          style={{
            border: '1px solid #d9d9d9',
            borderRadius: 6,
            padding: '8px',
            maxHeight: 320,
            overflow: 'auto',
          }}
        >
          <Tree
            showIcon
            treeData={treeData as never}
            loadData={onLoadData as never}
            selectedKeys={selectedPath ? [selectedPath] : []}
            onSelect={(keys) => {
              if (keys.length > 0) setSelectedPath(String(keys[0]));
            }}
            blockNode
            icon={<FolderOpenOutlined />}
          />
        </div>
      )}
    </Modal>
  );
}

// ─── Main component ────────────────────────────────────────────────────────────

export default function ShareList() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [search, setSearch] = useState('');
  const [visFilter, setVisFilter] = useState<string>();
  const [ownerFilter, setOwnerFilter] = useState<string>();
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [dirBrowserOpen, setDirBrowserOpen] = useState(false);
  const [form] = Form.useForm();

  const debouncedSearch = useDebounce(search);

  const queryParams = useMemo(() => {
    const p: Record<string, string | number> = { page, page_size: pageSize };
    if (debouncedSearch) p.search = debouncedSearch;
    if (visFilter) p.visibility = visFilter;
    if (ownerFilter) p.owner_tenant_id = ownerFilter;
    return p;
  }, [debouncedSearch, visFilter, ownerFilter, page, pageSize]);

  const { data, isLoading } = useQuery({
    queryKey: ['shares', queryParams],
    queryFn: () => listShares(queryParams),
  });

  const { data: tenantsData } = useQuery({
    queryKey: ['tenants-select'],
    queryFn: () => listTenants({ page_size: 200 }),
  });

  const createMutation = useMutation({
    mutationFn: (params: CreateShareParams) => createShare(params),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['shares'] });
      setDrawerOpen(false);
      form.resetFields();
      message.success('共享已创建');
    },
    onError: (err: { response?: { data?: { error?: { message?: string } } } }) => {
      message.error(err.response?.data?.error?.message || '创建共享失败');
    },
  });

  const handleCreate = () => {
    form.validateFields().then((values) => {
      createMutation.mutate(values);
    });
  };

  const handleDelete = (share: Share) => {
    let inputName = '';
    modal.confirm({
      title: `删除共享「${share.name}」？`,
      content: (
        <div>
          <Typography.Text type="danger">删除后无法恢复，所有关联权限也将被清除。</Typography.Text>
          <Input
            placeholder="请输入共享名称确认"
            style={{ marginTop: 8 }}
            onChange={(e) => { inputName = e.target.value; }}
          />
        </div>
      ),
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        if (inputName !== share.name) {
          message.error('名称不匹配');
          throw new Error('mismatch');
        }
        await deleteShare(share.id);
        queryClient.invalidateQueries({ queryKey: ['shares'] });
        message.success('共享已删除');
      },
    });
  };

  // F12: When source_path changes, auto-fill name with last segment if name is empty
  const handleSourcePathChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const path = e.target.value;
    const currentName = form.getFieldValue('name');
    if (!currentName) {
      const lastSegment = path.replace(/\/$/, '').split('/').filter(Boolean).pop();
      if (lastSegment) {
        form.setFieldValue('name', lastSegment);
      }
    }
  };

  // F12: Apply path selected from dir browser
  const handleDirSelected = (path: string) => {
    form.setFieldValue('source_path', path);
    const currentName = form.getFieldValue('name');
    if (!currentName) {
      const lastSegment = path.replace(/\/$/, '').split('/').filter(Boolean).pop();
      if (lastSegment) {
        form.setFieldValue('name', lastSegment);
      }
    }
  };

  const tenantOptions = (tenantsData?.tenants ?? []).map((t) => ({
    label: t.name,
    value: t.id,
  }));

  const columns = [
    {
      title: '名称', dataIndex: 'name', key: 'name',
      render: (name: string, r: Share) => (
        <a onClick={() => navigate(`/admin/shares/${r.id}`)}>{name}</a>
      ),
    },
    {
      title: '所属租户', dataIndex: 'owner_tenant_id', key: 'owner', width: 200,
      render: (tid: string) => {
        const t = tenantsData?.tenants.find((x) => x.id === tid);
        return t ? <a onClick={() => navigate(`/admin/tenants/${tid}`)}>{t.name}</a> : tid.slice(0, 8);
      },
    },
    { title: '源路径', dataIndex: 'source_path', key: 'path' },
    {
      title: '可见性', dataIndex: 'visibility', key: 'vis', width: 100,
      render: (v: string) => <Tag color={v === 'public' ? 'blue' : 'default'}>{v === 'public' ? '公开' : '私有'}</Tag>,
    },
    {
      title: '创建时间', dataIndex: 'created_at', key: 'created', width: 180,
      render: (v: string) => formatTime(v),
    },
    {
      title: '操作', key: 'actions', width: 140,
      render: (_: unknown, record: Share) => (
        <Space size="small">
          <Button size="small" type="link" onClick={() => navigate(`/admin/shares/${record.id}`)}>查看</Button>
          <Button size="small" danger onClick={() => handleDelete(record)}>删除</Button>
        </Space>
      ),
    },
  ];

  // The selected owner tenant for the dir browser
  const selectedOwnerTenantId = Form.useWatch('owner_tenant_id', form) as string | undefined;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16 }}>
        <Typography.Title level={4} style={{ margin: 0 }}>共享管理</Typography.Title>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setDrawerOpen(true)}>
          创建共享
        </Button>
      </div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Input
          placeholder="搜索名称"
          prefix={<SearchOutlined />}
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(1); }}
          allowClear
          style={{ width: 240 }}
        />
        <Select
          placeholder="可见性"
          allowClear
          value={visFilter}
          onChange={(v) => { setVisFilter(v); setPage(1); }}
          style={{ width: 120 }}
          options={[
            { label: '公开', value: 'public' },
            { label: '私有', value: 'private' },
          ]}
        />
        <Select
          placeholder="所属租户"
          allowClear
          showSearch
          optionFilterProp="label"
          value={ownerFilter}
          onChange={(v) => { setOwnerFilter(v); setPage(1); }}
          style={{ width: 200 }}
          options={tenantOptions}
        />
      </Space>
      <Table
        dataSource={data?.shares ?? []}
        columns={columns}
        rowKey="id"
        loading={isLoading}
        pagination={{
          current: page, pageSize, total: data?.total ?? 0,
          onChange: (p, ps) => { setPage(p); setPageSize(ps); },
          showSizeChanger: true, showTotal: (t) => `共 ${t} 个共享`,
        }}
      />

      {/* ── Create Share Drawer ── */}
      <Drawer
        title="创建共享"
        open={drawerOpen}
        onClose={() => { setDrawerOpen(false); form.resetFields(); }}
        width={520}
        extra={
          <Button type="primary" onClick={handleCreate} loading={createMutation.isPending}>
            创建
          </Button>
        }
      >
        <Form form={form} layout="vertical">
          <Form.Item name="owner_tenant_id" label="所属租户" rules={[{ required: true, message: '请选择租户' }]}>
            <Select showSearch optionFilterProp="label" placeholder="选择租户" options={tenantOptions} />
          </Form.Item>
          <Form.Item name="name" label="共享名称" rules={[{ required: true, message: '请输入共享名称' }]}>
            <Input placeholder="例如 shared-models" />
          </Form.Item>
          {/* F12: Source path with browse button */}
          <Form.Item name="source_path" label="源路径" rules={[{ required: true, message: '请输入源路径' }]}>
            <Input
              placeholder="例如 data/shared"
              onChange={handleSourcePathChange}
              addonAfter={
                <Button
                  size="small"
                  type="link"
                  style={{ padding: '0 4px', height: 'auto' }}
                  disabled={!selectedOwnerTenantId}
                  onClick={() => setDirBrowserOpen(true)}
                >
                  浏览
                </Button>
              }
            />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={3} />
          </Form.Item>
          <Form.Item name="visibility" label="可见性" initialValue="private">
            <Select options={[
              { label: '私有', value: 'private' },
              { label: '公开', value: 'public' },
            ]} />
          </Form.Item>
        </Form>
      </Drawer>

      {/* F12: Directory browser modal */}
      <DirBrowserModal
        open={dirBrowserOpen}
        namespaceId={selectedOwnerTenantId ?? null}
        onSelect={handleDirSelected}
        onClose={() => setDirBrowserOpen(false)}
      />
    </div>
  );
}
