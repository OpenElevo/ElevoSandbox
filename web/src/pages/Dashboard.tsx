import { Card, Col, Row, Statistic, Table, Typography, Skeleton } from 'antd';
import { TeamOutlined, ShareAltOutlined, CloudServerOutlined, KeyOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { getDashboardStats } from '@/api/dashboard';
import { listAuditLogs } from '@/api/audit';
import { formatRelative } from '@/utils/time';

export default function Dashboard() {
  const navigate = useNavigate();

  const { data: stats, isLoading: statsLoading } = useQuery({
    queryKey: ['dashboard-stats'],
    queryFn: getDashboardStats,
  });

  const { data: recentActivity, isLoading: activityLoading } = useQuery({
    queryKey: ['recent-activity'],
    queryFn: () => listAuditLogs({ page: 1, page_size: 10 }),
  });

  const statCards = [
    {
      title: 'Tenants',
      value: stats?.tenants.total ?? 0,
      suffix: stats ? `/ ${stats.tenants.active} active` : '',
      icon: <TeamOutlined style={{ fontSize: 24, color: '#1677ff' }} />,
      onClick: () => navigate('/admin/tenants'),
    },
    {
      title: 'Shares',
      value: stats?.shares.total ?? 0,
      icon: <ShareAltOutlined style={{ fontSize: 24, color: '#52c41a' }} />,
      onClick: () => navigate('/admin/shares'),
    },
    {
      title: 'Running Sandboxes',
      value: stats?.sandboxes.running ?? 0,
      icon: <CloudServerOutlined style={{ fontSize: 24, color: '#fa8c16' }} />,
      onClick: () => navigate('/admin/sandboxes'),
    },
    {
      title: 'Active API Keys',
      value: stats?.api_keys.active ?? 0,
      icon: <KeyOutlined style={{ fontSize: 24, color: '#722ed1' }} />,
      onClick: () => navigate('/admin/tenants'),
    },
  ];

  const activityColumns = [
    { title: 'Time', dataIndex: 'created_at', key: 'time', width: 140,
      render: (v: string) => formatRelative(v) },
    { title: 'Action', dataIndex: 'action', key: 'action', width: 180 },
    { title: 'Resource', dataIndex: 'resource_name', key: 'resource',
      render: (name: string, record: { resource_type: string }) =>
        name || record.resource_type },
  ];

  return (
    <div>
      <Typography.Title level={4} style={{ marginBottom: 24 }}>Dashboard</Typography.Title>
      {statsLoading ? <Skeleton active paragraph={{ rows: 2 }} /> : (
        <Row gutter={[16, 16]} style={{ marginBottom: 24 }}>
          {statCards.map((card) => (
            <Col xs={24} sm={12} lg={6} key={card.title}>
              <Card
                hoverable
                onClick={card.onClick}
                style={{ cursor: 'pointer' }}
              >
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                  <Statistic title={card.title} value={card.value} suffix={card.suffix} />
                  {card.icon}
                </div>
              </Card>
            </Col>
          ))}
        </Row>
      )}
      <Card title="Recent Activity">
        <Table
          dataSource={recentActivity?.logs ?? []}
          columns={activityColumns}
          rowKey="id"
          pagination={false}
          loading={activityLoading}
          size="small"
        />
      </Card>
    </div>
  );
}
