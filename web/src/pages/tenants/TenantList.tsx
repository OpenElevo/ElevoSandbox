import { useState, useMemo, useRef } from 'react';
import {
  Table, Button, Input, Space, Tag, Typography, Drawer, Form, Select,
  App, Modal, Checkbox, DatePicker, Radio, Alert,
} from 'antd';
import { PlusOutlined, SearchOutlined, CopyOutlined, CheckOutlined } from '@ant-design/icons';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import dayjs from 'dayjs';
import { listTenants, createTenant, activateTenant, deactivateTenant } from '@/api/tenants';
import type { Tenant, CreateTenantParams } from '@/types';
import { formatTime } from '@/utils/time';
import { useDebounce } from '@/hooks/useDebounce';
import { usePagination } from '@/hooks/usePagination';
import { useDeleteTenant } from '@/hooks/useDeleteTenant';

// ─── Token display modal with copy button ──────────────────────────────────────

interface TokenModalData {
  key: string;
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
        description="请立即复制并妥善保存以下 Token，此 Token 仅展示一次，关闭后无法再查看。"
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

export default function TenantList() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>();
  const [storageFilter, setStorageFilter] = useState<string>();
  const { page, pageSize, setPage, setPageSize } = usePagination();
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [form] = Form.useForm();
  const [createApiKey, setCreateApiKey] = useState(false);
  const [storageType, setStorageType] = useState<'managed' | 'remote'>('managed');
  const [tokenModal, setTokenModal] = useState<TokenModalData | null>(null);
  const [tokenAcked, setTokenAcked] = useState(false);

  // We need a ref to track the "createApiKey" checkbox value inside Form.Item
  const createApiKeyRef = useRef(false);
  createApiKeyRef.current = createApiKey;

  const debouncedSearch = useDebounce(search);

  const queryParams = useMemo(() => {
    const p: Record<string, string | number | boolean> = { page, page_size: pageSize };
    if (debouncedSearch) p.search = debouncedSearch;
    if (statusFilter === 'active') p.is_active = true;
    if (statusFilter === 'inactive') p.is_active = false;
    if (storageFilter) p.storage_type = storageFilter;
    return p;
  }, [debouncedSearch, statusFilter, storageFilter, page, pageSize]);

  const { data, isLoading } = useQuery({
    queryKey: ['tenants', queryParams],
    queryFn: () => listTenants(queryParams),
  });

