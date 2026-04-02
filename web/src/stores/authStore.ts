import { create } from 'zustand';
import type { LoginMethod, OidcUserInfo } from '@/types';

interface AuthState {
  token: string | null;
  isAuthenticated: boolean;
  loginMethod: LoginMethod;
  user: OidcUserInfo | null;
  login: (token: string, method?: LoginMethod, user?: OidcUserInfo | null) => void;
  logout: () => void;
  setToken: (token: string) => void;
}

export const useAuthStore = create<AuthState>((set) => ({
  token: localStorage.getItem('token'),
  isAuthenticated: !!localStorage.getItem('token'),
  loginMethod: (localStorage.getItem('loginMethod') as LoginMethod) || null,
  user: (() => {
    try {
      const raw = localStorage.getItem('oidcUser');
      return raw ? JSON.parse(raw) as OidcUserInfo : null;
    } catch {
      return null;
    }
  })(),
  login: (token: string, method?: LoginMethod, user?: OidcUserInfo | null) => {
    localStorage.setItem('token', token);
    localStorage.setItem('loginMethod', method || 'password');
    if (user) {
      localStorage.setItem('oidcUser', JSON.stringify(user));
    } else if (method === 'password') {
      localStorage.removeItem('oidcUser');
    }
    set({ token, isAuthenticated: true, loginMethod: method || 'password', user: user ?? null });
  },
  logout: () => {
    localStorage.removeItem('token');
    localStorage.removeItem('loginMethod');
    localStorage.removeItem('oidcUser');
    set({ token: null, isAuthenticated: false, loginMethod: null, user: null });
  },
  setToken: (token: string) => {
    localStorage.setItem('token', token);
    set({ token, isAuthenticated: true });
  },
}));
