import { useState, useMemo } from 'react';
import { Table, Select, Space, Typography, DatePicker, Tag } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listAuditLogs } from '@/api/audit';
import { listTenants } from '@/api/tenants';
import type { AuditLog, AuditFilter } from '@/types';
import { formatTime } from '@/utils/time';
import { AUDIT_ACTION_GROUPS, AUDIT_ACTION_LABELS, RESOURCE_TYPES, RESOURCE_TYPE_LABELS } from '@/utils/constants';
import dayjs from 'dayjs';

const { RangePicker } = DatePicker;

const actionOptions = Object.entries(AUDIT_ACTION_GROUPS).map(([group, actions]) => ({
  label: group,
  options: actions.map((a) => ({ label: AUDIT_ACTION_LABELS[a] || a, value: a })),
}));

export default function AuditLogList() {
  const navigate = useNavigate();
  const [actionFilter, setActionFilter] = useState<string[]>([]);
  const [actorTypeFilter, setActorTypeFilter] = useState<string>();
  const [actorTenantFilter, setActorTenantFilter] = useState<string>();
  const [resourceTypeFilter, setResourceTypeFilter] = useState<string>();
  const [timeRange, setTimeRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [expandedKey, setExpandedKey] = useState<string>();

  const { data: tenantsData } = useQuery({
    queryKey: ['tenants-select'],
    queryFn: () => listTenants({ page_size: 200 }),
  });

  const tenantOptions = (tenantsData?.tenants ?? []).map((t) => ({ label: t.name, value: t.id }));

  const filter = useMemo<AuditFilter>(() => {
    const f: AuditFilter = { page, page_size: pageSize };
    if (actionFilter.length > 0) f.action = actionFilter;
    if (actorTypeFilter) f.actor_type = actorTypeFilter;
    if (actorTypeFilter === 'tenant' && actorTenantFilter) f.actor_id = actorTenantFilter;
    if (resourceTypeFilter) f.resource_type = resourceTypeFilter;
    if (timeRange) {
      f.from = timeRange[0].toISOString();
      f.to = timeRange[1].toISOString();
    }
    return f;
  }, [actionFilter, actorTypeFilter, actorTenantFilter, resourceTypeFilter, timeRange, page, pageSize]);

  const { data, isLoading } = useQuery({
    queryKey: ['audit-logs', filter],
    queryFn: () => listAuditLogs(filter),
  });

  const tenantMap = new Map((tenantsData?.tenants ?? []).map((t) => [t.id, t.name]));

  const columns = [
    { title: '时间', dataIndex: 'created_at', key: 'time', width: 180,
      render: (v: string) => formatTime(v) },
    { title: '操作', dataIndex: 'action', key: 'action', width: 180,
      render: (v: string) => <Tag>{AUDIT_ACTION_LABELS[v] || v}</Tag> },
    { title: '操作者', key: 'actor', width: 160,
      render: (_: unknown, r: AuditLog) => {
        if (r.actor_type === 'admin') return <Tag color="red">管理员</Tag>;
        const name = r.actor_id ? tenantMap.get(r.actor_id) : null;
        return name || (r.actor_id ? r.actor_id.slice(0, 8) : '-');
      },
    },
    { title: '资源类型', dataIndex: 'resource_type', key: 'res_type', width: 100,
      render: (v: string) => RESOURCE_TYPE_LABELS[v] || v },
    { title: '资源名称', key: 'res_name',
      render: (_: unknown, r: AuditLog) => {
        const name = r.resource_name || r.resource_id.slice(0, 8);
        // Don't link to resources that may have been deleted/revoked
        const isDestructive = r.action.includes('delete') || r.action.includes('revoke');
        if (!isDestructive) {
          if (r.resource_type === 'tenant') {
            return <a onClick={() => navigate(`/admin/tenants/${r.resource_id}`)}>{name}</a>;
          }
          if (r.resource_type === 'share') {
            return <a onClick={() => navigate(`/admin/shares/${r.resource_id}`)}>{name}</a>;
          }
        }
        return name;
      },
    },
    { title: 'IP', dataIndex: 'ip_address', key: 'ip', width: 130 },
  ];

  return (
    <div>
      <Typography.Title level={4} style={{ marginBottom: 16 }}>审计日志</Typography.Title>
      <Space style={{ marginBottom: 16 }} wrap>
        <Select
          mode="multiple"
          placeholder="操作类型"
          allowClear
          value={actionFilter}
          onChange={(v) => { setActionFilter(v); setPage(1); }}
          style={{ minWidth: 200 }}
          maxTagCount="responsive"
          options={actionOptions}
        />
        <Select
          placeholder="操作者类型"
          allowClear
          value={actorTypeFilter}
          onChange={(v) => { setActorTypeFilter(v); setActorTenantFilter(undefined); setPage(1); }}
          style={{ width: 130 }}
          options={[
            { label: '管理员', value: 'admin' },
            { label: '租户', value: 'tenant' },
          ]}
        />
        <Select
          placeholder="执行者（租户）"
          allowClear
          showSearch
          optionFilterProp="label"
          disabled={actorTypeFilter !== 'tenant'}
          value={actorTenantFilter}
          onChange={(v) => { setActorTenantFilter(v); setPage(1); }}
          style={{ width: 180 }}
          options={tenantOptions}
        />
        <Select
          placeholder="资源类型"
          allowClear
          value={resourceTypeFilter}
          onChange={(v) => { setResourceTypeFilter(v); setPage(1); }}
          style={{ width: 140 }}
          options={RESOURCE_TYPES.map((r) => ({ label: RESOURCE_TYPE_LABELS[r] || r, value: r }))}
        />
        <RangePicker
          showTime
          placeholder={['开始时间', '结束时间']}
          onChange={(dates) => setTimeRange(dates as [dayjs.Dayjs, dayjs.Dayjs] | null)}
        />
      </Space>
      {data && data.total > 1000 && (
        <Typography.Text type="warning" style={{ display: 'block', marginBottom: 8 }}>
          结果较多（{data.total} 条），请使用筛选条件缩小范围
        </Typography.Text>
      )}
      <Table
        dataSource={data?.logs ?? []}
        columns={columns}
        rowKey="id"
        loading={isLoading}
        expandable={{
          expandedRowKeys: expandedKey ? [expandedKey] : [],
          onExpand: (expanded, record) => setExpandedKey(expanded ? record.id : undefined),
          expandedRowRender: (record: AuditLog) => (
            <pre style={{ margin: 0, fontSize: 12, maxHeight: 200, overflow: 'auto' }}>
              {JSON.stringify(record.detail, null, 2)}
            </pre>
          ),
        }}
        pagination={{
          current: page, pageSize, total: data?.total ?? 0,
          onChange: (p, ps) => { setPage(p); setPageSize(ps); },
          showSizeChanger: true, showTotal: (t) => `共 ${t} 条日志`,
        }}
      />
    </div>
  );
}
