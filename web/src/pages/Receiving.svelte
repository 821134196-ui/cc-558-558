<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { currentUser, statusLabels } from '../stores.js';

  let exhibitors = [];
  let exhibitorId = '';
  let declared = [];
  let selected = new Set();
  let boxCode = '';
  let boxes = [];
  let error = '';
  let ok = '';

  $: canReceive = $currentUser && ['receiver', 'staff', 'manager'].includes($currentUser.role);

  async function loadExhibitors() {
    exhibitors = await api('/exhibitors');
    if (exhibitors.length && !exhibitorId) exhibitorId = String(exhibitors[0].id);
    await loadItems();
  }
  async function loadItems() {
    error = '';
    if (!exhibitorId) return;
    try {
      declared = await api(`/items?exhibitor_id=${exhibitorId}&status=declared`);
      boxes = await api(`/boxes?exhibitor_id=${exhibitorId}`);
      selected = new Set();
    } catch (e) { error = e.message; }
  }
  onMount(() => loadExhibitors().catch((e) => (error = e.message)));

  function toggle(id) {
    selected.has(id) ? selected.delete(id) : selected.add(id);
    selected = selected;
  }

  async function receive() {
    error = ''; ok = '';
    try {
      const res = await api('/boxes', {
        method: 'POST',
        body: { box_code: boxCode, exhibitor_id: Number(exhibitorId), item_ids: [...selected] },
      });
      ok = `箱 #${res.box_id} 已封箱入馆，共 ${res.received_count} 件`;
      boxCode = '';
      await loadItems();
    } catch (e) { error = e.message; }
  }
</script>

<h2 style="font-size:18px">入口按箱接收</h2>
{#if !canReceive}<div class="msg error">当前身份无收箱权限（需入口接收员/场馆人员）。</div>{/if}
{#if error}<div class="msg error">{error}</div>{/if}
{#if ok}<div class="msg ok">{ok}</div>{/if}

<div class="card">
  <div class="row">
    <div>
      <label>参展商</label>
      <select bind:value={exhibitorId} on:change={loadItems}>
        {#each exhibitors as e}<option value={String(e.id)}>{e.name}</option>{/each}
      </select>
    </div>
    <div>
      <label>箱号</label>
      <input bind:value={boxCode} placeholder="如 BOX-001" />
    </div>
    <button on:click={receive} disabled={!canReceive || !boxCode.trim() || selected.size === 0}>
      封箱入馆（{selected.size} 件）
    </button>
  </div>
</div>

<div class="card">
  <h3>待入馆展品（清单登记状态）</h3>
  <table>
    <thead><tr><th></th><th>#</th><th>名称</th><th>序列号</th><th>来源</th></tr></thead>
    <tbody>
      {#each declared as it}
        <tr>
          <td><input type="checkbox" checked={selected.has(it.id)} on:change={() => toggle(it.id)} /></td>
          <td>{it.id}</td><td>{it.name}</td><td>{it.serial_no || '—'}</td>
          <td>{it.origin === 'replacement' ? '临时替换' : '原始清单'}</td>
        </tr>
      {:else}
        <tr><td colspan="5" class="muted">该展商没有待入馆的展品</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<div class="card">
  <h3>已接收箱记录</h3>
  {#each boxes as b}
    <div style="margin-bottom:10px">
      <b>箱 {b.box.box_code}</b>
      <span class="muted">｜{b.box.received_at}｜经手：{b.receiver}｜{b.items.length} 件</span>
      <div>
        {#each b.items as it}
          <span class="chip">#{it.id} {it.name} <span class="badge {it.status}">{statusLabels[it.status]}</span></span>
        {/each}
      </div>
    </div>
  {:else}
    <div class="muted">暂无收箱记录</div>
  {/each}
</div>
