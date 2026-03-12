import client from './client';

export async function login(password: string): Promise<string> {
  const res = await client.post('/auth/login', { password });
  return res.data.token;
}
