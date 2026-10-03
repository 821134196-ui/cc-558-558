<script>
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api, fmtTime, STATUS_LABEL } from '../api.js';

  export let bus;
  const notify = createEventDispatcher();

  let exhibitors = [], eid = 'e-alpha';
  let exhibits = [], results = {}, notes = {};
  let passes = [], counts = null;

  async function load() {
    exhibitors = await api.get('/api/exhibitors');
    passes = await api.get(`/api/passes?exhibitor_id=${eid}`);
    await loadExhibits();
  }
  async function loadExhibits() {
    exhibits = await api.get(`/api/exhibits?exhibitor_id=${eid}`);
    // 只核对当前有效件（替换件也须入馆）
    exhibits.forEach((e) => {
      if (results[e.code] === undefined) results[e.code] = e.status === 'OUT' ? 'out' : 'out';
    });
    counts = null;
  }

  async function verifyAll() {
    const items = exhibits
      .filter((e) => e.active && e.status !== 'OUT' && e.status !== 'REPLACED')
      .map((e) => ({ code: e.code, result: results[e.code] || 'out', note: notes[e.code] || null }));
    if (!items.length) return notify('notify', { msg: '没有需要核验的展品' });
    try {
      const r = await api.post(`/api/verify/${eid}`, { items });
      notify('notify', {
        msg: `核验完成：实物出馆 ${r.checked_out.length} 件，登记异常 ${r.exceptions.length} 件`,
        kind: 'ok',
      });
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  async function checkoutOne(code) {
    try {
      await api.post(`/api/exhibits/${code}/checkout?exhibitor_id=${eid}`, {});
      notify('notify', { msg: `${code} 实物核验相符，单件出馆`, kind: 'ok' });
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  async function issuePass() {
    try {
      const r = await api.post('/api/passes', { exhibitor_id: eid });
      notify('notify', {
        msg: `放行凭证 ${r.code} 已签发：申报/入馆/去向 = ${r.counts.declared}/${r.counts.received}/${r.counts.accounted}`,
        kind: 'ok',
      });
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  let unsub;
  onMount(() => {
    (async () => {
      exhibitors = await api.get('/api/exhibitors');
      const me = await api.get('/api/me');
      if (me.exhibitor_id) eid = me.exhibitor_id;
      await load();
    })();
    unsub = bus.on(load);
  });
  onDestroy(() => unsub && unsub());
  $: if (eid) loadExhibits();
</script>

<div class="card mb8">
  <h2>撤展逐件核实（按原清单）</h2>
  <div class="row">
    <label class="field" style="max-width:300px">参展商
      <select bind:value={eid} on:change={loadExhibits}>
        {#each exhibitors as e}<option value={e.id}>{e.name}</option>{/each}
      </select>
    </label>
    <button class="btn" on:click={verifyAll}>按下列结果批量逐件核实</button>
    <button class="ghost" style="background:var(--violet);color:#1e1b4b;border:none;font-weight:600"
      on:click={issuePass}>场馆经理签发《出馆放行凭证》</button>
  </div>
  <p class="muted" style="font-size:12.5px">
    入馆、在场、出馆数量必须相符；有未处理缺件/破损，或申报≠入馆≠（已出馆+经理核销）时，<b>放行凭证无法签发</b>。
    参展商身份点击签发将得到 403。
  </p>
</div>

<div class="grid cols-2">
  <div class="card">
    <h2>逐件核验表</h2>
    <table>
      <tr><th>编码</th><th>名称</th><th>当前</th><th>核验结果</th><th>备注/单件出馆</th></tr>
      {#each exhibits.filter((e) => e.active) as e}
        <tr>
          <td class="mono"><b>{e.code}</b></td>
          <td>{e.name}</td>
          <td><span class="badge b-{e.status.toLowerCase()}">{STATUS_LABEL[e.status] || e.status}</span></td>
          <td>
            {#if e.status === 'OUT'}<span class="muted">已出馆</span>
            {:else}
              <select bind:value={results[e.code]}>
                <option value="out">实物相符→出馆</option>
                <option value="missing">缺件</option>
                <option value="damaged">破损</option>
              </select>
            {/if}
          </td>
          <td>
            {#if e.status !== 'OUT'}
              <div class="row">
                <input style="max-width:120px" bind:value={notes[e.code]} placeholder="异常说明" />
                <button class="ghost" on:click={() => checkoutOne(e.code)}>单件出馆</button>
              </div>
            {/if}
          </td>
        </tr>
      {/each}
    </table>
  </div>

  <div class="card">
    <h2>已签发放行凭证</h2>
    {#if passes.length === 0}
      <p class="muted">尚无凭证。数量核对全部通过并由场馆经理签发后生成。</p>
    {/if}
    {#each passes as p}
      <div class="card" style="background:var(--panel-2);margin-bottom:10px">
        <div class="row" style="justify-content:space-between">
          <h3 class="mono" style="margin:0;font-size:16px;color:var(--ok)">🎫 {p.code}</h3>
          <span class="muted">{fmtTime(p.created_at)}</span>
        </div>
        <div class="grid cols-4 mt10">
          <div><div class="metric">{p.declared}</div><small>申报</small></div>
          <div><div class="metric">{p.received}</div><small>入馆</small></div>
          <div><div class="metric">{p.out}</div><small>出馆</small></div>
          <div><div class="metric">{p.missing + p.damaged}</div><small>缺/损</small></div>
        </div>
        <div class="muted mt10">签发经理：{p.approved_by} ｜ 凭此码到「闸口扫码」出馆（一次性）</div>
      </div>
    {/each}
  </div>
</div>
