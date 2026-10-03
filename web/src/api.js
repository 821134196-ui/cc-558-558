import { get } from 'svelte/store';
import { currentUser } from './stores.js';

export async function api(path, { method = 'GET', body, form } = {}) {
  const u = get(currentUser);
  const headers = {};
  if (u) headers['X-User-Id'] = String(u.id);
  let payload;
  if (form) {
    payload = form; // FormData，浏览器自动带 multipart 边界
  } else if (body !== undefined) {
    headers['Content-Type'] = 'application/json';
    payload = JSON.stringify(body);
  }
  const res = await fetch(`/api${path}`, { method, headers, body: payload });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `请求失败 (${res.status})`);
  return data;
}
