import axios from 'axios';

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
      localStorage.setItem('token', refreshedToken);
      setTimeout(() => { isRefreshing = false; }, 1000);
    }
    return response;
  },
  (error) => {
    if (error.response?.status === 401) {
      localStorage.removeItem('token');
      if (window.location.pathname !== '/admin/login') {
        window.location.href = '/admin/login?redirect=' + encodeURIComponent(window.location.pathname);
      }
    }
    return Promise.reject(error);
  },
);

export default client;
