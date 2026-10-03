<script>
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api } from '../api.js';

  export let bus;
  const notify = createEventDispatcher();

  let booths = [], exhibits = [];
  let exId = '', boothId = '';
  // 每件展品的转移目标选择
  let transferTarget = {};

  async function load() {
    booths = await api.get('/api/booths');
    exhibits = await api.get('/api/exhibits');
  }

  async function place() {
    if (!exId || !boothId) return notify('notify', { msg: '请先选择展品和展位' });
    try {
      await api.post('/api/placements', { exhibit_id: exId, booth_id: boothId });
      notify('notify', { msg: '布展占位成功', kind: 'ok' });
      exId = ''; boothId = '';
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  async function transfer(code) {
    const tgt = transferTarget[code];
    if (!tgt) return notify('notify', { msg: '请选择目标展位' });
    try {
      await api.post(`/api/exhibits/${code}/transfer`, { booth_id: tgt });
      notify('notify', { msg: `${code} 已转移展位`, kind: 'ok' });
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  async function retract(pid, code) {
    if (!confirm(`确认将 ${code} 从展位撤下？`)) return;
    try {
      await api.post(`/api/placements/${pid}/retract`, {});
      notify('notify', { msg: `${code} 已撤下`, kind: 'ok' });
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  const placeable = () => exhibits.filter((e) => e.active && ['RECEIVED', 'PLACED'].includes(e.status));
  const boothItems = (code) => exhibits.filter((e) => e.booth === code);

  let unsub;
  onMount(() => { load(); unsub = bus.on(load); });
  onDestroy(() => unsub && unsub());
</script>

<div class="card mb8">
  <h2>布展占位登记（仅场馆布展人员）</h2>
  <p class="muted" style="font-size:12.5px">
    同一件展品不能同时出现在两个展位：重复占位会被拒绝（409）。借用展位须先在「变更审批」完成转借审批。
  </p>
  <div class="row">
    <select style="max-width:360px" bind:value={exId}>
      <option value="">— 选择已入馆展品 —</option>
      {#each placeable() as e}
        <option value={e.id}>{e.code} · {e.name}{e.booth ? `（当前在 ${e.booth}，再占位将被拒绝）` : ''}</option>
      {/each}
    </select>
    <span>→</span>
    <select style="max-width:280px" bind:value={boothId}>
      <option value="">— 选择展位 —</option>
      {#each booths as b}
        <option value={b.id}>{b.code} {b.name}{b.borrower_exhibitor_name ? `（借予 ${b.borrower_exhibitor_name}）` : ''}</option>
      {/each}
    </select>
    <button class="btn" on:click={place}>登记占位</button>
  </div>
</div>

<div class="grid cols-3">
  {#each booths as b}
    {@const items = boothItems(b.code)}
    <div class="card booth-card">
      <h3>{b.code} · {b.name}（{items.length} 件）</h3>
      <div class="muted" style="font-size:12px;margin-bottom:6px">
        {#if b.borrower_exhibitor_name}
          已转借：{b.borrower_exhibitor_name}{b.loan_note ? `（${b.loan_note}）` : ''}
        {:else}自有展位{/if}
      </div>
      {#each items as it}
        <span class="exhibit-chip">
          <b class="mono">{it.code}</b> {it.name}
          <div class="handler">最后经手：{it.last_handler || '—'}</div>
          <div class="row mt10">
            <select style="max-width:130px" bind:value={transferTarget[it.code]}>
              <option value={null}>转移至…</option>
              {#each booths.filter((x) => x.code !== b.code) as tb}
                <option value={tb.id}>{tb.code}</option>
              {/each}
            </select>
            <button class="ghost" on:click={() => transfer(it.code)}>转移</button>
            <button class="danger" on:click={() => retract(it.placement_id, it.code)}>撤下</button>
          </div>
        </span>
      {:else}
        <span class="muted" style="font-size:12.5px">空位</span>
      {/each}
    </div>
  {/each}
</div>
