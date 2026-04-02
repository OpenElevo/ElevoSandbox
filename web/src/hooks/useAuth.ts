import { useAuthStore } from '@/stores/authStore';
import { login as apiLogin } from '@/api/auth';
import { oidcLogout } from '@/api/oidc';
import { clearOidcRefreshTimer } from '@/hooks/useOidcRefresh';
import { useNavigate } from 'react-router-dom';
import { useCallback } from 'react';

export function useAuth() {
  const { isAuthenticated, login: storeLogin, logout: storeLogout, loginMethod } = useAuthStore();
  const navigate = useNavigate();

  const login = useCallback(async (password: string) => {
    const token = await apiLogin(password);
    storeLogin(token);
  }, [storeLogin]);

  const logout = useCallback(async () => {
    clearOidcRefreshTimer();
    let idpLogoutUrl: string | null = null;
    if (loginMethod === 'oidc') {
      try {
        const res = await oidcLogout();
        idpLogoutUrl = res.idp_logout_url;
      } catch {
        // Ignore OIDC logout errors, still clear local state
      }
    }
    storeLogout();
    if (idpLogoutUrl) {
      window.location.href = idpLogoutUrl;
    } else {
      navigate('/admin/login');
    }
  }, [storeLogout, loginMethod, navigate]);

  return { isAuthenticated, login, logout };
}
