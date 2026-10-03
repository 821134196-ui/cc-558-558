<script>
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api, fmtTime, STATUS_LABEL, EVENT_LABEL } from '../api.js';

  export let bus;
  const notify = createEventDispatcher();

  let exhibits = [], filterEid = '', selected = null;

  async function load() {
    const q = filterEid ? `?exhibitor_id=${filterEid}` : '';
    exhibits = await api.get(`/api/exhibits${q}`);
    if (selected) await show(selected.exhibit.code);
  }
  async function show(code) {
    const q = filterEid ? `?exhibitor_id=${filterEid}` : '';
    selected = await api.get(`/api/exhibits/${code}/trace${q}`);
  }
  let unsub;
  onMount(() => { load(); unsub = bus.on(load); });
  onDestroy(() => unsub && unsub());
</script>

<div class="grid cols-2">
  <div class="card">
    <h2>展品清单与去向</h2>
    <label class="field">
      <select bind:value={filterEid} on:change={load}>
        <option value="">全部参展商</option>
        <option value="e-alpha">甲方法人公司</option>
        <option value="e-beta">乙方艺术机构</option>
      </select>
    </label>
    <table>
      <tr><th>编码</th><th>名称</th><th>状态</th><th>展位</th><th>最后经手</th></tr>
      {#each exhibits as e}
        <tr>
          <td><button type="button" class="linkbtn mono" on:click={() => show(e.code)}><b>{e.code}</b></button></td>
          <td>{e.name}</td>
          <td>
            {#if !e.active}<span class="badge b-replaced">已替换</span>
            {:else}<span class="badge b-{e.status.toLowerCase()}">{STATUS_LABEL[e.status] || e.status}</span>{/if}
          </td>
          <td>{e.booth || '—'}</td>
          <td>{e.last_handler || '—'}</td>
        </tr>
      {/each}
    </table>
  </div>

  <div class="card">
    {#if selected}
      {@const ex = selected.exhibit}
      <h2>展品追溯：{ex.code} · {ex.name}</h2>
      <p class="muted">当前状态：{STATUS_LABEL[ex.status] || ex.status}{ex.active ? '' : '（原展品，已被替换留档）'}</p>

      <h3>逐件流转记录（append-only，发现缺件时沿此链找最后经手人）</h3>
      <ul class="timeline">
        {#each selected.events as ev}
          <li>
            <span class="ty">{EVENT_LABEL[ev.type] || ev.type}</span>
            {#if ev.crate}<span class="badge b-received">箱 {ev.crate}</span>{/if}
            {#if ev.booth}<span class="badge b-placed">展位 {ev.booth}</span>{/if}
            <span class="muted"> · {ev.actor || '系统'}</span>
            {#if ev.detail}<div class="muted">{ev.detail}</div>{/if}
            <div class="at">{fmtTime(ev.at)}</div>
          </li>
        {/each}
      </ul>

      {#if selected.exceptions.length}
        <h3 class="mt10">异常记录</h3>
        <table>
          <tr><th>类型</th><th>状态</th><th>登记人</th><th>说明</th></tr>
          {#each selected.exceptions as x}
            <tr>
              <td><span class="badge b-{x.kind}">{x.kind === 'missing' ? '缺件' : '破损'}</span></td>
              <td>{x.status === 'open' ? '未处理' : x.status === 'resolved_found' ? '已找到' : '经理核销'}</td>
              <td>{x.opened_by}</td><td>{x.note}</td>
            </tr>
          {/each}
        </table>
      {/if}

      {#if selected.photos.length}
        <h3 class="mt10">现场照片</h3>
        <div class="photo-grid" style="display:flex;gap:10px;flex-wrap:wrap">
          {#each selected.photos as p}
            <a href={p.url} target="_blank"><img src={p.url} alt="现场照片" style="width:150px;height:110px;object-fit:cover" /></a>
          {/each}
        </div>
      {/if}
    {:else}
      <h2>展品追溯</h2>
      <p class="muted">点击左侧展品编码查看完整流转链、异常与现场照片。</p>
    {/if}
  </div>
</div>
