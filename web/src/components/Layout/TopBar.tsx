import { Layout, Button, Space, Typography } from 'antd';
import { MenuFoldOutlined, MenuUnfoldOutlined, LogoutOutlined } from '@ant-design/icons';
import { useAuth } from '@/hooks/useAuth';

const { Header } = Layout;

interface TopBarProps {
  collapsed: boolean;
  onToggle: () => void;
}

export default function TopBar({ collapsed, onToggle }: TopBarProps) {
  const { logout } = useAuth();

  return (
    <Header style={{
      padding: '0 24px',
      background: '#fff',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      borderBottom: '1px solid #f0f0f0',
    }}>
      <Button
        type="text"
        icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
        onClick={onToggle}
      />
      <Space>
        <Typography.Text type="secondary">Admin</Typography.Text>
        <Button
          type="text"
          icon={<LogoutOutlined />}
          onClick={logout}
        >
          Logout
        </Button>
      </Space>
    </Header>
  );
}
