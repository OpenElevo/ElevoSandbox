import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Spin, Result, App } from 'antd';
import { useAuthStore } from '@/stores/authStore';
import { exchangeSessionCode } from '@/api/oidc';

export default function LoginSuccess() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const { login, isAuthenticated } = useAuthStore();
  const { message } = App.useApp();
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const errorCode = searchParams.get('error');
  const sessionCode = searchParams.get('code');

  useEffect(() => {
    if (isAuthenticated) {
      navigate('/admin/dashboard', { replace: true });
      return;
    }
    if (errorCode) {
      setLoading(false);
      setError(errorCode);
      return;
    }
    if (!sessionCode) {
      setLoading(false);
      setError('missing_code');
      return;
    }

    let cancelled = false;
    (async () => {
      try {
        const result = await exchangeSessionCode(sessionCode);
        if (cancelled) return;
        login(result.token, 'oidc', result.user);
        message.success('SSO 登录成功');
        navigate('/admin/dashboard', { replace: true });
      } catch (err) {
        if (cancelled) return;
        const msg =
          (err as { response?: { data?: { error?: { message?: string } } } })
            ?.response?.data?.error?.message || '登录失败';
        setError(msg);
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => { cancelled = true; };
  }, [errorCode, sessionCode, login, isAuthenticated, navigate, message]);

  if (error) {
    return (
      <div style={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: '#f5f5f5',
      }}>
        <Result
          status="error"
          title="登录失败"
          subTitle={getErrorDescription(error)}
          extra={
            <a onClick={() => navigate('/admin/login')} style={{ cursor: 'pointer' }}>
              返回登录
            </a>
          }
        />
      </div>
    );
  }

  if (loading && !error) {
    return (
      <div style={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: '#f5f5f5',
      }}>
        <Spin size="large" tip="正在完成登录..." />
      </div>
    );
  }
}

function getErrorDescription(code: string): string {
  const map: Record<string, string> = {
    invalid_state: '登录请求已过期，请重新登录',
    missing_code: '登录凭证缺失，请重新登录',
    missing_state: '登录状态缺失，请重新登录',
    session_expired: '登录会话已过期，请重新登录',
    token_exchange_failed: '令牌交换失败，请重试',
    invalid_token: '身份验证失败，请重试',
    not_configured: 'SSO 未配置',
    internal_error: '服务器内部错误，请重试',
  };
  return map[code] || `登录失败: ${code}`;
}