  const createMutation = useMutation({
    mutationFn: (params: CreateTenantParams) => createTenant(params),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['tenants'] });
      setDrawerOpen(false);
      form.resetFields();
      setCreateApiKey(false);
      setStorageType('managed');
      message.success('租户已创建');
      if (result.api_key) {
        setTokenAcked(false);
        setTokenModal({ key: result.api_key.key.name, token: result.api_key.token });
      }
    },
    onError: () => message.error('创建租户失败'),
  });

  const handleCreate = () => {
    form.validateFields().then((values) => {
      const params: CreateTenantParams = {
        name: values.name,
        description: values.description,
        storage_type: values.storage_type,
      };

      if (values.storage_type === 'remote' && values.storage_config_json) {
        try {
          params.storage_config = JSON.parse(values.storage_config_json);
        } catch {
          message.error('存储配置 JSON 格式无效');
          return;
        }
      }

      if (createApiKeyRef.current && values.api_key_name) {
        params.initial_api_key = { name: values.api_key_name };
        if (values.api_key_expires_at) {
          params.initial_api_key.expires_at = (values.api_key_expires_at as ReturnType<typeof dayjs>).toISOString();
        }
      }

      createMutation.mutate(params);
    });
  };

  const handleToggleStatus = (tenant: Tenant) => {
    const action = tenant.is_active ? deactivateTenant : activateTenant;
    const label = tenant.is_active ? '停用' : '启用';
    modal.confirm({
      title: `${label}租户「${tenant.name}」？`,
      onOk: async () => {
        await action(tenant.id);
        queryClient.invalidateQueries({ queryKey: ['tenants'] });
        message.success(`租户已${label}`);
      },
    });
  };

  const handleDelete = useDeleteTenant();

  const columns = [
    {
      title: '名称', dataIndex: 'name', key: 'name',
      render: (name: string, r: Tenant) => (
        <a onClick={() => navigate(`/admin/tenants/${r.id}`)}>{name}</a>
      ),
    },
    {
      title: '描述', dataIndex: 'description', key: 'description', width: 200,
      render: (desc: string) => {
        if (!desc) return '-';
        return desc.length > 50
          ? <span title={desc}>{desc.slice(0, 50)}...</span>
          : desc;
      },
    },
    {
      title: '存储类型', dataIndex: 'storage_type', key: 'storage_type', width: 80,
      render: (v: string) => (v === 'remote' ? '远程' : '托管'),
    },
    {
      title: '状态', dataIndex: 'is_active', key: 'status', width: 100,
      render: (active: boolean) => (
        <Tag color={active ? 'green' : 'default'}>{active ? '活跃' : '已停用'}</Tag>
      ),
    },
    { title: '共享数', dataIndex: 'share_count', key: 'shares', width: 80 },
    { title: 'API Key', dataIndex: 'active_api_key_count', key: 'keys', width: 90 },
    {
      title: '创建时间', dataIndex: 'created_at', key: 'created', width: 180,
      sorter: true,
      render: (v: string) => formatTime(v),
    },
    {
      title: '操作', key: 'actions', width: 220,
      render: (_: unknown, record: Tenant) => (
        <Space size="small">
          <Button size="small" type="link" onClick={() => navigate(`/admin/tenants/${record.id}`)}>查看</Button>
          <Button size="small" onClick={() => handleToggleStatus(record)}>
            {record.is_active ? '停用' : '启用'}
          </Button>
          <Button size="small" danger onClick={() => handleDelete(record)}>删除</Button>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16 }}>
        <Typography.Title level={4} style={{ margin: 0 }}>租户管理</Typography.Title>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setDrawerOpen(true)}>
          创建租户
        </Button>
      </div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Input
          placeholder="搜索名称或 ID"
          prefix={<SearchOutlined />}
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(1); }}
          allowClear
          style={{ width: 280 }}
        />
        <Select
          placeholder="状态"
          allowClear
          value={statusFilter}
          onChange={(v) => { setStatusFilter(v); setPage(1); }}
          style={{ width: 120 }}
          options={[
            { label: '活跃', value: 'active' },
            { label: '已停用', value: 'inactive' },
          ]}
        />
        <Select
          placeholder="存储类型"
          allowClear
          value={storageFilter}
          onChange={(v) => { setStorageFilter(v); setPage(1); }}
          style={{ width: 120 }}
          options={[
            { label: '全部', value: undefined },
            { label: '托管', value: 'managed' },
            { label: '远程', value: 'remote' },
          ].filter((o) => o.value !== undefined)}
        />
      </Space>
      <Table
        dataSource={data?.tenants ?? []}
        columns={columns}
        rowKey="id"
        loading={isLoading}
        pagination={{
          current: page, pageSize, total: data?.total ?? 0,
          onChange: (p, ps) => { setPage(p); setPageSize(ps); },
          showSizeChanger: true, showTotal: (t) => `共 ${t} 个租户`,
        }}
      />

      {/* ── Create Tenant Drawer ── */}
      <Drawer
        title="创建租户"
        open={drawerOpen}
        onClose={() => {
          setDrawerOpen(false);
          form.resetFields();
          setCreateApiKey(false);
          setStorageType('managed');
        }}
        width={520}
        extra={
          <Button type="primary" onClick={handleCreate} loading={createMutation.isPending}>
            创建
          </Button>
        }
      >
        <Form
          form={form}
          layout="vertical"
          initialValues={{ storage_type: 'managed' }}
        >
          <Form.Item name="name" label="名称" rules={[{ required: true, message: '请输入租户名称' }]}>
            <Input placeholder="租户名称" />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={3} placeholder="可选描述" />
          </Form.Item>

          {/* Storage type */}
          <Form.Item name="storage_type" label="存储类型">
            <Radio.Group onChange={(e) => setStorageType(e.target.value)}>
              <Radio value="managed">托管</Radio>
              <Radio value="remote">远程</Radio>
            </Radio.Group>
          </Form.Item>

          {/* Remote storage config — shown only when "远程" is selected */}
          {storageType === 'remote' && (
            <Form.Item
              name="storage_config_json"
              label="远程存储配置（JSON）"
              rules={[
                { required: true, message: '请输入存储配置' },
                {
                  validator: (_, value) => {
                    if (!value) return Promise.resolve();
                    try { JSON.parse(value); return Promise.resolve(); }
                    catch { return Promise.reject(new Error('无效的 JSON 格式')); }
                  },
                },
              ]}
            >
              <Input.TextArea rows={5} placeholder='{"type": "s3", "bucket": "my-bucket", ...}' style={{ fontFamily: 'monospace' }} />
            </Form.Item>
          )}

          {/* API Key creation checkbox + conditional fields */}
          <Form.Item style={{ marginBottom: 0 }}>
            <Checkbox
              checked={createApiKey}
              onChange={(e) => {
                setCreateApiKey(e.target.checked);
                if (e.target.checked) {
                  form.setFieldValue('api_key_name', `key-${Date.now().toString(36)}`);
                }
              }}
            >
              同时创建第一个 API Key
            </Checkbox>
          </Form.Item>

          {createApiKey && (
            <>
              <Form.Item
                name="api_key_name"
                label="Key 名称"
                style={{ marginTop: 12 }}
                rules={[{ required: true, message: '请输入 Key 名称' }]}
              >
                <Input placeholder="例如 default" />
              </Form.Item>
              <Form.Item
                name="api_key_expires_at"
                label="过期时间"
                extra="不填写则永不过期"
              >
                <DatePicker
                  showTime
                  placeholder="留空则永不过期"
                  disabledDate={(current) => current && current.isBefore(dayjs(), 'day')}
                  style={{ width: '100%' }}
                />
              </Form.Item>
            </>
          )}
        </Form>
      </Drawer>

      {/* ── Token display modal ── */}
      <TokenDisplayModal
        data={tokenModal}
        acked={tokenAcked}
        onAckedChange={setTokenAcked}
        onClose={() => setTokenModal(null)}
      />
    </div>
  );
}
