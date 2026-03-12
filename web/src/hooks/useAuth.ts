import { useAuthStore } from '@/stores/authStore';
import { login as apiLogin } from '@/api/auth';
import { useNavigate } from 'react-router-dom';
import { useCallback } from 'react';

export function useAuth() {
  const { isAuthenticated, login: storeLogin, logout: storeLogout } = useAuthStore();
  const navigate = useNavigate();

  const login = useCallback(async (password: string) => {
    const token = await apiLogin(password);
    storeLogin(token);
  }, [storeLogin]);

  const logout = useCallback(() => {
    storeLogout();
    navigate('/admin/login');
  }, [storeLogout, navigate]);

  return { isAuthenticated, login, logout };
}
