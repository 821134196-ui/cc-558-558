import { writable } from 'svelte/store';

const saved = typeof localStorage !== 'undefined' ? localStorage.getItem('expo-user') : null;
export const currentUser = writable(saved ? JSON.parse(saved) : null);
currentUser.subscribe((u) => {
  if (typeof localStorage === 'undefined') return;
  if (u) localStorage.setItem('expo-user', JSON.stringify(u));
  else localStorage.removeItem('expo-user');
});

export const roleLabels = {
  exhibitor: '参展商',
  receiver: '入口接收员',
  staff: '场馆人员',
  manager: '场馆主管',
};

export const statusLabels = {
  declared: '清单登记',
  received: '已入馆',
  placed: '在展位',
  verified_out: '已核实出馆',
  exited: '已出馆',
  missing_closed: '缺件结案',
};

export const eventLabels = {
  item_declared: '清单登记',
  item_received: '按箱入馆',
  item_placed: '布展占位',
  item_transferred: '展位转移',
  item_verified_out: '撤展核实',
  item_exited: '闸口出馆',
  item_missing_closed: '缺件结案',
  exit_accounted_missing: '缺件核销',
  exception_reported: '异常上报',
  exception_approved: '异常批准',
  exception_rejected: '异常驳回',
  amendment_requested: '变更申请',
  amendment_confirmed: '变更确认',
  amendment_rejected: '变更驳回',
  booth_transferred: '展位转借',
  item_replaced: '被替换',
  pass_issued: '开具放行凭证',
};

export const amendmentLabels = {
  replacement: '临时替换展品',
  booth_transfer: '转借展位',
  early_exit: '提前撤展',
};
