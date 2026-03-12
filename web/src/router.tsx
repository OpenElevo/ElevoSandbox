import { createBrowserRouter, Navigate, Outlet } from 'react-router-dom';
import { useAuthStore } from '@/stores/authStore';
import AppLayout from '@/components/Layout/AppLayout';
import Login from '@/pages/Login';
import Dashboard from '@/pages/Dashboard';
import TenantList from '@/pages/tenants/TenantList';
import TenantDetail from '@/pages/tenants/TenantDetail';
import ShareList from '@/pages/shares/ShareList';
import ShareDetail from '@/pages/shares/ShareDetail';
import SandboxList from '@/pages/sandboxes/SandboxList';
import AuditLogList from '@/pages/audit/AuditLogList';

function RequireAuth() {
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  if (!isAuthenticated) {
    const redirect = window.location.pathname;
    return <Navigate to={`/admin/login?redirect=${encodeURIComponent(redirect)}`} replace />;
  }
  return <Outlet />;
}

export const router = createBrowserRouter([
  {
    path: '/admin/login',
    element: <Login />,
  },
  {
    path: '/admin',
    element: <RequireAuth />,
    children: [
      {
        element: <AppLayout />,
        children: [
          { index: true, element: <Navigate to="/admin/dashboard" replace /> },
          { path: 'dashboard', element: <Dashboard /> },
          { path: 'tenants', element: <TenantList /> },
          { path: 'tenants/:id', element: <TenantDetail /> },
          { path: 'shares', element: <ShareList /> },
          { path: 'shares/:id', element: <ShareDetail /> },
          { path: 'sandboxes', element: <SandboxList /> },
          { path: 'audit', element: <AuditLogList /> },
        ],
      },
    ],
  },
  {
    path: '*',
    element: <Navigate to="/admin" replace />,
  },
]);
