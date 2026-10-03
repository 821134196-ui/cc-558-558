<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { currentUser, amendmentLabels } from '../stores.js';

  let list = [];
  let exhibitors = [];
  let booths = [];
  let myItems = [];
  let error = '';
  let ok = '';

  let kind = 'replacement';
  let exhibitorId = '';
  // replacement
  let repItemId = '';
  let repNewName = '';
  let repNewSerial = '';
  // booth_transfer
  let btBoothId = '';
  let btToExhibitor = '';
  // early_exit
  let eeSelected = new Set();

  $: isExhibitor = $currentUser && $currentUser.role === 'exhibitor';
  $: canDecide = $currentUser && ['staff', 'manager'].includes($currentUser.role);
  $: effectiveExhibitor = isExhibitor ? String($currentUser.exhibitor_id) : exhibitorId;
  $: myBooths = booths.filter((b) => String(b.owner_exhibitor_id || '') === effectiveExhibitor);

  async function load() {
    error = '';
    try {
      list = await api('/amendments');
      exhibitors = await api('/exhibitors');
      const b = await api('/booths');
      booths = b.map((x) => ({ ...x.booth }));
      if (!isExhibitor && exhibitors.length && !exhibitorId) exhibitorId = String(exhibitors[0].id);
      await loadItems();
    } catch (e) { error = e.message; }
  }
  async function loadItems() {
    if (!effectiveExhibitor) return;
    myItems = await api(`/items?exhibitor_id=${effectiveExhibitor}`);
  }
  onMount(load);

  function toggleEe(id) {
    eeSelected.has(id) ? eeSelected.delete(id) : eeSelected.add(id);
    eeSelected = eeSelected;
  }

  async function submit() {
    error = ''; ok = '';
    let payload = {};
    if (kind === 'replacement') {
      payload = { item_id: Number(repItemId), new_name: repNewName, new_serial_no: repNewSerial };
    } else if (kind === 'booth_transfer') {
      payload = { booth_id: Number(btBoothId), to_exhibitor_id: Number(btToExhibitor) };
    } else if (kind === 'early_exit') {
      payload = { item_ids: [...eeSelected] };
    }
    try {
      const body = { kind, payload };
      if (!isExhibitor) body.exhibitor_id = Number(effectiveExhibitor);
      const res = await api('/amendments', { method: 'POST', body });
      ok = `变更 #${res.amendment_id} 已提交：${res.summary}（待场馆方确认）`;
      repItemId = ''; repNewName = ''; repNewSerial = ''; btBoothId = ''; btToExhibitor = ''; eeSelected = new Set();
      await load();
    } catch (e) { error = e.message; }
  }

  async function decide(id, approve) {
    error = ''; ok = '';
    try {
      const res = await api(`/amendments/${id}/decide`, { method: 'POST', body: { approve } });
      ok = `变更 #${res.amendment_id} 已${approve ? '确认' : '驳回'}${res.effect ? '：' + res.effect : ''}`;
      await load();
    } catch (e) { error = e.message; }
  }
</script>

<h2 style="font-size:18px">变更申请（重新确认清单，不改写入馆记录）</h2>
{#if error}<div class="msg error">{error}</div>{/if}
{#if ok}<div class="msg ok">{ok}</div>{/if}

<div class="card">
  <h3>发起变更</h3>
  <div class="row">
    {#if !isExhibitor}
      <div>
        <label>参展商</label>
        <select bind:value={exhibitorId} on:change={loadItems}>
          {#each exhibitors as e}<option value={String(e.id)}>{e.name}</option>{/each}
        </select>
      </div>
    {/if}
    <div>
      <label>变更类型</label>
      <select bind:value={kind}>
        <option value="replacement">临时替换展品</option>
        <option value="booth_transfer">转借展位</option>
        <option value="early_exit">提前撤展</option>
      </select>
    </div>
  </div>

  {#if kind === 'replacement'}
    <div class="row" style="margin-top:10px">
      <div>
        <label>被替换展品</label>
        <select bind:value={repItemId}>
          <option value="">选择展品</option>
          {#each myItems as it}<option value={it.id}>#{it.id} {it.name}</option>{/each}
        </select>
      </div>
      <div><label>新展品名称</label><input bind:value={repNewName} /></div>
      <div><label>新展品序列号</label><input bind:value={repNewSerial} /></div>
    </div>
  {:else if kind === 'booth_transfer'}
    <div class="row" style="margin-top:10px">
      <div>
        <label>本展商展位</label>
        <select bind:value={btBoothId}>
          <option value="">选择展位</option>
          {#each myBooths as b}<option value={b.id}>{b.code}</option>{/each}
        </select>
      </div>
      <div>
        <label>转借给</label>
        <select bind:value={btToExhibitor}>
          <option value="">选择参展商</option>
          {#each exhibitors.filter((e) => String(e.id) !== effectiveExhibitor) as e}
            <option value={e.id}>{e.name}</option>
          {/each}
        </select>
      </div>
    </div>
  {:else}
    <div style="margin-top:10px">
      <label>提前撤展的展品</label>
      {#each myItems.filter((i) => ['received', 'placed'].includes(i.status)) as it}
        <label style="display:inline-flex; gap:4px; align-items:center; margin-right:12px">
          <input type="checkbox" checked={eeSelected.has(it.id)} on:change={() => toggleEe(it.id)} />
          #{it.id} {it.name}
        </label>
      {:else}
        <div class="muted">该展商暂无可提前撤展的展品</div>
      {/each}
    </div>
  {/if}

  <div style="margin-top:12px">
    <button on:click={submit}>提交变更申请</button>
  </div>
</div>

{#each list as row}
  <div class="card">
    <div class="row" style="justify-content:space-between">
      <div>
        <b>变更 #{row.amendment.id}</b>
        <span class="badge">{amendmentLabels[row.amendment.kind]}</span>
        <span class="badge {row.amendment.status}">{row.amendment.status}</span>
      </div>
      <div class="muted">{row.amendment.created_at}</div>
    </div>
    <p class="muted" style="margin:8px 0">
      展商：{row.exhibitor} ｜ 申请人：{row.creator}
      {#if row.decider}｜ 审批人：{row.decider}{/if}
    </p>
    <code style="font-size:12px">{row.amendment.payload}</code>
    {#if row.amendment.status === 'pending'}
      <div class="row" style="margin-top:8px">
        <button class="ok" on:click={() => decide(row.amendment.id, true)} disabled={!canDecide}>确认生效</button>
        <button class="danger" on:click={() => decide(row.amendment.id, false)} disabled={!canDecide}>驳回</button>
        {#if !canDecide}<span class="muted">需场馆人员/主管确认</span>{/if}
      </div>
    {/if}
  </div>
{:else}
  <div class="card muted">暂无变更申请</div>
{/each}
