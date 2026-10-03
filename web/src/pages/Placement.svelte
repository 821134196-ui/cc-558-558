<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '../api.js';
  import { currentUser } from '../stores.js';

  const dispatch = createEventDispatcher();
  let received = [];
  let placed = [];
  let booths = [];
  let boothByItem = {};
  let transferByItem = {};
  let noteByItem = {};
  let error = '';
  let ok = '';

  $: canPlace = $currentUser && ['staff', 'manager'].includes($currentUser.role);

  async function load() {
    error = '';
    try {
      received = await api('/items?status=received');
      placed = await api('/items?status=placed');
      const b = await api('/booths');
      booths = b.map((x) => ({ ...x.booth, owner_name: x.owner_name }));
    } catch (e) { error = e.message; }
  }
  onMount(load);

  async function place(itemId) {
    error = ''; ok = '';
    try {
      const boothId = Number(boothByItem[itemId]);
      const res = await api('/placements', { method: 'POST', body: { item_id: itemId, booth_id: boothId } });
      ok = `展品 #${res.item_id} 已登记到展位 ${res.booth}`;
      await load();
    } catch (e) { error = e.message; }
  }

  async function transfer(itemId) {
    error = ''; ok = '';
    try {
      const res = await api('/transfers', {
        method: 'POST',
        body: { item_id: itemId, to_booth_id: Number(transferByItem[itemId]), note: noteByItem[itemId] || '' },
      });
      ok = `展品 #${res.item_id} 已从 ${res.from} 转至 ${res.to}`;
      await load();
    } catch (e) { error = e.message; }
  }
</script>

<h2 style="font-size:18px">展位登记与转移</h2>
{#if !canPlace}<div class="msg error">当前身份无占位/转移权限（需场馆人员）。</div>{/if}
{#if error}<div class="msg error">{error}</div>{/if}
{#if ok}<div class="msg ok">{ok}</div>{/if}

<div class="card">
  <h3>待占位（已入馆在库）</h3>
  <table>
    <thead><tr><th>#</th><th>展品</th><th>展商</th><th>箱号</th><th>选择展位</th><th></th></tr></thead>
    <tbody>
      {#each received as it}
        <tr>
          <td>{it.id}</td>
          <td><a href={'#/placement'} on:click|preventDefault={() => dispatch('detail', it.id)}>{it.name}</a></td>
          <td>{it.exhibitor_name}</td>
          <td>{it.box_code || '—'}</td>
          <td>
            <select bind:value={boothByItem[it.id]}>
              <option value="">选择展位</option>
              {#each booths as b}
                <option value={b.id}>{b.code}（{b.owner_name || '公共'}）</option>
              {/each}
            </select>
          </td>
          <td><button on:click={() => place(it.id)} disabled={!canPlace || !boothByItem[it.id]}>登记占位</button></td>
        </tr>
      {:else}
        <tr><td colspan="6" class="muted">无待占位展品</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<div class="card">
  <h3>在展位（可转移）</h3>
  <table>
    <thead><tr><th>#</th><th>展品</th><th>展商</th><th>当前展位</th><th>转移至</th><th>备注</th><th></th></tr></thead>
    <tbody>
      {#each placed as it}
        <tr>
          <td>{it.id}</td>
          <td>
            <a href={'#/placement'} on:click|preventDefault={() => dispatch('detail', it.id)}>{it.name}</a>
            {#if it.open_exceptions > 0}<span class="badge open">异常</span>{/if}
          </td>
          <td>{it.exhibitor_name}</td>
          <td><span class="badge placed">{it.booth_code}</span></td>
          <td>
            <select bind:value={transferByItem[it.id]}>
              <option value="">选择展位</option>
              {#each booths.filter((b) => b.code !== it.booth_code) as b}
                <option value={b.id}>{b.code}（{b.owner_name || '公共'}）</option>
              {/each}
            </select>
          </td>
          <td><input bind:value={noteByItem[it.id]} placeholder="转移原因" /></td>
          <td><button on:click={() => transfer(it.id)} disabled={!canPlace || !transferByItem[it.id]}>转移</button></td>
        </tr>
      {:else}
        <tr><td colspan="7" class="muted">暂无在展位展品</td></tr>
      {/each}
    </tbody>
  </table>
</div>
