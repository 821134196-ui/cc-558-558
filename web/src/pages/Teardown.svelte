<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '../api.js';
  import { currentUser, statusLabels } from '../stores.js';

  const dispatch = createEventDispatcher();
  let exhibitors = [];
  let exhibitorId = '';
  let items = [];
  let rec = null;
  let error = '';
  let ok = '';

  // 异常上报表单
  let reportItem = null;
  let reportKind = 'missing';
  let reportNote = '';
  let reportPhoto = null;

  $: canVerify = $currentUser && ['staff', 'manager'].includes($currentUser.role);

  async function loadExhibitors() {
    exhibitors = await api('/exhibitors');
    if (exhibitors.length && !exhibitorId) exhibitorId = String(exhibitors[0].id);
    await load();
  }
  async function load() {
    error = '';
    if (!exhibitorId) return;
    try {
      const all = await api(`/items?exhibitor_id=${exhibitorId}`);
      items = all.filter((i) => ['received', 'placed'].includes(i.status));
      rec = await api(`/exhibitors/${exhibitorId}/reconciliation`);
    } catch (e) { error = e.message; }
  }
  onMount(() => loadExhibitors().catch((e) => (error = e.message)));

  async function verify(itemId) {
    error = ''; ok = '';
    try {
      await api('/teardown/verify', { method: 'POST', body: { item_id: itemId } });
      ok = `展品 #${itemId} 核实无误，已标记出馆`;
      await load();
    } catch (e) { error = e.message; }
  }

  function openReport(item, kind) {
    reportItem = item;
    reportKind = kind;
    reportNote = '';
    reportPhoto = null;
  }

  async function submitReport() {
    error = ''; ok = '';
    if (!reportPhoto) { error = '缺件/破损必须上传现场照片'; return; }
    const form = new FormData();
    form.append('item_id', String(reportItem.id));
    form.append('kind', reportKind);
    form.append('note', reportNote);
    form.append('photo', reportPhoto);
    try {
      const res = await api('/exceptions', { method: 'POST', form });
      ok = `异常 #${res.exception_id} 已上报，等待场馆方审批`;
      reportItem = null;
      await load();
    } catch (e) { error = e.message; }
  }
</script>

<h2 style="font-size:18px">撤展逐件核实</h2>
{#if error}<div class="msg error">{error}</div>{/if}
{#if ok}<div class="msg ok">{ok}</div>{/if}

<div class="card">
  <div class="row">
    <div>
      <label>参展商</label>
      <select bind:value={exhibitorId} on:change={load}>
        {#each exhibitors as e}<option value={String(e.id)}>{e.name}</option>{/each}
      </select>
    </div>
    {#if rec}
      <div>
        <span class="stat"><span class="n">{rec.received}</span><div class="l">入馆</div></span>
        <span class="stat"><span class="n">{rec.on_site}</span><div class="l">在场</div></span>
        <span class="stat"><span class="n">{rec.verified_out + rec.exited + rec.missing_closed}</span><div class="l">已核销</div></span>
        {#if rec.balanced}<span class="badge approved">数量相符</span>{:else}<span class="badge open">数量不符</span>{/if}
      </div>
    {/if}
  </div>
</div>

<div class="card">
  <h3>待核实展品（按原清单逐件核对实物）</h3>
  <table>
    <thead><tr><th>#</th><th>展品</th><th>展位</th><th>状态</th><th>操作</th></tr></thead>
    <tbody>
      {#each items as it}
        <tr>
          <td>{it.id}</td>
          <td><a href={'#/teardown'} on:click|preventDefault={() => dispatch('detail', it.id)}>{it.name}</a></td>
          <td>{it.booth_code || '（在库）'}</td>
          <td><span class="badge {it.status}">{statusLabels[it.status]}</span></td>
          <td class="row">
            <button class="ok" on:click={() => verify(it.id)} disabled={!canVerify}>核实无误</button>
            <button class="ghost" on:click={() => openReport(it, 'damaged')}>破损</button>
            <button class="danger" on:click={() => openReport(it, 'missing')}>缺件</button>
          </td>
        </tr>
      {:else}
        <tr><td colspan="5" class="muted">该展商没有待核实的展品</td></tr>
      {/each}
    </tbody>
  </table>
</div>

{#if reportItem}
  <div class="modal-mask" on:click={() => (reportItem = null)}>
    <div class="modal" on:click|stopPropagation>
      <h3 style="margin-top:0">上报{reportKind === 'missing' ? '缺件' : '破损'}异常 — #{reportItem.id} {reportItem.name}</h3>
      <div class="row">
        <div>
          <label>类型</label>
          <select bind:value={reportKind}>
            <option value="missing">缺件</option>
            <option value="damaged">破损</option>
          </select>
        </div>
        <div style="flex:1">
          <label>情况说明</label>
          <input bind:value={reportNote} style="width:100%" placeholder="现场情况描述" />
        </div>
      </div>
      <div style="margin-top:10px">
        <label>现场照片（必传）</label>
        <input type="file" accept="image/*" on:change={(e) => (reportPhoto = e.target.files[0])} />
      </div>
      <div style="text-align:right; margin-top:12px">
        <button class="ghost" on:click={() => (reportItem = null)}>取消</button>
        <button class="danger" on:click={submitReport}>提交异常</button>
      </div>
    </div>
  </div>
{/if}
