<script>
  import { onMount } from 'svelte';
  import { currentUser, roleLabels } from './stores.js';
  import { api } from './api.js';
  import Board from './pages/Board.svelte';
  import Manifest from './pages/Manifest.svelte';
  import Receiving from './pages/Receiving.svelte';
  import Placement from './pages/Placement.svelte';
  import Teardown from './pages/Teardown.svelte';
  import Exceptions from './pages/Exceptions.svelte';
  import Amendments from './pages/Amendments.svelte';
  import ExitPass from './pages/ExitPass.svelte';
  import ItemDetail from './components/ItemDetail.svelte';

  const pages = [
    { id: 'board', name: '展位看板', comp: Board },
    { id: 'manifest', name: '入馆清单', comp: Manifest },
    { id: 'receiving', name: '入口收箱', comp: Receiving },
    { id: 'placement', name: '展位登记', comp: Placement },
    { id: 'teardown', name: '撤展核实', comp: Teardown },
    { id: 'exceptions', name: '异常处理', comp: Exceptions },
    { id: 'amendments', name: '变更申请', comp: Amendments },
    { id: 'exitpass', name: '放行与闸口', comp: ExitPass },
  ];

  let route = 'board';
  let users = [];
  let detailItemId = null;

  function syncRoute() {
    route = location.hash.replace(/^#\/?/, '') || 'board';
  }
  onMount(async () => {
    syncRoute();
    window.addEventListener('hashchange', syncRoute);
    try { users = await api('/users'); } catch (e) { /* 后端未启动时忽略 */ }
    return () => window.removeEventListener('hashchange', syncRoute);
  });

  $: activePage = pages.find((p) => p.id === route) || pages[0];

  function pickUser(e) {
    const id = Number(e.target.value);
    const found = users.find((u) => u.user.id === id);
    currentUser.set(found ? found.user : null);
  }
  function openDetail(e) { detailItemId = e.detail; }
</script>

<header class="topbar">
  <h1>展会展品核对系统</h1>
  <nav>
    {#each pages as p}
      <a href="#/{p.id}" class:active={activePage.id === p.id}>{p.name}</a>
    {/each}
  </nav>
  <div class="userpick">
    <span>当前身份</span>
    <select on:change={pickUser}>
      <option value="">未登录</option>
      {#each users as u}
        <option value={u.user.id} selected={$currentUser && $currentUser.id === u.user.id}>
          {u.user.name}（{roleLabels[u.user.role]}{u.exhibitor_name ? `·${u.exhibitor_name}` : ''}）
        </option>
      {/each}
    </select>
  </div>
</header>

<main>
  {#if !$currentUser}
    <div class="msg error">请先在右上角选择操作身份（演示系统使用 X-User-Id 头认证）。</div>
  {/if}
  <svelte:component this={activePage.comp} on:detail={openDetail} />
</main>

{#if detailItemId}
  <ItemDetail itemId={detailItemId} on:close={() => (detailItemId = null)} />
{/if}
