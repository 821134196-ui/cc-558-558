<script>
  import { onMount } from 'svelte';
  import { api } from '../api.js';
  import { currentUser } from '../stores.js';

  let exhibitors = [];
  let exhibitorId = '';
  let rec = null;
  let passes = [];
  let amendments = [];
  let earlyExitId = '';
  let scanCode = '';
  let scanResult = null;
  let error = '';
  let ok = '';

  $: canIssue = $currentUser && ['staff', 'manager'].includes($currentUser.role);
  $: confirmedEarlyExits = amendments.filter(
    (a) => a.amendment.kind === 'early_exit' && a.amendment.status === 'confirmed'
      && String(a.amendment.exhibitor_id) === exhibitorId
  );

  async function loadExhibitors() {
    exhibitors = await api('/exhibitors');
    if (exhibitors.length && !exhibitorId) exhibitorId = String(exhibitors[0].id);
    await load();
  }
  async function load() {
    error = '';
    try {
      passes = await api('/exit-passes');
      amendments = await api('/amendments');
      if (exhibitorId) rec = await api(`/exhibitors/${exhibitorId}/reconciliation`);
    } catch (e) { error = e.message; }
  }
  onMount(() => loadExhibitors().catch((e) => (error = e.message)));

  async function issueFull() {
    error = ''; ok = '';
    try {
      const res = await api('/exit-passes', {
        method: 'POST',
        body: { exhibitor_id: Number(exhibitorId) },
      });
      ok = `整单放行凭证已开具：${res.pass_code}（${res.item_count} 件）`;
      await load();
    } catch (e) { error = e.message; }
  }

  async function issuePartial() {
    error = ''; ok = '';
    const am = confirmedEarlyExits.find((a) => String(a.amendment.id) === earlyExitId);
    if (!am) { error = '请选择已确认的提前撤展变更'; return; }
    const payload = JSON.parse(am.amendment.payload);
    try {
      const res = await api('/exit-passes', {
        method: 'POST',
        body: {
          exhibitor_id: Number(exhibitorId),
          item_ids: payload.item_ids,
          amendment_id: am.amendment.id,
        },
      });
      ok = `部分放行凭证已开具：${res.pass_code}（${res.item_count} 件）`;
      await load();
    } catch (e) { error = e.message; }
  }

  async function scan() {
    error = ''; ok = ''; scanResult = null;
    try {
      scanResult = await api('/gate/scan', { method: 'POST', body: { pass_code: scanCode } });
      await load();
    } catch (e) { error = e.message; }
  }
</script>

<h2 style="font-size:18px">放行凭证与闸口</h2>
{#if error}<div class="msg error">{error}</div>{/if}
{#if ok}<div class="msg ok">{ok}</div>{/if}

<div class="card">
  <h3>开具放行凭证（仅场馆方）</h3>
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
        <span class="stat"><span class="n">{rec.verified_out}</span><div class="l">已核实</div></span>
        <span class="stat"><span class="n">{rec.exited}</span><div class="l">已出馆</div></span>
        <span class="stat"><span class="n">{rec.missing_closed}</span><div class="l">缺件结案</div></span>
        {#if rec.balanced}<span class="badge approved">数量相符</span>{:else}<span class="badge open">数量不符</span>{/if}
      </div>
    {/if}
  </div>
  <div class="row" style="margin-top:10px">
    <button on:click={issueFull} disabled={!canIssue}>开具整单放行凭证</button>
    <div>
      <label>提前撤展部分放行</label>
      <select bind:value={earlyExitId}>
        <option value="">选择已确认变更</option>
        {#each confirmedEarlyExits as a}
          <option value={String(a.amendment.id)}>变更 #{a.amendment.id}（{JSON.parse(a.amendment.payload).item_ids.length} 件）</option>
        {/each}
      </select>
    </div>
    <button class="ghost" on:click={issuePartial} disabled={!canIssue || !earlyExitId}>开具部分放行凭证</button>
    {#if !canIssue}<span class="muted">参展商无权自行开具放行凭证</span>{/if}
  </div>
</div>

<div class="card">
  <h3>闸口扫码（模拟）</h3>
  <div class="row">
    <div><label>凭证号</label><input bind:value={scanCode} placeholder="EP-XXXXXXXX" /></div>
    <button on:click={scan} disabled={!scanCode.trim()}>扫码核销</button>
  </div>
  {#if scanResult}
    <div class="msg ok">
      {scanResult.result}：凭证 {scanResult.pass_code}，出馆 {scanResult.exited_count} 件
      {#if scanResult.missing_accounted > 0}，缺件核销 {scanResult.missing_accounted} 件{/if}
    </div>
  {/if}
</div>

<div class="card">
  <h3>放行凭证记录</h3>
  <table>
    <thead><tr><th>凭证号</th><th>展商</th><th>类型</th><th>件数</th><th>开具人</th><th>状态</th><th>时间</th></tr></thead>
    <tbody>
      {#each passes as p}
        <tr>
          <td><code>{p.pass.pass_code}</code></td>
          <td>{p.exhibitor}</td>
          <td>{p.pass.kind === 'full' ? '整单' : '部分'}</td>
          <td>{p.item_count}</td>
          <td>{p.issuer}</td>
          <td><span class="badge {p.pass.status}">{p.pass.status === 'issued' ? '已开具' : p.pass.status === 'used' ? '已核销' : p.pass.status}</span></td>
          <td class="muted">{p.pass.created_at}{p.pass.used_at ? ` / 核销 ${p.pass.used_at}` : ''}</td>
        </tr>
      {:else}
        <tr><td colspan="7" class="muted">暂无放行凭证</td></tr>
      {/each}
    </tbody>
  </table>
</div>
