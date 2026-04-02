import { Layout, Button, Space, Typography, App, Avatar } from 'antd';
import { MenuFoldOutlined, MenuUnfoldOutlined, LogoutOutlined, UserOutlined } from '@ant-design/icons';
import { useAuth } from '@/hooks/useAuth';
import { useAuthStore } from '@/stores/authStore';

const { Header } = Layout;

interface TopBarProps {
  collapsed: boolean;
  onToggle: () => void;
}

export default function TopBar({ collapsed, onToggle }: TopBarProps) {
  const { logout } = useAuth();
  const { modal } = App.useApp();
  const user = useAuthStore((s) => s.user);

  const handleLogout = () => {
    modal.confirm({
      title: '确认退出',
      content: '确定要退出登录吗？',
      okText: '退出',
      cancelText: '取消',
      onOk: logout,
    });
  };

  const displayName = user?.name || '管理员';

  return (
    <Header style={{
      padding: '0 24px',
      background: '#fff',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      borderBottom: '1px solid #f0f0f0',
    }}>
      <Space align="center">
        <Button
          type="text"
          icon={collapsed ? <MenuUnfoldOutlined /> : <MenuFoldOutlined />}
          onClick={onToggle}
        />
        <Typography.Text strong style={{ fontSize: 16, color: '#1677ff' }}>
          Elevo Admin
        </Typography.Text>
      </Space>
      <Space align="center" size="middle">
        <Space size="small">
          <Avatar size="small" src={user?.picture || undefined} icon={!user?.picture ? <UserOutlined /> : undefined} />
          <Typography.Text type="secondary">{displayName}</Typography.Text>
        </Space>
        <Button
          type="text"
          icon={<LogoutOutlined />}
          onClick={handleLogout}
        >
          退出
        </Button>
      </Space>
    </Header>
  );
}
