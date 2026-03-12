import { Layout, Menu } from 'antd';
import {
  DashboardOutlined,
  TeamOutlined,
  ShareAltOutlined,
  CloudServerOutlined,
  AuditOutlined,
} from '@ant-design/icons';
import { useNavigate, useLocation } from 'react-router-dom';

const { Sider } = Layout;

interface SidebarProps {
  collapsed: boolean;
}

const menuItems = [
  { key: '/admin/dashboard', icon: <DashboardOutlined />, label: '仪表盘' },
  { key: '/admin/tenants', icon: <TeamOutlined />, label: '租户管理' },
  { key: '/admin/shares', icon: <ShareAltOutlined />, label: '共享管理' },
  { key: '/admin/sandboxes', icon: <CloudServerOutlined />, label: '沙箱管理' },
  { key: '/admin/audit', icon: <AuditOutlined />, label: '审计日志' },
];

export default function Sidebar({ collapsed }: SidebarProps) {
  const navigate = useNavigate();
  const location = useLocation();

  // Match current path to menu key (handle detail pages)
  const selectedKey = menuItems.find((item) =>
    location.pathname === item.key || location.pathname.startsWith(item.key + '/')
  )?.key || '/admin/dashboard';

  return (
    <Sider
      trigger={null}
      collapsible
      collapsed={collapsed}
      width={200}
      style={{
        overflow: 'auto',
        height: '100vh',
        position: 'sticky',
        top: 0,
        left: 0,
      }}
    >
      <div style={{
        height: 48,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        color: '#fff',
        fontSize: collapsed ? 16 : 18,
        fontWeight: 600,
        borderBottom: '1px solid rgba(255,255,255,0.1)',
      }}>
        {collapsed ? 'E' : 'Elevo 管理'}
      </div>
      <Menu
        theme="dark"
        mode="inline"
        selectedKeys={[selectedKey]}
        items={menuItems}
        onClick={({ key }) => navigate(key)}
      />
    </Sider>
  );
}
