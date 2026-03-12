import { useState, useMemo } from 'react';
import { Table, Button, Input, Space, Tag, Typography, Drawer, Form, Select, App, Modal, Checkbox } from 'antd';
import { PlusOutlined, SearchOutlined } from '@ant-design/icons';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listTenants, createTenant, activateTenant, deactivateTenant, deleteTenant } from '@/api/tenants';
import type { Tenant, CreateTenantParams } from '@/types';
import { formatTime } from '@/utils/time';
import { useDebounce } from '@/hooks/useDebounce';

export default function TenantList() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<string>();
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [form] = Form.useForm();
  const [tokenModal, setTokenModal] = useState<{ key: string; token: string } | null>(null);
  const [tokenAcked, setTokenAcked] = useState(false);

  const debouncedSearch = useDebounce(search);

  const queryParams = useMemo(() => {
    const p: Record<string, string | number | boolean> = { page, page_size: pageSize };
    if (debouncedSearch) p.search = debouncedSearch;
    if (statusFilter === 'active') p.is_active = true;
    if (statusFilter === 'inactive') p.is_active = false;
    return p;
  }, [debouncedSearch, statusFilter, page, pageSize]);

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
      const params: CreateTenantParams = { name: values.name, description: values.description };
      if (values.api_key_name) {
        params.initial_api_key = { name: values.api_key_name };
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

  const handleDelete = (tenant: Tenant) => {
    let inputName = '';
    modal.confirm({
      title: `删除租户「${tenant.name}」？`,
      content: (
        <div>
          <Typography.Text type="danger">此操作不可逆，请谨慎操作。</Typography.Text>
          <Input
            placeholder="请输入租户名称确认"
            style={{ marginTop: 8 }}
            onChange={(e) => { inputName = e.target.value; }}
          />
        </div>
      ),
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        if (inputName !== tenant.name) {
          message.error('名称不匹配');
          throw new Error('mismatch');
        }
        await deleteTenant(tenant.id, true);
        queryClient.invalidateQueries({ queryKey: ['tenants'] });
        message.success('租户已删除');
      },
    });
  };

  const columns = [
    { title: '名称', dataIndex: 'name', key: 'name',
      render: (name: string, r: Tenant) => (
        <a onClick={() => navigate(`/admin/tenants/${r.id}`)}>{name}</a>
      ),
    },
    { title: '状态', dataIndex: 'is_active', key: 'status', width: 100,
      render: (active: boolean) => (
        <Tag color={active ? 'green' : 'default'}>{active ? '活跃' : '已停用'}</Tag>
      ),
    },
    { title: '共享数', dataIndex: 'share_count', key: 'shares', width: 80 },
    { title: 'API Key', dataIndex: 'active_api_key_count', key: 'keys', width: 90 },
    { title: '创建时间', dataIndex: 'created_at', key: 'created', width: 180,
      render: (v: string) => formatTime(v) },
    { title: '操作', key: 'actions', width: 200,
      render: (_: unknown, record: Tenant) => (
        <Space size="small">
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
      <Space style={{ marginBottom: 16 }}>
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
      <Drawer
        title="创建租户"
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        width={480}
        extra={
          <Button type="primary" onClick={handleCreate} loading={createMutation.isPending}>
            创建
          </Button>
        }
      >
        <Form form={form} layout="vertical">
          <Form.Item name="name" label="名称" rules={[{ required: true, message: '请输入租户名称' }]}>
            <Input placeholder="租户名称" />
          </Form.Item>
          <Form.Item name="description" label="描述">
            <Input.TextArea rows={3} placeholder="可选描述" />
          </Form.Item>
          <Form.Item name="api_key_name" label="初始 API Key 名称">
            <Input placeholder="例如 default（留空则不创建）" />
          </Form.Item>
        </Form>
      </Drawer>
      <Modal
        title="API Key 已创建"
        open={!!tokenModal}
        onOk={() => setTokenModal(null)}
        onCancel={() => setTokenModal(null)}
        maskClosable={false}
        keyboard={false}
        okButtonProps={{ disabled: !tokenAcked }}
        cancelButtonProps={{ style: { display: 'none' } }}
      >
        <Typography.Paragraph>
          Key <strong>{tokenModal?.key}</strong> 已创建。请立即复制 Token，关闭后将无法再次查看。
        </Typography.Paragraph>
        <Input.TextArea value={tokenModal?.token} readOnly rows={2} />
        <Checkbox
          checked={tokenAcked}
          onChange={(e) => setTokenAcked(e.target.checked)}
          style={{ marginTop: 12 }}
        >
          我已安全保存此 Token
        </Checkbox>
      </Modal>
    </div>
  );
}
