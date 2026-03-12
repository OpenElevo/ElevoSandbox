import { useState, useMemo } from 'react';
import { Table, Button, Input, Space, Tag, Typography, Drawer, Form, Select, App, Modal } from 'antd';
import { PlusOutlined, SearchOutlined } from '@ant-design/icons';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listTenants, createTenant, activateTenant, deactivateTenant, deleteTenant } from '@/api/tenants';
import type { Tenant, CreateTenantParams } from '@/types';
import { formatTime } from '@/utils/time';

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

  const queryParams = useMemo(() => {
    const p: Record<string, string | number | boolean> = { page, page_size: pageSize };
    if (search) p.search = search;
    if (statusFilter === 'active') p.is_active = true;
    if (statusFilter === 'inactive') p.is_active = false;
    return p;
  }, [search, statusFilter, page, pageSize]);

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
      message.success('Tenant created');
      if (result.api_key) {
        setTokenModal({ key: result.api_key.key.name, token: result.api_key.token });
      }
    },
    onError: () => message.error('Failed to create tenant'),
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
    const label = tenant.is_active ? 'deactivate' : 'activate';
    modal.confirm({
      title: `${tenant.is_active ? 'Deactivate' : 'Activate'} "${tenant.name}"?`,
      onOk: async () => {
        await action(tenant.id);
        queryClient.invalidateQueries({ queryKey: ['tenants'] });
        message.success(`Tenant ${label}d`);
      },
    });
  };

  const handleDelete = (tenant: Tenant) => {
    let inputName = '';
    modal.confirm({
      title: `Delete "${tenant.name}"?`,
      content: (
        <div>
          <Typography.Text type="danger">This action cannot be undone.</Typography.Text>
          <Input
            placeholder="Type tenant name to confirm"
            style={{ marginTop: 8 }}
            onChange={(e) => { inputName = e.target.value; }}
          />
        </div>
      ),
      okButtonProps: { danger: true },
      onOk: async () => {
        if (inputName !== tenant.name) {
          message.error('Name does not match');
          throw new Error('mismatch');
        }
        await deleteTenant(tenant.id, true);
        queryClient.invalidateQueries({ queryKey: ['tenants'] });
        message.success('Tenant deleted');
      },
    });
  };

  const columns = [
    { title: 'Name', dataIndex: 'name', key: 'name',
      render: (name: string, r: Tenant) => (
        <a onClick={() => navigate(`/admin/tenants/${r.id}`)}>{name}</a>
      ),
    },
    { title: 'Status', dataIndex: 'is_active', key: 'status', width: 100,
      render: (active: boolean) => (
        <Tag color={active ? 'green' : 'default'}>{active ? 'Active' : 'Inactive'}</Tag>
      ),
    },
    { title: 'Shares', dataIndex: 'share_count', key: 'shares', width: 80 },
    { title: 'API Keys', dataIndex: 'active_api_key_count', key: 'keys', width: 90 },
    { title: 'Created', dataIndex: 'created_at', key: 'created', width: 180,
      render: (v: string) => formatTime(v) },
    { title: 'Actions', key: 'actions', width: 200,
      render: (_: unknown, record: Tenant) => (
        <Space size="small">
          <Button size="small" onClick={() => handleToggleStatus(record)}>
            {record.is_active ? 'Deactivate' : 'Activate'}
          </Button>
          <Button size="small" danger onClick={() => handleDelete(record)}>Delete</Button>
        </Space>
      ),
    },
  ];

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16 }}>
        <Typography.Title level={4} style={{ margin: 0 }}>Tenants</Typography.Title>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setDrawerOpen(true)}>
          Create Tenant
        </Button>
      </div>
      <Space style={{ marginBottom: 16 }}>
        <Input
          placeholder="Search by name or ID"
          prefix={<SearchOutlined />}
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(1); }}
          allowClear
          style={{ width: 280 }}
        />
        <Select
          placeholder="Status"
          allowClear
          value={statusFilter}
          onChange={(v) => { setStatusFilter(v); setPage(1); }}
          style={{ width: 120 }}
          options={[
            { label: 'Active', value: 'active' },
            { label: 'Inactive', value: 'inactive' },
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
          showSizeChanger: true, showTotal: (t) => `${t} tenants`,
        }}
      />
      <Drawer
        title="Create Tenant"
        open={drawerOpen}
        onClose={() => setDrawerOpen(false)}
        width={480}
        extra={
          <Button type="primary" onClick={handleCreate} loading={createMutation.isPending}>
            Create
          </Button>
        }
      >
        <Form form={form} layout="vertical">
          <Form.Item name="name" label="Name" rules={[{ required: true, message: 'Name is required' }]}>
            <Input placeholder="Tenant name" />
          </Form.Item>
          <Form.Item name="description" label="Description">
            <Input.TextArea rows={3} placeholder="Optional description" />
          </Form.Item>
          <Form.Item name="api_key_name" label="Initial API Key Name">
            <Input placeholder="e.g. default (leave empty to skip)" />
          </Form.Item>
        </Form>
      </Drawer>
      <Modal
        title="API Key Created"
        open={!!tokenModal}
        onOk={() => setTokenModal(null)}
        onCancel={() => setTokenModal(null)}
        cancelButtonProps={{ style: { display: 'none' } }}
      >
        <Typography.Paragraph>
          Key <strong>{tokenModal?.key}</strong> created. Copy the token now — it won't be shown again.
        </Typography.Paragraph>
        <Input.TextArea value={tokenModal?.token} readOnly rows={2} />
      </Modal>
    </div>
  );
}
