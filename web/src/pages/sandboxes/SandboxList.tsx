import { useState, useMemo } from 'react';
import { Table, Button, Input, Space, Tag, Typography, Drawer, Descriptions, App, Select } from 'antd';
import { SearchOutlined } from '@ant-design/icons';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { listSandboxes, getSandbox, deleteSandbox } from '@/api/sandboxes';
import { listTenants } from '@/api/tenants';
import type { Sandbox } from '@/types';
import { formatTime } from '@/utils/time';
import { SANDBOX_STATE_COLORS } from '@/utils/constants';

export default function SandboxList() {
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [search, setSearch] = useState('');
  const [stateFilter, setStateFilter] = useState<string>();
  const [nsFilter, setNsFilter] = useState<string>();
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [detailId, setDetailId] = useState<string | null>(null);
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);

  const queryParams = useMemo(() => {
    const p: Record<string, string | number> = { page, page_size: pageSize };
    if (search) p.search = search;
    if (stateFilter) p.state = stateFilter;
    if (nsFilter) p.namespace_id = nsFilter;
    return p;
  }, [search, stateFilter, nsFilter, page, pageSize]);

  const { data, isLoading } = useQuery({
    queryKey: ['sandboxes', queryParams],
    queryFn: () => listSandboxes(queryParams),
  });

  const { data: tenantsData } = useQuery({
    queryKey: ['tenants-select'],
    queryFn: () => listTenants({ page_size: 200 }),
  });

  const { data: detail } = useQuery({
    queryKey: ['sandbox', detailId],
    queryFn: () => getSandbox(detailId!),
    enabled: !!detailId,
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteSandbox(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sandboxes'] });
      message.success('Sandbox deleted');
    },
  });

  const handleDelete = (sandbox: Sandbox) => {
    modal.confirm({
      title: `Delete sandbox "${sandbox.name || sandbox.id}"?`,
      okButtonProps: { danger: true },
      onOk: () => deleteMutation.mutateAsync(sandbox.id),
    });
  };

  const handleBatchDelete = () => {
    if (selectedKeys.length === 0) return;
    modal.confirm({
      title: `Delete ${selectedKeys.length} sandbox(es)?`,
      okButtonProps: { danger: true },
      onOk: async () => {
        for (const id of selectedKeys) {
          await deleteSandbox(id);
        }
        setSelectedKeys([]);
        queryClient.invalidateQueries({ queryKey: ['sandboxes'] });
        message.success(`${selectedKeys.length} sandbox(es) deleted`);
      },
    });
  };

  const tenantMap = new Map((tenantsData?.tenants ?? []).map((t) => [t.id, t.name]));
  const tenantOptions = (tenantsData?.tenants ?? []).map((t) => ({ label: t.name, value: t.id }));

  const columns = [
    { title: 'Name', dataIndex: 'name', key: 'name',
      render: (name: string, r: Sandbox) => (
        <a onClick={() => setDetailId(r.id)}>{name || r.id.slice(0, 8)}</a>
      ),
    },
    { title: 'Template', dataIndex: 'template', key: 'template', width: 120 },
    { title: 'State', dataIndex: 'state', key: 'state', width: 100,
      render: (s: string) => <Tag color={SANDBOX_STATE_COLORS[s] || 'default'}>{s}</Tag>,
    },
    { title: 'Namespace', dataIndex: 'namespace_id', key: 'ns', width: 160,
      render: (nid: string) => tenantMap.get(nid) || (nid ? nid.slice(0, 8) : '-'),
    },
    { title: 'Created', dataIndex: 'created_at', key: 'created', width: 180,
      render: (v: string) => formatTime(v),
    },
    { title: 'Actions', key: 'actions', width: 100,
      render: (_: unknown, record: Sandbox) => (
        <Button size="small" danger onClick={() => handleDelete(record)}
          disabled={record.state === 'starting' || record.state === 'stopping'}>
          Delete
        </Button>
      ),
    },
  ];

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16 }}>
        <Typography.Title level={4} style={{ margin: 0 }}>Sandboxes</Typography.Title>
        {selectedKeys.length > 0 && (
          <Button danger onClick={handleBatchDelete}>
            Delete {selectedKeys.length} selected
          </Button>
        )}
      </div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Input
          placeholder="Search by name or ID"
          prefix={<SearchOutlined />}
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(1); }}
          allowClear
          style={{ width: 240 }}
        />
        <Select
          placeholder="State"
          allowClear
          value={stateFilter}
          onChange={(v) => { setStateFilter(v); setPage(1); }}
          style={{ width: 130 }}
          options={['running', 'starting', 'stopping', 'stopped', 'error'].map((s) => ({ label: s, value: s }))}
        />
        <Select
          placeholder="Namespace"
          allowClear
          showSearch
          optionFilterProp="label"
          value={nsFilter}
          onChange={(v) => { setNsFilter(v); setPage(1); }}
          style={{ width: 200 }}
          options={tenantOptions}
        />
      </Space>
      <Table
        dataSource={data?.sandboxes ?? []}
        columns={columns}
        rowKey="id"
        loading={isLoading}
        rowSelection={{
          selectedRowKeys: selectedKeys,
          onChange: (keys) => setSelectedKeys(keys as string[]),
        }}
        pagination={{
          current: page, pageSize, total: data?.total ?? 0,
          onChange: (p, ps) => { setPage(p); setPageSize(ps); },
          showSizeChanger: true, showTotal: (t) => `${t} sandboxes`,
        }}
      />
      <Drawer
        title={`Sandbox: ${detail?.name || detail?.id?.slice(0, 8) || ''}`}
        open={!!detailId}
        onClose={() => setDetailId(null)}
        width={520}
      >
        {detail && (
          <Descriptions column={1} bordered size="small">
            <Descriptions.Item label="ID">{detail.id}</Descriptions.Item>
            <Descriptions.Item label="Name">{detail.name || '-'}</Descriptions.Item>
            <Descriptions.Item label="Template">{detail.template}</Descriptions.Item>
            <Descriptions.Item label="State">
              <Tag color={SANDBOX_STATE_COLORS[detail.state] || 'default'}>{detail.state}</Tag>
            </Descriptions.Item>
            <Descriptions.Item label="Namespace">{tenantMap.get(detail.namespace_id) || detail.namespace_id}</Descriptions.Item>
            <Descriptions.Item label="Root Path">{detail.root_path}</Descriptions.Item>
            {detail.error_message && (
              <Descriptions.Item label="Error">
                <Typography.Text type="danger">{detail.error_message}</Typography.Text>
              </Descriptions.Item>
            )}
            <Descriptions.Item label="Timeout">{detail.timeout}s</Descriptions.Item>
            <Descriptions.Item label="Created">{formatTime(detail.created_at)}</Descriptions.Item>
            <Descriptions.Item label="Updated">{formatTime(detail.updated_at)}</Descriptions.Item>
            {detail.mounts && detail.mounts.length > 0 && (
              <Descriptions.Item label="Mounts">
                {detail.mounts.map((m) => (
                  <div key={m.share_id}>{m.mount_path} &larr; {m.share_id.slice(0, 8)}</div>
                ))}
              </Descriptions.Item>
            )}
          </Descriptions>
        )}
      </Drawer>
    </div>
  );
}
