import { useState } from 'react';
import { Layout, Breadcrumb } from 'antd';
import { Outlet, useLocation, Link } from 'react-router-dom';
import Sidebar from './Sidebar';
import TopBar from './TopBar';

const { Content } = Layout;

const ROUTE_LABELS: Record<string, string> = {
  dashboard: '仪表盘',
  tenants: '租户管理',
  shares: '共享管理',
  sandboxes: '沙箱管理',
  audit: '审计日志',
};

function useBreadcrumbItems() {
  const location = useLocation();
  // Strip /admin prefix and split
  const path = location.pathname.replace(/^\/admin\/?/, '');
  if (!path) return [];

  const segments = path.split('/').filter(Boolean);
  if (segments.length <= 1) return []; // No breadcrumb for top-level pages

  const items: { title: React.ReactNode }[] = [];
  for (let i = 0; i < segments.length; i++) {
    const segment = segments[i];
    const label = ROUTE_LABELS[segment] || segment;
    const href = '/admin/' + segments.slice(0, i + 1).join('/');

    if (i < segments.length - 1) {
      items.push({ title: <Link to={href}>{label}</Link> });
    } else {
      // Last segment is current page, not clickable
      // For detail pages (UUID-like), show truncated ID
      const isUuid = /^[0-9a-f-]{8,}$/i.test(segment);
      items.push({ title: isUuid ? segment.slice(0, 8) + '...' : label });
    }
  }
  return items;
}

export default function AppLayout() {
  const [collapsed, setCollapsed] = useState(() => {
    return localStorage.getItem('sidebar_collapsed') === 'true';
  });

  const toggleCollapsed = () => {
    const next = !collapsed;
    setCollapsed(next);
    localStorage.setItem('sidebar_collapsed', String(next));
  };

  const breadcrumbItems = useBreadcrumbItems();

  return (
    <Layout style={{ minHeight: '100vh' }}>
      <Sidebar collapsed={collapsed} />
      <Layout>
        <TopBar collapsed={collapsed} onToggle={toggleCollapsed} />
        <Content style={{ margin: 24, minWidth: 0 }}>
          {breadcrumbItems.length > 0 && (
            <Breadcrumb items={breadcrumbItems} style={{ marginBottom: 16 }} />
          )}
          <Outlet />
        </Content>
      </Layout>
    </Layout>
  );
}
