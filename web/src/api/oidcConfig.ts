import client from './client';

export interface OidcConfigDisplay {
  enabled: boolean;
  issuer_url: string;
  client_id: string;
  client_secret: string;
  redirect_uri: string;
  jwks_refresh_interval_secs: number;
  disable_password_login: boolean;
  auto_create_tenant: boolean;
}

export interface OidcConfigUpdateParams {
  enabled: boolean;
  issuer_url: string;
  client_id: string;
  client_secret?: string;
  redirect_uri: string;
  jwks_refresh_interval_secs?: number;
  disable_password_login: boolean;
  auto_create_tenant: boolean;
}

export async function getOidcFullConfig(): Promise<OidcConfigDisplay> {
  const res = await client.get('/system/oidc-config');
  return res.data;
}

export async function updateOidcConfig(params: OidcConfigUpdateParams): Promise<{ success: boolean }> {
  const res = await client.put('/system/oidc-config', params);
  return res.data;
}

export async function testOidcConfig(): Promise<{ success: boolean; message: string }> {
  const res = await client.post('/system/oidc-config/test');
  return res.data;
}
