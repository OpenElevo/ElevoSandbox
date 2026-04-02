import axios from 'axios';
import { message } from 'antd';
import { useAuthStore } from '../stores/authStore';
import { clearOidcRefreshTimer } from '../hooks/useOidcRefresh';

const client = axios.create({
  baseURL: '/api/v1',
  timeout: 30000,
  headers: { 'Content-Type': 'application/json' },
});

// Request interceptor: attach JWT token
client.interceptors.request.use((config) => {
  const token = localStorage.getItem('token');
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

// Response interceptor: handle token refresh and errors
let isRefreshing = false;

client.interceptors.response.use(
  (response) => {
    // Auto-refresh token if server sends a new one
    const refreshedToken = response.headers['x-refreshed-token'];
    if (refreshedToken && !isRefreshing) {
      isRefreshing = true;
      useAuthStore.getState().setToken(refreshedToken);
      setTimeout(() => { isRefreshing = false; }, 1000);
    }
    return response;
  },
  (error) => {
    const status = error.response?.status;
    const errorMsg = error.response?.data?.error?.message;

    if (status === 401) {
      clearOidcRefreshTimer();
      useAuthStore.getState().logout();
      if (window.location.pathname !== '/admin/login') {
        message.error('登录已过期，请重新登录');
        window.location.href = '/admin/login?redirect=' + encodeURIComponent(window.location.pathname);
      }
    } else if (status === 403) {
      message.error(errorMsg || '无权限执行此操作');
    } else if (status === 429) {
      message.error('请求过于频繁，请稍后再试');
    } else if (status && status >= 500) {
      message.error(errorMsg || '服务器内部错误，请稍后重试');
    } else if (!error.response) {
      message.error('网络连接失败，请检查网络');
    }
    return Promise.reject(error);
  },
);

export default client;
