<script>
  import { onMount } from 'svelte';
  import { api, setUser, getUser } from './api.js';
  import Dashboard from './pages/Dashboard.svelte';
  import Manifests from './pages/Manifests.svelte';
  import Crates from './pages/Crates.svelte';
  import Booths from './pages/Booths.svelte';
  import Exhibits from './pages/Exhibits.svelte';
  import Exceptions from './pages/Exceptions.svelte';
  import Changes from './pages/Changes.svelte';
  import Checkout from './pages/Checkout.svelte';
  import Gate from './pages/Gate.svelte';

  const TABS = [
    ['dashboard', '总览核对'],
    ['manifests', '清单申报'],
    ['crates', '按箱接收'],
    ['booths', '展位看板'],
    ['exhibits', '展品追溯'],
    ['exceptions', '异常与照片'],
    ['changes', '变更审批'],
    ['checkout', '撤展放行'],
    ['gate', '闸口扫码'],
  ];
  const ROLE_LABEL = { exhibitor: '参展商', gatekeeper: '入口闸口', venue: '场馆布展', manager: '场馆经理' };

  let users = [];
  let uid = getUser();
  let me = null;
  let tab = 'dashboard';
  let toasts = [];
  let seq = 0;

  function toast(msg, kind = 'err') {
    const id = ++seq;
    toasts = [...toasts, { id, msg, kind }];
    setTimeout(() => (toasts = toasts.filter((t) => t.id !== id)), 5200);
  }

  async function loadUsers() {
    users = await api.get('/api/users');
    const exists = users.find((u) => u.id === uid);
    if (!exists) { uid = 'u-gate'; setUser(uid); }
    me = users.find((u) => u.id === uid) || null;
  }

  function switchUser(e) {
    setUser(e.target.value);
    uid = e.target.value;
    me = users.find((u) => u.id === uid);
  }

  // 全局事件总线：子页面操作后刷新
  const listeners = new Set();
  const bus = {
    emit: () => listeners.forEach((fn) => fn()),
    on: (fn) => { listeners.add(fn); return () => listeners.delete(fn); },
  };

  onMount(loadUsers);
</script>

<header class="topbar">
  <h1>📦 展品入馆封箱与撤展核对</h1>
  <span class="who" style="margin-left:12px">
    {#if me}当前身份：<b>{me.name}</b>（{ROLE_LABEL[me.role]}）{/if}
  </span>
  <span class="spacer"></span>
  <label style="display:flex;align-items:center;gap:6px">
    <span class="muted">切换身份</span>
    <select value={uid} on:change={switchUser}>
      {#each users as u}
        <option value={u.id}>{u.name} · {ROLE_LABEL[u.role]}</option>
      {/each}
    </select>
  </label>
</header>

<nav class="tabs">
  {#each TABS as [key, label]}
    <button class={tab === key ? 'active' : ''} on:click={() => (tab = key)}>{label}</button>
  {/each}
</nav>

<main>
  {#if me}
    {#if tab === 'dashboard'}<Dashboard on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {:else if tab === 'manifests'}<Manifests on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {:else if tab === 'crates'}<Crates on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {:else if tab === 'booths'}<Booths on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {:else if tab === 'exhibits'}<Exhibits on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {:else if tab === 'exceptions'}<Exceptions on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {:else if tab === 'changes'}<Changes on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {:else if tab === 'checkout'}<Checkout on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {:else if tab === 'gate'}<Gate on:notify={e => toast(e.detail.msg, e.detail.kind)} {bus} />
    {/if}
  {:else}
    <p class="muted">加载中…</p>
  {/if}
</main>

<div class="toast">
  {#each toasts as t}
    <div class="t {t.kind}">{t.msg}</div>
  {/each}
</div>
