import { useEffect, useRef, useCallback } from 'react';
import { useAuthStore } from '@/stores/authStore';
import { oidcRefresh } from '@/api/oidc';

const REFRESH_INTERVAL_MS = 4 * 60 * 1000; // 4 minutes
const BACKOFF_BASE_MS = 30_000;
const BACKOFF_MAX_MS = 4 * 60 * 1000;

let refreshTimerId: ReturnType<typeof setTimeout> | null = null;

/** Clear the OIDC refresh timer (call on 401 or logout) */
export function clearOidcRefreshTimer() {
  if (refreshTimerId) {
    clearTimeout(refreshTimerId);
    refreshTimerId = null;
  }
}

/** Start periodic OIDC token refresh when logged in via OIDC */
export function useOidcRefresh() {
  const loginMethod = useAuthStore((s) => s.loginMethod);
  const isAuthenticated = useAuthStore((s) => s.isAuthenticated);
  const backoffRef = useRef(REFRESH_INTERVAL_MS);
  const mountedRef = useRef(true);

  const scheduleRefresh = useCallback(() => {
    clearOidcRefreshTimer();
    refreshTimerId = setTimeout(async () => {
      if (!mountedRef.current) return;
      try {
        await oidcRefresh();
        backoffRef.current = REFRESH_INTERVAL_MS; // reset on success
      } catch {
        // Exponential backoff: start at 30s, double each failure, max 4min
        backoffRef.current = Math.min(
          backoffRef.current >= REFRESH_INTERVAL_MS ? BACKOFF_BASE_MS : backoffRef.current * 2,
          BACKOFF_MAX_MS,
        );
      }
      if (mountedRef.current && isAuthenticated && loginMethod === 'oidc') {
        scheduleRefresh();
      }
    }, backoffRef.current);
  }, [isAuthenticated, loginMethod]);

  useEffect(() => {
    mountedRef.current = true;

    if (isAuthenticated && loginMethod === 'oidc') {
      scheduleRefresh();
    }

    // Refresh on page visibility change
    const handleVisibility = () => {
      if (document.visibilityState === 'visible' && isAuthenticated && loginMethod === 'oidc') {
        oidcRefresh().catch(() => {});
      }
    };
    document.addEventListener('visibilitychange', handleVisibility);

    // Refresh on network recovery
    const handleOnline = () => {
      if (isAuthenticated && loginMethod === 'oidc') {
        oidcRefresh().catch(() => {});
      }
    };
    window.addEventListener('online', handleOnline);

    return () => {
      mountedRef.current = false;
      clearOidcRefreshTimer();
      document.removeEventListener('visibilitychange', handleVisibility);
      window.removeEventListener('online', handleOnline);
    };
  }, [isAuthenticated, loginMethod, scheduleRefresh]);
}
