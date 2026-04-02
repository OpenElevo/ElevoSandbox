import client from './client';

export async function getOidcConfig(): Promise<{ enabled: boolean; disable_password_login: boolean }> {
  const res = await client.get('/auth/oidc/config');
  return res.data;
}

export async function authorizeOidc(): Promise<{ authorize_url: string }> {
  const res = await client.post('/auth/oidc/authorize');
  return res.data;
}

export async function exchangeSessionCode(code: string): Promise<{
  token: string;
  user: { name: string; email: string | null; picture: string | null; is_admin: boolean };
}> {
  const res = await client.get(`/auth/oidc/session?code=${encodeURIComponent(code)}`);
  return res.data;
}

export async function oidcRefresh(): Promise<{ success: boolean }> {
  const res = await client.post('/auth/oidc/refresh');
  return res.data;
}

export async function oidcLogout(): Promise<{ idp_logout_url: string | null }> {
  const res = await client.post('/auth/logout');
  return res.data;
}
