import { useState } from 'react';
import { useParams, useNavigate, useSearchParams } from 'react-router-dom';
import { Card, Descriptions, Tag, Button, Space, Tabs, Typography, App, Drawer, Form, Input, Table, Modal } from 'antd';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { getTenant, updateTenant, activateTenant, deactivateTenant, deleteTenant, listApiKeys, createApiKey, revokeApiKey } from '@/api/tenants';
import { listShares } from '@/api/shares';
import type { ApiKey } from '@/types';
import { formatTime } from '@/utils/time';
import FileBrowser from '@/components/FileBrowser/FileBrowser';

export default function TenantDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = searchParams.get('tab') || 'keys';

  const [editOpen, setEditOpen] = useState(false);
  const [editForm] = Form.useForm();
  const [keyForm] = Form.useForm();
  const [keyDrawerOpen, setKeyDrawerOpen] = useState(false);
  const [tokenModal, setTokenModal] = useState<{ name: string; token: string } | null>(null);

  const { data: tenant, isLoading } = useQuery({
    queryKey: ['tenant', id],
    queryFn: () => getTenant(id!),
    enabled: !!id,
  });

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

  const updateMutation = useMutation({
    mutationFn: (params: { name?: string; description?: string }) => updateTenant(id!, params),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['tenant', id] });
      setEditOpen(false);
      message.success('Tenant updated');
    },
  });

  const createKeyMutation = useMutation({
    mutationFn: (params: { name: string }) => createApiKey(id!, params),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ['api-keys', id] });
      setKeyDrawerOpen(false);
      keyForm.resetFields();
      setTokenModal({ name: result.key.name, token: result.token });
    },
  });

  const handleEdit = () => {
    if (!tenant) return;
    editForm.setFieldsValue({ name: tenant.name, description: tenant.description });
    setEditOpen(true);
  };

  const handleToggle = () => {
    if (!tenant) return;
    const action = tenant.is_active ? deactivateTenant : activateTenant;
    const label = tenant.is_active ? 'deactivate' : 'activate';
    modal.confirm({
      title: `${tenant.is_active ? 'Deactivate' : 'Activate'} "${tenant.name}"?`,
      onOk: async () => {
        await action(id!);
        queryClient.invalidateQueries({ queryKey: ['tenant', id] });
        message.success(`Tenant ${label}d`);
      },
    });
  };

  const handleDelete = () => {
    if (!tenant) return;
    let inputName = '';
    modal.confirm({
      title: `Delete "${tenant.name}"?`,
      content: (
        <div>
          <Typography.Text type="danger">This cannot be undone.</Typography.Text>
          <Input style={{ marginTop: 8 }} placeholder="Type tenant name" onChange={(e) => { inputName = e.target.value; }} />
        </div>
      ),
      okButtonProps: { danger: true },
      onOk: async () => {
        if (inputName !== tenant.name) { message.error('Name mismatch'); throw new Error('mismatch'); }
        await deleteTenant(id!, true);
        message.success('Tenant deleted');
        navigate('/admin/tenants');
      },
    });
  };

  const handleRevokeKey = (key: ApiKey) => {
    modal.confirm({
      title: `Revoke key "${key.name}"?`,
      onOk: async () => {
        await revokeApiKey(id!, key.id);
        queryClient.invalidateQueries({ queryKey: ['api-keys', id] });
        message.success('Key revoked');
      },
    });
  };

  if (isLoading || !tenant) return <Card loading />;

  const keyColumns = [
    { title: 'Name', dataIndex: 'name', key: 'name' },
    { title: 'Prefix', dataIndex: 'token_prefix', key: 'prefix' },
    { title: 'Status', dataIndex: 'is_active', key: 'status',
      render: (v: boolean) => <Tag color={v ? 'green' : 'default'}>{v ? 'Active' : 'Revoked'}</Tag> },
    { title: 'Last Used', dataIndex: 'last_used_at', key: 'last_used',
      render: (v: string | null) => v ? formatTime(v) : 'Never' },
    { title: 'Created', dataIndex: 'created_at', key: 'created', render: (v: string) => formatTime(v) },
    { title: 'Actions', key: 'actions',
      render: (_: unknown, record: ApiKey) => record.is_active ? (
        <Button size="small" danger onClick={() => handleRevokeKey(record)}>Revoke</Button>
      ) : null },
  ];

  const shareColumns = [
    { title: 'Name', dataIndex: 'name', key: 'name',
      render: (name: string, r: { id: string }) => <a onClick={() => navigate(`/admin/shares/${r.id}`)}>{name}</a> },
    { title: 'Source Path', dataIndex: 'source_path', key: 'path' },
    { title: 'Visibility', dataIndex: 'visibility', key: 'vis',
      render: (v: string) => <Tag color={v === 'public' ? 'blue' : 'default'}>{v}</Tag> },
    { title: 'Created', dataIndex: 'created_at', key: 'created', render: (v: string) => formatTime(v) },
  ];

  return (
    <div>
      <Button type="link" onClick={() => navigate('/admin/tenants')} style={{ padding: 0, marginBottom: 8 }}>
        &larr; Back to Tenants
      </Button>
      <Card
        title={tenant.name}
        extra={
          <Space>
            <Button onClick={handleEdit}>Edit</Button>
            <Button onClick={handleToggle}>{tenant.is_active ? 'Deactivate' : 'Activate'}</Button>
            <Button danger onClick={handleDelete}>Delete</Button>
          </Space>
        }
      >
        <Descriptions column={2}>
          <Descriptions.Item label="ID">{tenant.id}</Descriptions.Item>
          <Descriptions.Item label="Status">
            <Tag color={tenant.is_active ? 'green' : 'default'}>{tenant.is_active ? 'Active' : 'Inactive'}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="Description" span={2}>{tenant.description || '-'}</Descriptions.Item>
          <Descriptions.Item label="Created">{formatTime(tenant.created_at)}</Descriptions.Item>
          <Descriptions.Item label="Updated">{formatTime(tenant.updated_at)}</Descriptions.Item>
        </Descriptions>
      </Card>

      <Tabs
        activeKey={activeTab}
        onChange={(key) => setSearchParams({ tab: key })}
        style={{ marginTop: 16 }}
        items={[
          { key: 'keys', label: 'API Keys', children: (
            <div>
              <Button type="primary" size="small" onClick={() => setKeyDrawerOpen(true)} style={{ marginBottom: 12 }}>
                Create Key
              </Button>
              <Table dataSource={apiKeys ?? []} columns={keyColumns} rowKey="id" size="small" pagination={false} />
            </div>
          )},
          { key: 'shares', label: 'Shares', children: (
            <Table dataSource={sharesData?.shares ?? []} columns={shareColumns} rowKey="id" size="small" pagination={false} />
          )},
          { key: 'files', label: 'Namespace Files', children: (
            <FileBrowser namespaceId={id!} />
          )},
        ]}
      />

      <Drawer title="Edit Tenant" open={editOpen} onClose={() => setEditOpen(false)} width={400}
        extra={<Button type="primary" onClick={() => editForm.validateFields().then((v) => updateMutation.mutate(v))} loading={updateMutation.isPending}>Save</Button>}>
        <Form form={editForm} layout="vertical">
          <Form.Item name="name" label="Name" rules={[{ required: true }]}><Input /></Form.Item>
          <Form.Item name="description" label="Description"><Input.TextArea rows={3} /></Form.Item>
        </Form>
      </Drawer>

      <Drawer title="Create API Key" open={keyDrawerOpen} onClose={() => setKeyDrawerOpen(false)} width={400}
        extra={<Button type="primary" onClick={() => keyForm.validateFields().then((v) => createKeyMutation.mutate(v))} loading={createKeyMutation.isPending}>Create</Button>}>
        <Form form={keyForm} layout="vertical">
          <Form.Item name="name" label="Key Name" rules={[{ required: true }]}><Input placeholder="e.g. production" /></Form.Item>
        </Form>
      </Drawer>

      <Modal title="API Key Created" open={!!tokenModal} onOk={() => setTokenModal(null)} onCancel={() => setTokenModal(null)} cancelButtonProps={{ style: { display: 'none' } }}>
        <Typography.Paragraph>Key <strong>{tokenModal?.name}</strong> created. Copy the token now.</Typography.Paragraph>
        <Input.TextArea value={tokenModal?.token} readOnly rows={2} />
      </Modal>
    </div>
  );
}
