import { useState, useMemo } from 'react';
import { Table, Button, Input, Space, Tag, Typography, Drawer, Form, Select, App } from 'antd';
import { PlusOutlined, SearchOutlined } from '@ant-design/icons';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listShares, createShare, deleteShare } from '@/api/shares';
import { listTenants } from '@/api/tenants';
import type { Share, CreateShareParams } from '@/types';
import { formatTime } from '@/utils/time';

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
  const [form] = Form.useForm();

  const queryParams = useMemo(() => {
    const p: Record<string, string | number> = { page, page_size: pageSize };
    if (search) p.search = search;
    if (visFilter) p.visibility = visFilter;
    if (ownerFilter) p.owner_tenant_id = ownerFilter;
    return p;
  }, [search, visFilter, ownerFilter, page, pageSize]);

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
      message.success('Share created');
    },
    onError: (err: { response?: { data?: { error?: { message?: string } } } }) => {
      message.error(err.response?.data?.error?.message || 'Failed to create share');
    },
  });

  const handleCreate = () => {
    form.validateFields().then((values) => {
      createMutation.mutate(values);
    });
  };

  const handleDelete = (share: Share) => {
    modal.confirm({
      title: `Delete share "${share.name}"?`,
      okButtonProps: { danger: true },
      onOk: async () => {
        await deleteShare(share.id);
        queryClient.invalidateQueries({ queryKey: ['shares'] });
        message.success('Share deleted');
      },
    });
  };

  const tenantOptions = (tenantsData?.tenants ?? []).map((t) => ({
    label: t.name,
    value: t.id,
  }));

  const columns = [
    { title: 'Name', dataIndex: 'name', key: 'name',
      render: (name: string, r: Share) => (
        <a onClick={() => navigate(`/admin/shares/${r.id}`)}>{name}</a>
      ),
    },
    { title: 'Owner', dataIndex: 'owner_tenant_id', key: 'owner', width: 200,
      render: (tid: string) => {
        const t = tenantsData?.tenants.find((x) => x.id === tid);
        return t ? <a onClick={() => navigate(`/admin/tenants/${tid}`)}>{t.name}</a> : tid.slice(0, 8);
      },
    },
    { title: 'Source Path', dataIndex: 'source_path', key: 'path' },
    { title: 'Visibility', dataIndex: 'visibility', key: 'vis', width: 100,
      render: (v: string) => <Tag color={v === 'public' ? 'blue' : 'default'}>{v}</Tag>,
    },
    { title: 'Created', dataIndex: 'created_at', key: 'created', width: 180,
      render: (v: string) => formatTime(v),
    },
    { title: 'Actions', key: 'actions', width: 100,
      render: (_: unknown, record: Share) => (
        <Button size="small" danger onClick={() => handleDelete(record)}>Delete</Button>
      ),
    },
  ];

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16 }}>
        <Typography.Title level={4} style={{ margin: 0 }}>Shares</Typography.Title>
        <Button type="primary" icon={<PlusOutlined />} onClick={() => setDrawerOpen(true)}>
          Create Share
        </Button>
      </div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Input
          placeholder="Search by name"
          prefix={<SearchOutlined />}
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(1); }}
          allowClear
          style={{ width: 240 }}
        />
        <Select
          placeholder="Visibility"
          allowClear
          value={visFilter}
          onChange={(v) => { setVisFilter(v); setPage(1); }}
          style={{ width: 120 }}
          options={[
            { label: 'Public', value: 'public' },
            { label: 'Private', value: 'private' },
          ]}
        />
        <Select
          placeholder="Owner"
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
          showSizeChanger: true, showTotal: (t) => `${t} shares`,
        }}
      />
      <Drawer
        title="Create Share"
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
          <Form.Item name="owner_tenant_id" label="Owner Tenant" rules={[{ required: true }]}>
            <Select showSearch optionFilterProp="label" placeholder="Select tenant" options={tenantOptions} />
          </Form.Item>
          <Form.Item name="name" label="Share Name" rules={[{ required: true }]}>
            <Input placeholder="e.g. shared-models" />
          </Form.Item>
          <Form.Item name="source_path" label="Source Path" rules={[{ required: true }]}>
            <Input placeholder="e.g. data/shared" />
          </Form.Item>
          <Form.Item name="description" label="Description">
            <Input.TextArea rows={3} />
          </Form.Item>
          <Form.Item name="visibility" label="Visibility" initialValue="private">
            <Select options={[
              { label: 'Private', value: 'private' },
              { label: 'Public', value: 'public' },
            ]} />
          </Form.Item>
        </Form>
      </Drawer>
    </div>
  );
}
