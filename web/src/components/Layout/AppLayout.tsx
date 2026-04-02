import { useState } from 'react';
import { Layout, Breadcrumb } from 'antd';
import { Outlet, useLocation, Link } from 'react-router-dom';
import Sidebar from './Sidebar';
import TopBar from './TopBar';
import { useBreadcrumbStore } from '@/stores/breadcrumbStore';
import { useOidcRefresh } from '@/hooks/useOidcRefresh';

const { Content } = Layout;

const ROUTE_LABELS: Record<string, string> = {
  dashboard: '仪表盘',
  tenants: '租户管理',
  shares: '共享管理',
  sandboxes: '沙箱管理',
  audit: '审计日志',
};

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const UUID_SHORT_RE = /^[0-9a-f-]{8,}$/i;

function isUuidLike(segment: string): boolean {
  return UUID_RE.test(segment) || (UUID_SHORT_RE.test(segment) && segment.length >= 8);
}

function useBreadcrumbItems() {
  const location = useLocation();
  const names = useBreadcrumbStore((s) => s.names);

  // Strip /admin prefix and split
  const path = location.pathname.replace(/^\/admin\/?/, '');
  if (!path) return [];

  const segments = path.split('/').filter(Boolean);
  if (segments.length <= 1) return []; // No breadcrumb for top-level pages

  const items: { title: React.ReactNode }[] = [];
  for (let i = 0; i < segments.length; i++) {
    const segment = segments[i];
    const href = '/admin/' + segments.slice(0, i + 1).join('/');

    let label: string;
    if (ROUTE_LABELS[segment]) {
      label = ROUTE_LABELS[segment];
    } else if (isUuidLike(segment) && names[segment]) {
      label = names[segment];
    } else if (isUuidLike(segment)) {
      label = segment.slice(0, 8) + '...';
    } else {
      label = segment;
    }

    if (i < segments.length - 1) {
      items.push({ title: <Link to={href}>{label}</Link> });
    } else {
      items.push({ title: label });
    }
  }
  return items;
}

export default function AppLayout() {
  useOidcRefresh();
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
