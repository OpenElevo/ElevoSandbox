import { Card, Col, Row, Statistic, Typography, Skeleton, Button, Empty } from 'antd';
import { TeamOutlined, ShareAltOutlined, CloudServerOutlined, KeyOutlined } from '@ant-design/icons';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { getDashboardStats } from '@/api/dashboard';

export default function Dashboard() {
  const navigate = useNavigate();

  const { data: stats, isLoading: statsLoading } = useQuery({
    queryKey: ['dashboard-stats'],
    queryFn: getDashboardStats,
  });

  const statCards = [
    {
      title: '租户',
      value: stats ? `${stats.tenants.active}/${stats.tenants.total}` : '-',
      suffix: '活跃/总数',
      icon: <TeamOutlined style={{ fontSize: 24, color: '#1677ff' }} />,
      onClick: () => navigate('/admin/tenants'),
    },
    {
      title: '共享',
      value: stats?.shares.total ?? 0,
      icon: <ShareAltOutlined style={{ fontSize: 24, color: '#52c41a' }} />,
      onClick: () => navigate('/admin/shares'),
    },
    {
      title: '运行中沙箱',
      value: stats?.sandboxes.running ?? 0,
      icon: <CloudServerOutlined style={{ fontSize: 24, color: '#fa8c16' }} />,
      onClick: () => navigate('/admin/sandboxes'),
    },
    {
      title: '活跃 API Key',
      value: stats?.api_keys.active ?? 0,
      icon: <KeyOutlined style={{ fontSize: 24, color: '#722ed1' }} />,
      onClick: () => navigate('/admin/tenants'),
    },
  ];

  return (
    <div>
      <Typography.Title level={4} style={{ marginBottom: 24 }}>仪表盘</Typography.Title>
      <Row gutter={[16, 16]} style={{ marginBottom: 24 }}>
        {statCards.map((card) => (
          <Col xs={24} sm={12} lg={6} key={card.title}>
            <Card
              hoverable
              onClick={card.onClick}
              style={{ cursor: 'pointer' }}
            >
              {statsLoading ? (
                <Skeleton active paragraph={{ rows: 1 }} />
              ) : (
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                  <Statistic title={card.title} value={card.value} suffix={card.suffix} />
                  {card.icon}
                </div>
              )}
            </Card>
          </Col>
        ))}
      </Row>
      <Card title="最近活动" style={{ marginTop: 24 }}>
        <Empty description="审计日志功能即将完善" />
        <div style={{ textAlign: 'center', marginTop: 16 }}>
          <Button type="link" onClick={() => navigate('/admin/audit-logs')}>查看全部审计日志 →</Button>
        </div>
      </Card>
    </div>
  );
}
