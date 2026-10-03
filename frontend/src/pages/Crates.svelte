<script>
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api, fmtTime } from '../api.js';

  export let bus;
  const notify = createEventDispatcher();

  let exhibitors = [], eid = '', crateCode = '', note = '';
  let codesText = '';
  let crates = [], exhibits = [];

  async function loadBase() {
    exhibitors = await api.get('/api/exhibitors');
    const me = await api.get('/api/me');
    if (me.exhibitor_id) eid = me.exhibitor_id;
    else if (!eid && exhibitors[0]) eid = exhibitors[0].id;
  }
  async function loadCrates() { crates = await api.get('/api/crates'); }
  async function loadExhibits() {
    exhibits = await api.get(`/api/exhibits?exhibitor_id=${eid}`);
  }

  async function receive() {
    try {
      const exhibit_codes = codesText.split(/[\s,，\n]+/).map((s) => s.trim()).filter(Boolean);
      await api.post('/api/crates', {
        code: crateCode, exhibitor_id: eid, exhibit_codes,
        note: note || null,
      });
      notify('notify', { msg: `箱 ${crateCode} 已封箱接收，关联 ${exhibit_codes.length} 件展品`, kind: 'ok' });
      crateCode = ''; codesText = ''; note = '';
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  let unsub;
  onMount(async () => {
    await loadBase();
    await Promise.all([loadCrates(), loadExhibits()]);
    unsub = bus.on(async () => { await loadCrates(); await loadExhibits(); });
  });
  onDestroy(() => unsub && unsub());
  $: if (eid) loadExhibits();
</script>

<div class="grid cols-2">
  <div class="card">
    <h2>入口按箱接收（封箱）</h2>
    <p class="muted" style="font-size:12.5px">仅入口闸口人员可接收。箱号不能重复；箱内每件必须在该参展商当前有效清单内且尚未入馆。</p>
    <label class="field">参展商
      <select bind:value={eid}>
        {#each exhibitors as e}<option value={e.id}>{e.name}</option>{/each}
      </select>
    </label>
    <label class="field">箱号（条码）
      <input bind:value={crateCode} placeholder="如 CRATE-A-01" />
    </label>
    <label class="field">箱内展品编码（空格/逗号/换行分隔）
      <textarea rows="4" bind:value={codesText} placeholder="A-001 A-002 A-003"></textarea>
    </label>
    <label class="field">备注
      <input bind:value={note} placeholder="如：木架包装，外箱完好" />
    </label>
    <button class="btn" on:click={receive}>按箱封箱接收入馆</button>
  </div>

  <div class="card">
    <h2>该参展商清单（点击编码填入箱内）</h2>
    <table>
      <tr><th>编码</th><th>名称</th><th>状态</th></tr>
      {#each exhibits as ex}
        <tr>
          <td><button type="button" class="linkbtn mono" on:click={() => (codesText += (codesText ? ' ' : '') + ex.code)}>{ex.code}</button></td>
          <td>{ex.name}</td>
          <td><span class="badge b-{ex.status.toLowerCase()}">{ex.status}</span></td>
        </tr>
      {/each}
    </table>
  </div>
</div>

<div class="card mt10">
  <h2>已接收箱单（入馆记录，不可修改）</h2>
  <table>
    <tr><th>箱号</th><th>接收人</th><th>时间</th><th>备注</th><th>箱内展品</th></tr>
    {#each crates as c}
      <tr>
        <td class="mono"><b>{c.code}</b></td>
        <td>{c.receiver}</td>
        <td>{fmtTime(c.received_at)}</td>
        <td>{c.note}</td>
        <td>{c.items.map((i) => i.code).join('、')}（{c.items.length} 件）</td>
      </tr>
    {/each}
  </table>
</div>
