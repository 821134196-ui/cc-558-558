<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '../api.js';
  import { currentUser, statusLabels } from '../stores.js';

  const dispatch = createEventDispatcher();
  let manifests = [];
  let exhibitors = [];
  let error = '';
  let ok = '';
  let rows = [{ name: '', serial_no: '', description: '' }];

  $: isExhibitor = $currentUser && $currentUser.role === 'exhibitor';
  $: mine = isExhibitor
    ? manifests.filter((m) => m.exhibitor.id === $currentUser.exhibitor_id)
    : manifests;

  async function load() {
    error = '';
    try {
      manifests = await api('/manifests');
      if (!isExhibitor) exhibitors = await api('/exhibitors');
    } catch (e) { error = e.message; }
  }
  onMount(load);

  function addRow() { rows = [...rows, { name: '', serial_no: '', description: '' }]; }
  function removeRow(i) { rows = rows.filter((_, j) => j !== i); }

  async function submit() {
    error = ''; ok = '';
    const items = rows.filter((r) => r.name.trim());
    try {
      const res = await api('/manifests', { method: 'POST', body: { items } });
      ok = `清单已提交（#${res.manifest_id}，共 ${res.item_ids.length} 件）`;
      rows = [{ name: '', serial_no: '', description: '' }];
      await load();
    } catch (e) { error = e.message; }
  }
</script>

<h2 style="font-size:18px">入馆清单</h2>
{#if error}<div class="msg error">{error}</div>{/if}
{#if ok}<div class="msg ok">{ok}</div>{/if}

{#if isExhibitor && mine.length === 0}
  <div class="card">
    <h3>提交逐件清单（提交后不可直接修改，变更请走「变更申请」）</h3>
    {#each rows as row, i}
      <div class="row" style="margin-bottom:8px">
        <div><label>展品名称 *</label><input bind:value={row.name} placeholder="如：全息投影仪" /></div>
        <div><label>序列号</label><input bind:value={row.serial_no} /></div>
        <div><label>备注</label><input bind:value={row.description} /></div>
        <button class="ghost" on:click={() => removeRow(i)} disabled={rows.length === 1}>删除</button>
      </div>
    {/each}
    <div class="row">
      <button class="ghost" on:click={addRow}>+ 增加一件</button>
      <button on:click={submit}>提交清单</button>
    </div>
  </div>
{/if}

{#each mine as m}
  <div class="card">
    <h3>{m.exhibitor.name} 的清单（#{m.manifest.id}，{m.manifest.created_at}）</h3>
    <table>
      <thead><tr><th>#</th><th>名称</th><th>序列号</th><th>来源</th><th>状态</th><th></th></tr></thead>
      <tbody>
        {#each m.items as it}
          <tr>
            <td>{it.id}</td>
            <td>{it.name}</td>
            <td>{it.serial_no || '—'}</td>
            <td>{it.origin === 'replacement' ? '临时替换' : '原始清单'}</td>
            <td><span class="badge {it.status}">{statusLabels[it.status]}</span></td>
            <td><button class="ghost" on:click={() => dispatch('detail', it.id)}>流转</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else}
  {#if !isExhibitor || mine.length > 0}{:else}
    <div class="card muted">暂无清单</div>
  {/if}
{/each}
