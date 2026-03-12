import { useState, useMemo } from 'react';
import { Table, Select, Space, Typography, DatePicker, Tag } from 'antd';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { listAuditLogs } from '@/api/audit';
import { listTenants } from '@/api/tenants';
import type { AuditLog, AuditFilter } from '@/types';
import { formatTime } from '@/utils/time';
import { AUDIT_ACTION_GROUPS, RESOURCE_TYPES } from '@/utils/constants';
import dayjs from 'dayjs';

const { RangePicker } = DatePicker;

const actionOptions = Object.entries(AUDIT_ACTION_GROUPS).map(([group, actions]) => ({
  label: group,
  options: actions.map((a) => ({ label: a, value: a })),
}));

export default function AuditLogList() {
  const navigate = useNavigate();
  const [actionFilter, setActionFilter] = useState<string>();
  const [actorTypeFilter, setActorTypeFilter] = useState<string>();
  const [resourceTypeFilter, setResourceTypeFilter] = useState<string>();
  const [timeRange, setTimeRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(20);
  const [expandedKey, setExpandedKey] = useState<string>();

  const { data: tenantsData } = useQuery({
    queryKey: ['tenants-select'],
    queryFn: () => listTenants({ page_size: 200 }),
  });

  const filter = useMemo<AuditFilter>(() => {
    const f: AuditFilter = { page, page_size: pageSize };
    if (actionFilter) f.action = actionFilter;
    if (actorTypeFilter) f.actor_type = actorTypeFilter;
    if (resourceTypeFilter) f.resource_type = resourceTypeFilter;
    if (timeRange) {
      f.from = timeRange[0].toISOString();
      f.to = timeRange[1].toISOString();
    }
    return f;
  }, [actionFilter, actorTypeFilter, resourceTypeFilter, timeRange, page, pageSize]);

  const { data, isLoading } = useQuery({
    queryKey: ['audit-logs', filter],
    queryFn: () => listAuditLogs(filter),
  });

  const tenantMap = new Map((tenantsData?.tenants ?? []).map((t) => [t.id, t.name]));

  const columns = [
    { title: 'Time', dataIndex: 'created_at', key: 'time', width: 180,
      render: (v: string) => formatTime(v) },
    { title: 'Action', dataIndex: 'action', key: 'action', width: 180,
      render: (v: string) => <Tag>{v}</Tag> },
    { title: 'Actor', key: 'actor', width: 160,
      render: (_: unknown, r: AuditLog) => {
        if (r.actor_type === 'admin') return <Tag color="red">Admin</Tag>;
        const name = r.actor_id ? tenantMap.get(r.actor_id) : null;
        return name || (r.actor_id ? r.actor_id.slice(0, 8) : '-');
      },
    },
    { title: 'Resource', dataIndex: 'resource_type', key: 'res_type', width: 100 },
    { title: 'Resource Name', key: 'res_name',
      render: (_: unknown, r: AuditLog) => {
        const name = r.resource_name || r.resource_id.slice(0, 8);
        if (r.resource_type === 'tenant') {
          return <a onClick={() => navigate(`/admin/tenants/${r.resource_id}`)}>{name}</a>;
        }
        if (r.resource_type === 'share') {
          return <a onClick={() => navigate(`/admin/shares/${r.resource_id}`)}>{name}</a>;
        }
        return name;
      },
    },
    { title: 'IP', dataIndex: 'ip_address', key: 'ip', width: 130 },
  ];

  return (
    <div>
      <Typography.Title level={4} style={{ marginBottom: 16 }}>Audit Logs</Typography.Title>
      <Space style={{ marginBottom: 16 }} wrap>
        <Select
          placeholder="Action"
          allowClear
          value={actionFilter}
          onChange={(v) => { setActionFilter(v); setPage(1); }}
          style={{ width: 200 }}
          options={actionOptions}
        />
        <Select
          placeholder="Actor Type"
          allowClear
          value={actorTypeFilter}
          onChange={(v) => { setActorTypeFilter(v); setPage(1); }}
          style={{ width: 130 }}
          options={[
            { label: 'Admin', value: 'admin' },
            { label: 'Tenant', value: 'tenant' },
          ]}
        />
        <Select
          placeholder="Resource Type"
          allowClear
          value={resourceTypeFilter}
          onChange={(v) => { setResourceTypeFilter(v); setPage(1); }}
          style={{ width: 140 }}
          options={RESOURCE_TYPES.map((r) => ({ label: r, value: r }))}
        />
        <RangePicker
          showTime
          onChange={(dates) => setTimeRange(dates as [dayjs.Dayjs, dayjs.Dayjs] | null)}
        />
      </Space>
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
          showSizeChanger: true, showTotal: (t) => `${t} logs`,
        }}
      />
    </div>
  );
}
