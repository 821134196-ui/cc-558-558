// 极简 API 客户端：当前操作人通过 X-User-Id 头识别
let currentUserId = localStorage.getItem('uid') || 'u-gate';

export function setUser(id) {
  currentUserId = id;
  localStorage.setItem('uid', id);
}
export function getUser() {
  return currentUserId;
}

async function request(method, path, body, isForm) {
  const opts = {
    method,
    headers: { 'X-User-Id': currentUserId },
  };
  if (body !== undefined) {
    if (isForm) {
      opts.body = body; // FormData，浏览器自动加 boundary
    } else {
      opts.headers['Content-Type'] = 'application/json';
      opts.body = JSON.stringify(body);
    }
  }
  const res = await fetch(path, opts);
  const text = await res.text();
  let data = null;
  try { data = text ? JSON.parse(text) : null; } catch { data = text; }
  if (!res.ok) {
    const msg = data && data.error ? data.error : `HTTP ${res.status}`;
    const err = new Error(msg);
    err.status = res.status;
    throw err;
  }
  return data;
}

export const api = {
  get: (p) => request('GET', p),
  post: (p, b) => request('POST', p, b ?? {}),
  upload: (p, form) => request('POST', p, form, true),
};

export function fmtTime(t) {
  if (!t) return '';
  const d = new Date(t);
  return d.toLocaleString('zh-CN', { hour12: false });
}

export const STATUS_LABEL = {
  DECLARED: '已申报', RECEIVED: '已入馆', PLACED: '在展位',
  OUT: '已出馆', REPLACED: '已替换',
};

export const EVENT_LABEL = {
  DECLARED: '申报', CRATED: '装箱', RECEIVED: '按箱入馆', PLACED: '布展占位',
  TRANSFER: '展位转移', RETRACT: '展位撤下', REPLACE: '临时替换',
  EXCEPTION: '登记异常', RESOLVED: '异常处理', CHECK_OUT: '核验出馆', EARLY_OUT: '批准提前撤展',
};
