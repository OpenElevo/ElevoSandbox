import { useState, useEffect } from 'react';
import { useParams, useNavigate, useSearchParams } from 'react-router-dom';
import {
  Card, Descriptions, Tag, Button, Space, Tabs, App, Modal,
  Drawer, Form, Input, Table, Checkbox, DatePicker, Alert, Select,
} from 'antd';
import { CopyOutlined, CheckOutlined, EyeOutlined, EyeInvisibleOutlined } from '@ant-design/icons';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  getTenant, updateTenant, activateTenant, deactivateTenant,
  listApiKeys, createApiKey, revokeApiKey, getApiKeyToken, listTenantPermissions,
} from '@/api/tenants';
import { listShares } from '@/api/shares';
import type { ApiKey, SharePermission } from '@/types';
import { PERMISSION_COLORS, PERMISSION_LABELS } from '@/utils/constants';
import { formatTime } from '@/utils/time';
import FileBrowser from '@/components/FileBrowser/FileBrowser';
import DirtyFormGuard from '@/components/DirtyFormGuard';
import { useBreadcrumbStore } from '@/stores/breadcrumbStore';
import { useDeleteTenant } from '@/hooks/useDeleteTenant';
import dayjs from 'dayjs';

function copyToClipboard(text: string): Promise<void> {
  if (navigator.clipboard && window.isSecureContext) {
    return navigator.clipboard.writeText(text);
  }
  const ta = document.createElement('textarea');
  ta.value = text;
  ta.style.cssText = 'position:fixed;left:-9999px;opacity:0';
  document.body.appendChild(ta);
  ta.select();
  document.execCommand('copy');
  document.body.removeChild(ta);
  return Promise.resolve();
}

function TokenCell({ tenantId, record, message }: { tenantId: string; record: ApiKey; message: ReturnType<typeof App.useApp>['message'] }) {
  const [visible, setVisible] = useState(false);
  const [token, setToken] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [loading, setLoading] = useState(false);

  const handleFetch = async () => {
    if (visible || token) { setVisible(true); return; }
    setLoading(true);
    try {
      const t = await getApiKeyToken(tenantId, record.id);
      setToken(t);
      setVisible(true);
    } catch {
      message.error('获取 Token 失败');
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = async () => {
    try {
      let t = token;
      if (!t) {
        setLoading(true);
        t = await getApiKeyToken(tenantId, record.id);
        setToken(t);
        setLoading(false);
      }
      await copyToClipboard(t);
      setCopied(true);
      message.success('Token 已复制');
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setLoading(false);
      message.error('复制失败');
    }
  };

  return (
    <Space size={4}>
      <code style={{ fontSize: 12 }}>
        {visible && token ? token : record.token_prefix}
      </code>
      <Button
        type="text"
        size="small"
        icon={visible ? <EyeInvisibleOutlined /> : <EyeOutlined />}
        onClick={handleFetch}
        loading={loading}
        style={{ padding: 0 }}
      />
      <Button
        type="text"
        size="small"
        icon={copied ? <CheckOutlined style={{ color: '#52c41a' }} /> : <CopyOutlined />}
        onClick={handleCopy}
        loading={loading}
        style={{ padding: 0 }}
      />
    </Space>
  );
}

function getApiKeyStatus(key: ApiKey): { label: string; color: string } {
  if (!key.is_active) return { label: '已撤销', color: 'default' };
  if (key.expires_at && dayjs(key.expires_at).isBefore(dayjs())) {
    return { label: '已过期', color: 'orange' };
  }
  return { label: '活跃', color: 'green' };
}

// ─── Token display modal with copy button ──────────────────────────────────────

interface TokenModalData {
  name: string;
  token: string;
}

interface TokenDisplayModalProps {
  data: TokenModalData | null;
  acked: boolean;
  onAckedChange: (v: boolean) => void;
  onClose: () => void;
}

function TokenDisplayModal({ data, acked, onAckedChange, onClose }: TokenDisplayModalProps) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    if (!data) return;
    navigator.clipboard.writeText(data.token).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  };

  return (
    <Modal
      title="API Key 已创建"
      open={!!data}
      onOk={onClose}
      onCancel={onClose}
      maskClosable={false}
      keyboard={false}
      okButtonProps={{ disabled: !acked }}
      cancelButtonProps={{ style: { display: 'none' } }}
    >
      <Alert
        type="warning"
        message="API Key 创建成功"
        description="请立即复制并妥善保存以下 Token。你也可以随时通过列表中的复制按钮再次获取。"
        showIcon
        style={{ marginBottom: 12 }}
      />
      <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 12 }}>
        <Input
          readOnly
          value={data?.token ?? ''}
          onClick={(e) => (e.target as HTMLInputElement).select()}
          style={{ fontFamily: 'monospace' }}
        />
        <Button
          icon={copied ? <CheckOutlined /> : <CopyOutlined />}
          onClick={handleCopy}
          type={copied ? 'primary' : 'default'}
        >
          {copied ? '已复制 ✓' : '复制'}
        </Button>
      </div>
      <Checkbox checked={acked} onChange={(e) => onAckedChange(e.target.checked)}>
        我已安全保存此 Token
      </Checkbox>
    </Modal>
  );
}

