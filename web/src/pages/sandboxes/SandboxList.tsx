import { useState, useMemo } from 'react';
import { Table, Button, Input, Space, Tag, Typography, Drawer, Descriptions, App, Select, Alert } from 'antd';
import { SearchOutlined } from '@ant-design/icons';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { listSandboxes, getSandbox, deleteSandbox, batchDeleteSandboxes } from '@/api/sandboxes';
import { listTenants } from '@/api/tenants';
import { listShares } from '@/api/shares';
import type { Sandbox } from '@/types';
import { formatTime } from '@/utils/time';
import { SANDBOX_STATE_COLORS, SANDBOX_STATE_LABELS } from '@/utils/constants';
import { useDebounce } from '@/hooks/useDebounce';
import { usePagination } from '@/hooks/usePagination';

export default function SandboxList() {
  const queryClient = useQueryClient();
  const { message, modal } = App.useApp();
  const [search, setSearch] = useState('');
  const [stateFilter, setStateFilter] = useState<string>();
  const [nsFilter, setNsFilter] = useState<string>();
  const { page, pageSize, setPage, setPageSize } = usePagination();
  const [detailId, setDetailId] = useState<string | null>(null);
  const [selectedKeys, setSelectedKeys] = useState<string[]>([]);
  const [allPagesSelected, setAllPagesSelected] = useState(false);

  const debouncedSearch = useDebounce(search);

  const queryParams = useMemo(() => {
    const p: Record<string, string | number> = { page, page_size: pageSize };
    if (debouncedSearch) p.search = debouncedSearch;
    if (stateFilter) p.state = stateFilter;
    if (nsFilter) p.namespace_id = nsFilter;
    return p;
  }, [debouncedSearch, stateFilter, nsFilter, page, pageSize]);

  const { data, isLoading } = useQuery({
    queryKey: ['sandboxes', queryParams],
    queryFn: () => listSandboxes(queryParams),
  });

  const { data: tenantsData } = useQuery({
    queryKey: ['tenants-select'],
    queryFn: () => listTenants({ page_size: 200 }),
  });

  const { data: sharesData } = useQuery({
    queryKey: ['shares-select'],
    queryFn: () => listShares({ page_size: 500 }),
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
      message.success('沙箱已删除');
    },
  });

  const handleDelete = (sandbox: Sandbox) => {
    modal.confirm({
      title: `删除沙箱「${sandbox.name || sandbox.id.slice(0, 8)}」？`,
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: () => deleteMutation.mutateAsync(sandbox.id),
    });
  };

  const handleBatchDelete = () => {
    if (selectedKeys.length === 0 && !allPagesSelected) return;

    const countLabel = allPagesSelected
      ? `所有匹配的 ${data?.total ?? 0} 个`
      : `${selectedKeys.length} 个`;

    modal.confirm({
      title: `批量删除 ${countLabel} 沙箱？`,
      okText: '删除',
      okButtonProps: { danger: true },
      cancelText: '取消',
      onOk: async () => {
        if (allPagesSelected) {
          // Use filter mode for cross-page all-select
          const filterParams: { state?: string; namespace_id?: string } = {};
          if (stateFilter) filterParams.state = stateFilter;
          if (nsFilter) filterParams.namespace_id = nsFilter;

          const result = await batchDeleteSandboxes({ filter: filterParams });
          setSelectedKeys([]);
          setAllPagesSelected(false);
          queryClient.invalidateQueries({ queryKey: ['sandboxes'] });
          if (result.failed > 0) {
            message.warning(`${result.deleted} 个已删除，${result.failed} 个失败`);
          } else {
            message.success(`${result.deleted} 个沙箱已删除`);
          }
        } else {
          // Use ids mode for current-page selection
          const result = await batchDeleteSandboxes({ ids: selectedKeys });
          setSelectedKeys([]);
          setAllPagesSelected(false);
          queryClient.invalidateQueries({ queryKey: ['sandboxes'] });
          if (result.failed > 0) {
            message.warning(`${result.deleted} 个已删除，${result.failed} 个失败`);
          } else {
            message.success(`${result.deleted} 个沙箱已删除`);
          }
        }
      },
    });
  };

  const handleCancelSelection = () => {
    setSelectedKeys([]);
    setAllPagesSelected(false);
  };

  const tenantMap = new Map((tenantsData?.tenants ?? []).map((t) => [t.id, t.name]));
  const shareMap = new Map((sharesData?.shares ?? []).map((s) => [s.id, s.name]));
  const tenantOptions = (tenantsData?.tenants ?? []).map((t) => ({ label: t.name, value: t.id }));

  const stateOptions = ['running', 'starting', 'stopping', 'stopped', 'error'].map((s) => ({
    label: SANDBOX_STATE_LABELS[s] || s,
    value: s,
  }));

  const total = data?.total ?? 0;
  const currentPageCount = data?.sandboxes?.length ?? 0;
  const showAllPagesBanner = selectedKeys.length > 0 && total > currentPageCount;

  const columns = [
    {
      title: '名称', dataIndex: 'name', key: 'name',
      render: (name: string, r: Sandbox) => (
        <a onClick={() => setDetailId(r.id)}>{name || r.id.slice(0, 8)}</a>
      ),
    },
    { title: '模板', dataIndex: 'template', key: 'template', width: 120 },
    {
      title: '状态', dataIndex: 'state', key: 'state', width: 100,
      render: (s: string) => <Tag color={SANDBOX_STATE_COLORS[s] || 'default'}>{SANDBOX_STATE_LABELS[s] || s}</Tag>,
    },
    {
      title: '命名空间', dataIndex: 'namespace_id', key: 'ns', width: 160,
      render: (nid: string) => tenantMap.get(nid) || (nid ? nid.slice(0, 8) : '-'),
    },
    {
      title: '创建时间', dataIndex: 'created_at', key: 'created', width: 180,
      render: (v: string) => formatTime(v),
    },
    {
      title: '操作', key: 'actions', width: 120,
      render: (_: unknown, record: Sandbox) => {
        const isTransient = record.state === 'starting' || record.state === 'stopping';
        const isRunning = record.state === 'running';
        return (
          <Space size="small">
            {isRunning && (
              <Button size="small" onClick={() => handleDelete(record)} disabled={isTransient}>
                停止
              </Button>
            )}
            {!isRunning && (
              <Button size="small" danger onClick={() => handleDelete(record)} disabled={isTransient}>
                删除
              </Button>
            )}
          </Space>
        );
      },
    },
  ];

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16 }}>
        <Typography.Title level={4} style={{ margin: 0 }}>沙箱管理</Typography.Title>
      </div>
      <Space style={{ marginBottom: 16 }} wrap>
        <Input
          placeholder="搜索名称或 ID"
          prefix={<SearchOutlined />}
          value={search}
          onChange={(e) => { setSearch(e.target.value); setPage(1); }}
          allowClear
          style={{ width: 240 }}
        />
        <Select
          placeholder="状态"
          allowClear
          value={stateFilter}
          onChange={(v) => { setStateFilter(v); setPage(1); }}
          style={{ width: 130 }}
          options={stateOptions}
        />
        <Select
          placeholder="命名空间"
          allowClear
          showSearch
          optionFilterProp="label"
          value={nsFilter}
          onChange={(v) => { setNsFilter(v); setPage(1); }}
          style={{ width: 200 }}
          options={tenantOptions}
        />
      </Space>

      {/* Cross-page selection banner */}
      {showAllPagesBanner && !allPagesSelected && (
        <Alert
          type="info"
          style={{ marginBottom: 12 }}
          message={
            <Space>
              <span>已选 {selectedKeys.length} 项</span>
              <Button
                type="link"
                size="small"
                style={{ padding: 0 }}
                onClick={() => setAllPagesSelected(true)}
              >
                选择所有匹配结果（共 {total} 项）
              </Button>
              <Button danger size="small" onClick={handleBatchDelete}>批量删除</Button>
              <Button size="small" onClick={handleCancelSelection}>取消选择</Button>
            </Space>
          }
        />
      )}

      {allPagesSelected && (
        <Alert
          type="warning"
          style={{ marginBottom: 12 }}
          message={
            <Space>
              <span>已选择所有匹配结果（共 {total} 项）</span>
              <Button danger size="small" onClick={handleBatchDelete}>批量删除</Button>
              <Button size="small" onClick={handleCancelSelection}>取消选择</Button>
            </Space>
          }
        />
      )}

      {selectedKeys.length > 0 && !showAllPagesBanner && (
        <div style={{ marginBottom: 12 }}>
          <Button danger onClick={handleBatchDelete}>
            删除 {selectedKeys.length} 个选中项
          </Button>
        </div>
      )}

      <Table
        dataSource={data?.sandboxes ?? []}
        columns={columns}
        rowKey="id"
        loading={isLoading}
        rowSelection={{
          selectedRowKeys: selectedKeys,
          onChange: (keys) => {
            setSelectedKeys(keys as string[]);
            setAllPagesSelected(false);
          },
          getCheckboxProps: (record: Sandbox) => ({
            disabled: record.state === 'starting' || record.state === 'stopping',
          }),
        }}
        pagination={{
          current: page, pageSize, total,
          onChange: (p, ps) => { setPage(p); setPageSize(ps); },
          showSizeChanger: true, showTotal: (t) => `共 ${t} 个沙箱`,
        }}
      />
      <Drawer
        title={`沙箱: ${detail?.name || detail?.id?.slice(0, 8) || ''}`}
        open={!!detailId}
        onClose={() => setDetailId(null)}
        width={520}
      >
        {detail && (
          <Descriptions column={1} bordered size="small">
            <Descriptions.Item label="ID">{detail.id}</Descriptions.Item>
            <Descriptions.Item label="名称">{detail.name || '-'}</Descriptions.Item>
            <Descriptions.Item label="模板">{detail.template}</Descriptions.Item>
            <Descriptions.Item label="状态">
              <Tag color={SANDBOX_STATE_COLORS[detail.state] || 'default'}>{SANDBOX_STATE_LABELS[detail.state] || detail.state}</Tag>
            </Descriptions.Item>
            <Descriptions.Item label="命名空间">{tenantMap.get(detail.namespace_id) || detail.namespace_id}</Descriptions.Item>
            <Descriptions.Item label="根路径">{detail.root_path}</Descriptions.Item>
            {detail.error_message && (
              <Descriptions.Item label="错误">
                <Typography.Text type="danger">{detail.error_message}</Typography.Text>
              </Descriptions.Item>
            )}
            <Descriptions.Item label="超时">{detail.timeout}s</Descriptions.Item>
            <Descriptions.Item label="创建时间">{formatTime(detail.created_at)}</Descriptions.Item>
            <Descriptions.Item label="更新时间">{formatTime(detail.updated_at)}</Descriptions.Item>
            {detail.env && Object.keys(detail.env).length > 0 && (
              <Descriptions.Item label="环境变量">
                {Object.entries(detail.env).map(([k, v]) => (
                  <div key={k}>
                    <Typography.Text code>{k}</Typography.Text> ={' '}
                    {typeof v === 'string' && (
                      k.toLowerCase().includes('secret') ||
                      k.toLowerCase().includes('password') ||
                      k.toLowerCase().includes('token')
                    ) ? '••••••••' : String(v)}
                  </div>
                ))}
              </Descriptions.Item>
            )}
            {detail.metadata && Object.keys(detail.metadata).length > 0 && (
              <Descriptions.Item label="元数据">
                <Typography.Text copyable style={{ maxWidth: '100%', wordBreak: 'break-all' }}>
                  {JSON.stringify(detail.metadata, null, 2)}
                </Typography.Text>
              </Descriptions.Item>
            )}
            {detail.mounts && detail.mounts.length > 0 && (
              <Descriptions.Item label="挂载">
                {detail.mounts.map((m) => (
                  <div key={m.share_id}>
                    {m.mount_path} &larr; {shareMap.get(m.share_id) || m.share_id.slice(0, 8)}
                  </div>
                ))}
              </Descriptions.Item>
            )}
          </Descriptions>
        )}
      </Drawer>
    </div>
  );
}
