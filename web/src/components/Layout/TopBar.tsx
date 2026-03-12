import { Layout, Button, Space, Typography, App } from 'antd';
import { MenuFoldOutlined, MenuUnfoldOutlined, LogoutOutlined } from '@ant-design/icons';
import { useAuth } from '@/hooks/useAuth';

const { Header } = Layout;

interface TopBarProps {
  collapsed: boolean;
  onToggle: () => void;
}

export default function TopBar({ collapsed, onToggle }: TopBarProps) {
  const { logout } = useAuth();
  const { modal } = App.useApp();

  const handleLogout = () => {
    modal.confirm({
      title: '确认退出',
      content: '确定要退出登录吗？',
      okText: '退出',
      cancelText: '取消',
      onOk: logout,
    });
  };

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
        <Typography.Text type="secondary">管理员</Typography.Text>
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
