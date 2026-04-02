import { useState, useEffect, useRef } from 'react';
import { Card, Input, Button, Typography, App, Divider, Alert } from 'antd';
import { LockOutlined, LoginOutlined } from '@ant-design/icons';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useAuthStore } from '@/stores/authStore';
import { login as apiLogin } from '@/api/auth';
import { getOidcConfig, authorizeOidc } from '@/api/oidc';

export default function Login() {
  const [password, setPassword] = useState('');
  const [loading, setLoading] = useState(false);
  const [ssoLoading, setSsoLoading] = useState(false);
  const [error, setError] = useState('');
  const [oidcEnabled, setOidcEnabled] = useState(false);
  const [disablePassword, setDisablePassword] = useState(false);
  const [configFetched, setConfigFetched] = useState(false);
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

  // Fetch OIDC config on mount
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const config = await getOidcConfig();
        if (cancelled) return;
        setOidcEnabled(config.enabled);
        setDisablePassword(config.disable_password_login);
        setConfigFetched(true);
      } catch {
        // OIDC not configured, show password login only
        setConfigFetched(true);
      }
    })();
    return () => { cancelled = true; };
  }, []);

  // Handle URL error parameters from OIDC callback redirect
  const urlError = searchParams.get('error');
  const urlActivated = searchParams.get('activated');

  const handleLogin = async () => {
    if (!password.trim()) return;
    setLoading(true);
    setError('');
    try {
      const token = await apiLogin(password);
      storeLogin(token);
      message.success('登录成功');
      navigate(searchParams.get('redirect') || '/admin/dashboard', { replace: true });
    } catch (err: unknown) {
      const msg = (err as { response?: { data?: { error?: { message?: string } } } })
        ?.response?.data?.error?.message || '登录失败，请检查密码';
      setError(msg);
      inputRef.current?.focus();
    } finally {
      setLoading(false);
    }
  };

  const handleSsoLogin = async () => {
    setSsoLoading(true);
    try {
      const { authorize_url } = await authorizeOidc();
      window.location.href = authorize_url;
    } catch {
      setSsoLoading(false);
      message.error('SSO 启动失败，请使用密码登录');
    }
  };

  const getAlertMessage = () => {
    if (urlError) {
      const desc = searchParams.get('desc');
      const map: Record<string, string> = {
        invalid_state: '登录请求已过期，请重新登录',
        missing_state: '登录状态缺失，请重新登录',
        token_exchange_failed: '令牌交换失败，请重试',
        invalid_token: '身份验证失败，请重试',
        not_configured: 'SSO 未配置',
        internal_error: '服务器内部错误，请重试',
        access_denied: '访问被拒绝',
        sso_error: desc || '认证服务错误',
      };
      return { type: 'error' as const, message: map[urlError] || `登录失败: ${urlError}` };
    }
    if (urlActivated === 'true') {
      return { type: 'success' as const, message: '账号已激活，请使用 SSO 重新登录' };
    }
    return null;
  };

  const alert = getAlertMessage();

  // Defensive: no available login method (should never happen with proper backend validation)
  const noLoginMethod = configFetched && !oidcEnabled && disablePassword;

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
          <Typography.Title level={3} style={{ margin: 0 }}>Elevo 管理后台</Typography.Title>
          <Typography.Text type="secondary">
            {oidcEnabled ? '欢迎使用统一身份认证' : '请输入管理员密码登录'}
          </Typography.Text>
        </div>

        {alert && (
          <Alert
            type={alert.type}
            message={alert.message}
            showIcon
            closable
            style={{ marginBottom: 16 }}
            onClose={() => navigate('/admin/login', { replace: true })}
          />
        )}

        {noLoginMethod && (
          <Alert
            type="error"
            message="系统配置错误：没有可用的登录方式，请联系管理员"
            showIcon
            style={{ marginBottom: 16 }}
          />
        )}

        {oidcEnabled && (
          <Button
            type="primary"
            size="large"
            block
            loading={ssoLoading}
            icon={<LoginOutlined />}
            onClick={handleSsoLogin}
            style={{ marginBottom: disablePassword ? 0 : 24 }}
          >
            使用 SSO 登录
          </Button>
        )}

        {!disablePassword && (
          <>
            {oidcEnabled && (
              <>
                <Divider plain style={{ margin: '0 0 24px 0', fontSize: 12 }}>
                  或使用密码登录
                </Divider>
              </>
            )}
            <Input.Password
              ref={inputRef as never}
              size="large"
              prefix={<LockOutlined />}
              placeholder="管理员密码"
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
              disabled={!password.trim()}
              onClick={handleLogin}
              style={{ marginTop: 16 }}
            >
              登录
            </Button>
          </>
        )}
      </Card>
    </div>
  );
}
