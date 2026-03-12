import { useState, useEffect } from 'react';
import { useParams, useNavigate, useSearchParams } from 'react-router-dom';
import { Card, Descriptions, Tag, Button, Space, Tabs, App, Drawer, Form, Input, Select, Table, Alert } from 'antd';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { getShare, updateShare, deleteShare, listSharePermissions, grantPermission, updatePermission, revokePermission } from '@/api/shares';
import { listTenants } from '@/api/tenants';
import type { SharePermission, PermissionLevel } from '@/types';
import { formatTime } from '@/utils/time';
import { PERMISSION_LEVELS, PERMISSION_LABELS } from '@/utils/constants';
import FileBrowser from '@/components/FileBrowser/FileBrowser';
import DirtyFormGuard from '@/components/DirtyFormGuard';
import { useBreadcrumbStore } from '@/stores/breadcrumbStore';

export default function ShareDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = searchParams.get('tab') || 'permissions';
  const setBreadcrumbName = useBreadcrumbStore((s) => s.setBreadcrumbName);

  const [editOpen, setEditOpen] = useState(false);
  const [editDirty, setEditDirty] = useState(false);
  const [editForm] = Form.useForm();
  const [grantOpen, setGrantOpen] = useState(false);
  const [grantForm] = Form.useForm();

  const { data: share, isLoading } = useQuery({
    queryKey: ['share', id],
    queryFn: () => getShare(id!),
    enabled: !!id,
  });

  useEffect(() => {
    if (id && share?.name) {
      setBreadcrumbName(id, share.name);
    }
  }, [id, share?.name, setBreadcrumbName]);

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
      setEditDirty(false);
      message.success('共享已更新');
    },
  });

  const grantMutation = useMutation({
    mutationFn: (params: { tenant_id: string; permission: PermissionLevel }) =>
      grantPermission(id!, params.tenant_id, params.permission),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['share-permissions', id] });
      setGrantOpen(false);
      grantForm.resetFields();
      message.success('权限已授予');
    },
    onError: (err: { response?: { data?: { error?: { message?: string } } } }) => {
      message.error(err.response?.data?.error?.message || '授予权限失败');
    },
  });

  const handleEdit = () => {
    if (!share) return;
    editForm.setFieldsValue({ name: share.name, description: share.description, visibility: share.visibility });
    setEditDirty(false);
    setEditOpen(true);
  };

  const handleDelete = () => {
    if (!share) return;
    let inputName = '';
    modal.confirm({
      title: `删除共享「${share.name}」？`,
      content: (
        <div>
          <Input placeholder="请输入共享名称确认" style={{ marginTop: 8 }} onChange={(e) => { inputName = e.target.value; }} />
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
        await deleteShare(id!);
        message.success('共享已删除');
        navigate('/admin/shares');
      },
    });
  };

  const handleUpdatePermission = (tenantId: string, currentLevel: PermissionLevel, newLevel: PermissionLevel) => {
    const levelOrder = PERMISSION_LEVELS;
    const isDowngrade = levelOrder.indexOf(newLevel) < levelOrder.indexOf(currentLevel);

    const doUpdate = () => {
      updatePermission(id!, tenantId, newLevel).then(() => {
        queryClient.invalidateQueries({ queryKey: ['share-permissions', id] });
        message.success('权限已更新');
      });
    };

    if (isDowngrade) {
      modal.confirm({
        title: '确认降级权限？',
        content: `将从「${PERMISSION_LABELS[currentLevel]}」降级为「${PERMISSION_LABELS[newLevel]}」，可能导致正在使用的挂载变为只读或不可用。`,
        okText: '确认降级',
        cancelText: '取消',
        onOk: doUpdate,
      });
    } else {
      doUpdate();
    }
  };

  const handleRevokePermission = (tenantId: string) => {
    modal.confirm({
      title: '撤销权限？',
      content: '撤销后该租户将无法访问此共享。',
      okText: '撤销',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        await revokePermission(id!, tenantId);
        queryClient.invalidateQueries({ queryKey: ['share-permissions', id] });
        message.success('权限已撤销');
      },
    });
  };

  if (isLoading || !share) return <Card loading />;

  const tenantMap = new Map((tenantsData?.tenants ?? []).map((t) => [t.id, t.name]));

  const permColumns = [
    { title: '租户', dataIndex: 'tenant_id', key: 'tenant',
      render: (tid: string) => tenantMap.get(tid) || tid.slice(0, 8) },
    { title: '权限', dataIndex: 'permission', key: 'perm',
      render: (perm: PermissionLevel, record: SharePermission) => (
        <Select
          size="small"
          value={perm}
          onChange={(v) => handleUpdatePermission(record.tenant_id, perm, v)}
          style={{ width: 120 }}
          options={PERMISSION_LEVELS.map((l) => ({ label: PERMISSION_LABELS[l], value: l }))}
        />
      ),
    },
    { title: '授予时间', dataIndex: 'created_at', key: 'created', render: (v: string) => formatTime(v) },
    { title: '操作', key: 'actions',
      render: (_: unknown, record: SharePermission) => (
        <Button size="small" danger onClick={() => handleRevokePermission(record.tenant_id)}>撤销</Button>
      ),
    },
  ];

  // F13: Exclude owner tenant AND tenants that already have a permission entry
  const permissionedTenantIds = new Set((permissions ?? []).map((p) => p.tenant_id));
  const tenantOptions = (tenantsData?.tenants ?? [])
    .filter((t) => t.id !== share.owner_tenant_id && !permissionedTenantIds.has(t.id))
    .map((t) => ({ label: t.name, value: t.id }));

  return (
    <div>
      <Button type="link" onClick={() => navigate('/admin/shares')} style={{ padding: 0, marginBottom: 8 }}>
        &larr; 返回共享列表
      </Button>
      <Card
        title={share.name}
        extra={
          <Space>
            <Button onClick={handleEdit}>编辑</Button>
            <Button danger onClick={handleDelete}>删除</Button>
          </Space>
        }
      >
        <Descriptions column={2}>
          <Descriptions.Item label="ID">{share.id}</Descriptions.Item>
          <Descriptions.Item label="可见性">
            <Tag color={share.visibility === 'public' ? 'blue' : 'default'}>{share.visibility === 'public' ? '公开' : '私有'}</Tag>
          </Descriptions.Item>
          <Descriptions.Item label="所属租户">
            <a onClick={() => navigate(`/admin/tenants/${share.owner_tenant_id}`)}>
              {tenantMap.get(share.owner_tenant_id) || share.owner_tenant_id}
            </a>
          </Descriptions.Item>
          <Descriptions.Item label="源路径">{share.source_path}</Descriptions.Item>
          <Descriptions.Item label="描述" span={2}>{share.description || '-'}</Descriptions.Item>
          <Descriptions.Item label="创建时间">{formatTime(share.created_at)}</Descriptions.Item>
          <Descriptions.Item label="更新时间">{formatTime(share.updated_at)}</Descriptions.Item>
        </Descriptions>
      </Card>

      <Tabs
        activeKey={activeTab}
        onChange={(key) => setSearchParams({ tab: key })}
        style={{ marginTop: 16 }}
        items={[
          { key: 'permissions', label: '权限', children: (
            <div>
              {/* F22: Public share banner */}
              {share.visibility === 'public' && (
                <Alert
                  type="info"
                  message="公开 Share：所有活跃租户拥有隐式读取权限"
                  style={{ marginBottom: 8 }}
                  showIcon
                />
              )}
              {/* F22: Owner tenant always has full admin permission */}
              <Alert
                type="info"
                message={`所有者 ${share.owner_tenant_name ?? tenantMap.get(share.owner_tenant_id) ?? share.owner_tenant_id} 自动拥有完全管理权限`}
                style={{ marginBottom: 12 }}
                showIcon
              />
              <Button type="primary" size="small" onClick={() => setGrantOpen(true)} style={{ marginBottom: 12 }}>
                授予权限
              </Button>
              <Table dataSource={permissions ?? []} columns={permColumns} rowKey="tenant_id" size="small" pagination={false} />
            </div>
          )},
          { key: 'files', label: '文件', children: (
            <FileBrowser shareId={id!} />
          )},
        ]}
      />

      <Drawer title="编辑共享" open={editOpen} onClose={() => setEditOpen(false)} width={400}
        extra={<Button type="primary" onClick={() => editForm.validateFields().then((v) => updateMutation.mutate(v))} loading={updateMutation.isPending}>保存</Button>}>
        <DirtyFormGuard dirty={editDirty && editOpen} />
        <Form form={editForm} layout="vertical" onValuesChange={() => setEditDirty(true)}>
          <Form.Item name="name" label="名称" rules={[{ required: true }]}><Input /></Form.Item>
          <Form.Item name="description" label="描述"><Input.TextArea rows={3} /></Form.Item>
          <Form.Item name="visibility" label="可见性">
            <Select options={[{ label: '私有', value: 'private' }, { label: '公开', value: 'public' }]} />
          </Form.Item>
        </Form>
      </Drawer>

      <Drawer title="授予权限" open={grantOpen} onClose={() => setGrantOpen(false)} width={400}
        extra={<Button type="primary" onClick={() => grantForm.validateFields().then((v) => grantMutation.mutate(v))} loading={grantMutation.isPending}>授予</Button>}>
        <Form form={grantForm} layout="vertical">
          <Form.Item name="tenant_id" label="租户" rules={[{ required: true, message: '请选择租户' }]}>
            <Select showSearch optionFilterProp="label" options={tenantOptions} placeholder="选择租户" />
          </Form.Item>
          <Form.Item name="permission" label="权限级别" rules={[{ required: true, message: '请选择权限级别' }]}>
            <Select options={PERMISSION_LEVELS.map((l) => ({ label: PERMISSION_LABELS[l], value: l }))} />
          </Form.Item>
        </Form>
      </Drawer>
    </div>
  );
}
