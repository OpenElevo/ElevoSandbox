import { useState } from 'react';
import { useParams, useNavigate, useSearchParams } from 'react-router-dom';
import { Card, Descriptions, Tag, Button, Space, Tabs, App, Drawer, Form, Input, Select, Table } from 'antd';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { getShare, updateShare, deleteShare, listSharePermissions, grantPermission, updatePermission, revokePermission } from '@/api/shares';
import { listTenants } from '@/api/tenants';
import type { SharePermission, PermissionLevel } from '@/types';
import { formatTime } from '@/utils/time';
import { PERMISSION_LEVELS } from '@/utils/constants';
import FileBrowser from '@/components/FileBrowser/FileBrowser';

export default function ShareDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = searchParams.get('tab') || 'permissions';

  const [editOpen, setEditOpen] = useState(false);
  const [editForm] = Form.useForm();
  const [grantOpen, setGrantOpen] = useState(false);
  const [grantForm] = Form.useForm();

  const { data: share, isLoading } = useQuery({
    queryKey: ['share', id],
    queryFn: () => getShare(id!),
    enabled: !!id,
  });

  const { data: permissions } = useQuery({
    queryKey: ['share-permissions', id],
    queryFn: () => listSharePermissions(id!),
    enabled: !!id && activeTab === 'permissions',
  });

  const { data: tenantsData } = useQuery({
    queryKey: ['tenants-select'],
    queryFn: () => listTenants({ page_size: 200 }),
  });

  const updateMutation = useMutation({
    mutationFn: (params: { name?: string; description?: string; visibility?: 'public' | 'private' }) => updateShare(id!, params),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['share', id] });
      setEditOpen(false);
      message.success('Share updated');
    },
  });

  const grantMutation = useMutation({
    mutationFn: (params: { tenant_id: string; permission: PermissionLevel }) =>
      grantPermission(id!, params.tenant_id, params.permission),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['share-permissions', id] });
      setGrantOpen(false);
      grantForm.resetFields();
      message.success('Permission granted');
    },
    onError: (err: { response?: { data?: { error?: { message?: string } } } }) => {
      message.error(err.response?.data?.error?.message || 'Failed to grant permission');
    },
  });

  const handleEdit = () => {
    if (!share) return;
    editForm.setFieldsValue({ name: share.name, description: share.description, visibility: share.visibility });
    setEditOpen(true);
  };

  const handleDelete = () => {
    if (!share) return;
    modal.confirm({
      title: `Delete share "${share.name}"?`,
      okButtonProps: { danger: true },
      onOk: async () => {
        await deleteShare(id!);
        message.success('Share deleted');
        navigate('/admin/shares');
      },
    });
  };

  const handleUpdatePermission = (tenantId: string, level: PermissionLevel) => {
    updatePermission(id!, tenantId, level).then(() => {
      queryClient.invalidateQueries({ queryKey: ['share-permissions', id] });
      message.success('Permission updated');
    });
  };

  const handleRevokePermission = (tenantId: string) => {
    modal.confirm({
      title: 'Revoke permission?',
      onOk: async () => {
        await revokePermission(id!, tenantId);
        queryClient.invalidateQueries({ queryKey: ['share-permissions', id] });
        message.success('Permission revoked');
      },
    });
  };

  if (isLoading || !share) return <Card loading />;

  const tenantMap = new Map((tenantsData?.tenants ?? []).map((t) => [t.id, t.name]));

  const permColumns = [
    { title: 'Tenant', dataIndex: 'tenant_id', key: 'tenant',
      render: (tid: string) => tenantMap.get(tid) || tid.slice(0, 8) },
    { title: 'Permission', dataIndex: 'permission', key: 'perm',
      render: (perm: PermissionLevel, record: SharePermission) => (
        <Select
          size="small"
          value={perm}
          onChange={(v) => handleUpdatePermission(record.tenant_id, v)}
          style={{ width: 120 }}
          options={PERMISSION_LEVELS.map((l) => ({ label: l, value: l }))}
        />
      ),
    },
    { title: 'Granted', dataIndex: 'created_at', key: 'created', render: (v: string) => formatTime(v) },
    { title: 'Actions', key: 'actions',
      render: (_: unknown, record: SharePermission) => (
        <Button size="small" danger onClick={() => handleRevokePermission(record.tenant_id)}>Revoke</Button>
      ),
    },
  ];

  const tenantOptions = (tenantsData?.tenants ?? [])
    .filter((t) => t.id !== share.owner_tenant_id)
    .map((t) => ({ label: t.name, value: t.id }));

  return (
    <div>
      <Button type="link" onClick={() => navigate('/admin/shares')} style={{ padding: 0, marginBottom: 8 }}>
        &larr; Back to Shares
      </Button>
      <Card
        title={share.name}
        extra={
          <Space>
            <Button onClick={handleEdit}>Edit</Button>
            <Button danger onClick={handleDelete}>Delete</Button>
          </Space>
        }
      >
        <Descriptions column={2}>
          <Descriptions.Item label="ID">{share.id}</Descriptions.Item>
          <Descriptions.Item label="Visibility">
            <Tag color={share.visibility === 'public' ? 'blue' : 'default'}>{share.visibility}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="Owner">
            <a onClick={() => navigate(`/admin/tenants/${share.owner_tenant_id}`)}>
              {tenantMap.get(share.owner_tenant_id) || share.owner_tenant_id}
            </a>
          </Descriptions.Item>
          <Descriptions.Item label="Source Path">{share.source_path}</Descriptions.Item>
          <Descriptions.Item label="Description" span={2}>{share.description || '-'}</Descriptions.Item>
          <Descriptions.Item label="Created">{formatTime(share.created_at)}</Descriptions.Item>
          <Descriptions.Item label="Updated">{formatTime(share.updated_at)}</Descriptions.Item>
        </Descriptions>
      </Card>

      <Tabs
        activeKey={activeTab}
        onChange={(key) => setSearchParams({ tab: key })}
        style={{ marginTop: 16 }}
        items={[
          { key: 'permissions', label: 'Permissions', children: (
            <div>
              <Button type="primary" size="small" onClick={() => setGrantOpen(true)} style={{ marginBottom: 12 }}>
                Grant Permission
              </Button>
              <Table dataSource={permissions ?? []} columns={permColumns} rowKey="tenant_id" size="small" pagination={false} />
            </div>
          )},
          { key: 'files', label: 'Files', children: (
            <FileBrowser shareId={id!} />
          )},
        ]}
      />

      <Drawer title="Edit Share" open={editOpen} onClose={() => setEditOpen(false)} width={400}
        extra={<Button type="primary" onClick={() => editForm.validateFields().then((v) => updateMutation.mutate(v))} loading={updateMutation.isPending}>Save</Button>}>
        <Form form={editForm} layout="vertical">
          <Form.Item name="name" label="Name" rules={[{ required: true }]}><Input /></Form.Item>
          <Form.Item name="description" label="Description"><Input.TextArea rows={3} /></Form.Item>
          <Form.Item name="visibility" label="Visibility">
            <Select options={[{ label: 'Private', value: 'private' }, { label: 'Public', value: 'public' }]} />
          </Form.Item>
        </Form>
      </Drawer>

      <Drawer title="Grant Permission" open={grantOpen} onClose={() => setGrantOpen(false)} width={400}
        extra={<Button type="primary" onClick={() => grantForm.validateFields().then((v) => grantMutation.mutate(v))} loading={grantMutation.isPending}>Grant</Button>}>
        <Form form={grantForm} layout="vertical">
          <Form.Item name="tenant_id" label="Tenant" rules={[{ required: true }]}>
            <Select showSearch optionFilterProp="label" options={tenantOptions} placeholder="Select tenant" />
          </Form.Item>
          <Form.Item name="permission" label="Permission Level" rules={[{ required: true }]}>
            <Select options={PERMISSION_LEVELS.map((l) => ({ label: l, value: l }))} />
          </Form.Item>
        </Form>
      </Drawer>
    </div>
  );
}