// ─── Main component ────────────────────────────────────────────────────────────

export default function TenantDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = searchParams.get('tab') || 'keys';
  const setBreadcrumbName = useBreadcrumbStore((s) => s.setBreadcrumbName);

  const [editOpen, setEditOpen] = useState(false);
  const [editDirty, setEditDirty] = useState(false);
  const [editForm] = Form.useForm();
  const [keyForm] = Form.useForm();
  const [keyDrawerOpen, setKeyDrawerOpen] = useState(false);
  const [tokenModal, setTokenModal] = useState<TokenModalData | null>(null);
  const [tokenAcked, setTokenAcked] = useState(false);

  const { data: tenant, isLoading } = useQuery({
    queryKey: ['tenant', id],
    queryFn: () => getTenant(id!),
    enabled: !!id,
  });

  useEffect(() => {
    if (id && tenant?.name) {
      setBreadcrumbName(id, tenant.name);
    }
  }, [id, tenant?.name, setBreadcrumbName]);

  const { data: apiKeys } = useQuery({
    queryKey: ['api-keys', id],
    queryFn: () => listApiKeys(id!),
    enabled: !!id && activeTab === 'keys',
  });

  const { data: sharesData } = useQuery({
    queryKey: ['tenant-shares', id],
    queryFn: () => listShares({ owner_tenant_id: id! }),
    enabled: !!id && activeTab === 'shares',
  });

  const { data: permissionsData } = useQuery({
    queryKey: ['tenant-permissions', id],
    queryFn: () => listTenantPermissions(id!),
    enabled: !!id && activeTab === 'permissions',
  });

  const updateMutation = useMutation({
    mutationFn: (params: { name?: string; description?: string }) => updateTenant(id!, params),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tenant', id] });
      setEditOpen(false);
      setEditDirty(false);
      message.success('租户已更新');
    },
  });

  const createKeyMutation = useMutation({
    mutationFn: (params: { name: string; expires_at?: string }) => createApiKey(id!, params),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['api-keys', id] });
      setKeyDrawerOpen(false);
      keyForm.resetFields();
      setTokenAcked(false);
      setTokenModal({ name: result.key.name, token: result.token });
    },
  });

  const handleEdit = () => {
    if (!tenant) return;
    editForm.setFieldsValue({ name: tenant.name, description: tenant.description });
    setEditDirty(false);
    setEditOpen(true);
  };

  const handleToggle = () => {
    if (!tenant) return;
    const action = tenant.is_active ? deactivateTenant : activateTenant;
    const label = tenant.is_active ? '停用' : '启用';
    modal.confirm({
      title: `${label}租户「${tenant.name}」？`,
      onOk: async () => {
        await action(id!);
        queryClient.invalidateQueries({ queryKey: ['tenant', id] });
        message.success(`租户已${label}`);
      },
    });
  };

  const deleteTenantHandler = useDeleteTenant({ navigateAfterDelete: '/admin/tenants' });

  const handleDelete = () => {
    if (!tenant) return;
    deleteTenantHandler(tenant);
  };

  const handleCreateKey = () => {
    keyForm.validateFields().then((values) => {
      const params: { name: string; expires_at?: string } = { name: values.name };
      if (values.expires_at) {
        params.expires_at = (values.expires_at as ReturnType<typeof dayjs>).toISOString();
      }
      createKeyMutation.mutate(params);
    });
  };

  const handleRevokeKey = (key: ApiKey) => {
    modal.confirm({
      title: `撤销 Key「${key.name}」？`,
      content: '撤销后该 Key 将立即失效，此操作不可逆。',
      okText: '撤销',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        await revokeApiKey(id!, key.id);
        queryClient.invalidateQueries({ queryKey: ['api-keys', id] });
        message.success('Key 已撤销');
      },
    });
  };

  if (isLoading || !tenant) return <Card loading />;

  const keyColumns = [
    { title: '名称', dataIndex: 'name', key: 'name' },
    {
      title: 'Token', key: 'token',
      render: (_: unknown, record: ApiKey) => (
        <TokenCell tenantId={id!} record={record} message={message} />
      ),
    },
    {
      title: '状态', key: 'status',
      render: (_: unknown, record: ApiKey) => {
        const status = getApiKeyStatus(record);
        return <Tag color={status.color}>{status.label}</Tag>;
      },
    },
    {
      title: '过期时间', dataIndex: 'expires_at', key: 'expires',
      render: (v: string | null) => v ? formatTime(v) : '永不过期',
    },
    {
      title: '最近使用', dataIndex: 'last_used_at', key: 'last_used',
      render: (v: string | null) => v ? formatTime(v) : '从未使用',
    },
    { title: '创建时间', dataIndex: 'created_at', key: 'created', render: (v: string) => formatTime(v) },
    {
      title: '操作', key: 'actions',
      render: (_: unknown, record: ApiKey) => {
        const isActive = record.is_active && (!record.expires_at || dayjs(record.expires_at).isAfter(dayjs()));
        return isActive ? (
          <Button size="small" danger onClick={() => handleRevokeKey(record)}>撤销</Button>
        ) : null;
      },
    },
  ];

  const shareColumns = [
    {
      title: '名称', dataIndex: 'name', key: 'name',
      render: (name: string, r: { id: string }) => (
        <a onClick={() => navigate(`/admin/shares/${r.id}`)}>{name}</a>
      ),
    },
    { title: '源路径', dataIndex: 'source_path', key: 'path' },
    {
      title: '可见性', dataIndex: 'visibility', key: 'vis',
      render: (v: string) => <Tag color={v === 'public' ? 'blue' : 'default'}>{v === 'public' ? '公开' : '私有'}</Tag>,
    },
    { title: '创建时间', dataIndex: 'created_at', key: 'created', render: (v: string) => formatTime(v) },
  ];

  const permissionColumns = [
    {
      title: '共享', dataIndex: 'share_id', key: 'share',
      render: (sid: string, record: SharePermission) => (
        <a onClick={() => navigate(`/admin/shares/${sid}`)}>
          {record.share_name || sid.slice(0, 8) + '...'}
        </a>
      ),
    },
    {
      title: '权限', dataIndex: 'permission', key: 'permission',
      render: (p: string) => (
        <Tag color={(PERMISSION_COLORS as Record<string, string>)[p] || 'default'}>
          {(PERMISSION_LABELS as Record<string, string>)[p] || p}
        </Tag>
      ),
    },
    { title: '授予时间', dataIndex: 'created_at', key: 'created', render: (v: string) => formatTime(v) },
  ];

  return (
    <div>
      <Button type="link" onClick={() => navigate('/admin/tenants')} style={{ padding: 0, marginBottom: 8 }}>
        &larr; 返回租户列表
      </Button>
      <Card
        title={tenant.name}
        extra={
          <Space>
            <Button onClick={handleEdit}>编辑</Button>
            <Button onClick={handleToggle}>{tenant.is_active ? '停用' : '启用'}</Button>
            <Button danger onClick={handleDelete}>删除</Button>
          </Space>
        }
      >
        <Descriptions column={2}>
          <Descriptions.Item label="ID">{tenant.id}</Descriptions.Item>
          <Descriptions.Item label="状态">
            <Tag color={tenant.is_active ? 'green' : 'default'}>{tenant.is_active ? '活跃' : '已停用'}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="存储类型">{tenant.storage_type === 'remote' ? '远程' : '托管'}</Descriptions.Item>
          <Descriptions.Item label="描述" span={2}>{tenant.description || '-'}</Descriptions.Item>
          <Descriptions.Item label="创建时间">{formatTime(tenant.created_at)}</Descriptions.Item>
          <Descriptions.Item label="更新时间">{formatTime(tenant.updated_at)}</Descriptions.Item>
        </Descriptions>
      </Card>

      <Tabs
        activeKey={activeTab}
        onChange={(key) => setSearchParams({ tab: key })}
        style={{ marginTop: 16 }}
        items={[
          {
            key: 'keys', label: 'API Keys', children: (
              <div>
                <Button type="primary" size="small" onClick={() => {
                  keyForm.setFieldsValue({ name: `key-${Date.now().toString(36)}` });
                  setKeyDrawerOpen(true);
                }} style={{ marginBottom: 12 }}>
                  创建 Key
                </Button>
                <Table dataSource={apiKeys ?? []} columns={keyColumns} rowKey="id" size="small" pagination={false} />
              </div>
            ),
          },
          {
            key: 'shares', label: '共享', children: (
              <Table dataSource={sharesData?.shares ?? []} columns={shareColumns} rowKey="id" size="small" pagination={false} />
            ),
          },
          {
            key: 'permissions', label: '权限', children: (
              <Table
                dataSource={permissionsData ?? []}
                columns={permissionColumns}
                rowKey={(r: SharePermission) => `${r.share_id}-${r.tenant_id}`}
                size="small"
                pagination={false}
                locale={{ emptyText: '该租户暂无授权记录' }}
              />
            ),
          },
          {
            key: 'files', label: '命名空间文件', children: (
              <FileBrowser namespaceId={id!} />
            ),
          },
        ]}
      />

      <Drawer
        title="编辑租户"
        open={editOpen}
        onClose={() => setEditOpen(false)}
        width={400}
        extra={
          <Button
            type="primary"
            onClick={() => editForm.validateFields().then((v) => updateMutation.mutate(v))}
            loading={updateMutation.isPending}
          >
            保存
          </Button>
        }
      >
        <DirtyFormGuard dirty={editDirty && editOpen} />
        <Form form={editForm} layout="vertical" onValuesChange={() => setEditDirty(true)}>
          <Form.Item name="name" label="名称" rules={[{ required: true }]}><Input /></Form.Item>
          <Form.Item name="description" label="描述"><Input.TextArea rows={3} /></Form.Item>
          <Form.Item name="storage_type" label="存储类型">
            <Select
              options={[
                { value: 'managed', label: '托管 (本地存储)' },
                { value: 'remote', label: '远程 (gRPC 存储)' },
              ]}
            />
          </Form.Item>
        </Form>
      </Drawer>

      <Drawer
        title="创建 API Key"
        open={keyDrawerOpen}
        onClose={() => setKeyDrawerOpen(false)}
        width={400}
        extra={
          <Button type="primary" onClick={handleCreateKey} loading={createKeyMutation.isPending}>
            创建
          </Button>
        }
      >
        <Form form={keyForm} layout="vertical">
          <Form.Item name="name" label="Key 名称" rules={[{ required: true, message: '请输入 Key 名称' }]}>
            <Input placeholder="例如 production" />
          </Form.Item>
          <Form.Item name="expires_at" label="过期时间" extra="不填写则永不过期">
            <DatePicker
              showTime
              placeholder="留空则永不过期"
              disabledDate={(current) => current && current.isBefore(dayjs(), 'day')}
              style={{ width: '100%' }}
            />
          </Form.Item>
        </Form>
      </Drawer>

      <TokenDisplayModal
        data={tokenModal}
        acked={tokenAcked}
        onAckedChange={setTokenAcked}
        onClose={() => setTokenModal(null)}
      />
    </div>
  );
}
