import { useState, useEffect, useRef } from 'react';
import { Card, Input, Button, Typography, App } from 'antd';
import { LockOutlined } from '@ant-design/icons';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useAuthStore } from '@/stores/authStore';
import { login as apiLogin } from '@/api/auth';

export default function Login() {
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const inputRef = useRef<ReturnType<typeof Input.Password> & { focus: () => void }>(null);
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { isAuthenticated, login: storeLogin } = useAuthStore();
  const { message } = App.useApp();

  // Redirect if already authenticated
  useEffect(() => {
    if (isAuthenticated) {
      navigate(searchParams.get('redirect') || '/admin/dashboard', { replace: true });
    }
  }, [isAuthenticated, navigate, searchParams]);

  const handleLogin = async () => {
    if (!password.trim()) return;
    setLoading(true);
    setError('');
    try {
      const token = await apiLogin(password);
      storeLogin(token);
      message.success('Login successful');
      navigate(searchParams.get('redirect') || '/admin/dashboard', { replace: true });
    } catch (err: unknown) {
      const msg = (err as { response?: { data?: { error?: { message?: string } } } })
        ?.response?.data?.error?.message || 'Login failed';
      setError(msg);
      inputRef.current?.focus();
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{
      minHeight: '100vh',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      background: '#f5f5f5',
    }}>
      <Card style={{ width: 400 }}>
        <div style={{ textAlign: 'center', marginBottom: 32 }}>
          <Typography.Title level={3} style={{ margin: 0 }}>Elevo Admin</Typography.Title>
          <Typography.Text type="secondary">Sign in to continue</Typography.Text>
        </div>
        <Input.Password
          ref={inputRef as never}
          size="large"
          prefix={<LockOutlined />}
          placeholder="Admin password"
          value={password}
          onChange={(e) => { setPassword(e.target.value); setError(''); }}
          onPressEnter={handleLogin}
          status={error ? 'error' : undefined}
          autoFocus
        />
        {error && (
          <Typography.Text type="danger" style={{ display: 'block', marginTop: 8 }}>
            {error}
          </Typography.Text>
        )}
        <Button
          type="primary"
          size="large"
          block
          loading={loading}
          onClick={handleLogin}
          style={{ marginTop: 16 }}
        >
          Sign In
        </Button>
      </Card>
    </div>
  );
}
