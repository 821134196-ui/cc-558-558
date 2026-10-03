<script>
  import { onMount, onDestroy } from 'svelte';
  import { api, fmtTime } from '../api.js';

  let data = null;
  async function load() {
    data = await api.get('/api/overview');
  }
  let unsub;
  onMount(() => { load(); unsub = bus.on(load); });
  onDestroy(() => unsub && unsub());
  export let bus;
</script>

<h2 style="margin:0 0 14px">数量核对总览</h2>

{#if data}
  <div class="grid cols-2 mb8">
    {#each data.exhibitors as e}
      <div class="card">
        <div class="row" style="justify-content:space-between">
          <h3 style="margin:0">{e.exhibitor_name}</h3>
          {#if e.balanced}
            <span class="badge b-ok">数量相符 ✓</span>
          {:else}
            <span class="badge b-bad">账实不平</span>
          {/if}
        </div>
        <div class="grid cols-4 mt10">
          <div><div class="metric">{e.declared}</div><small>申报(入馆)</small></div>
          <div><div class="metric" style="color:var(--accent)">{e.received}</div><small>实际入馆</small></div>
          <div><div class="metric" style="color:var(--violet)">{e.out}</div><small>已出馆</small></div>
          <div><div class="metric" style="color:var(--ok)">{e.placed}</div><small>在展位</small></div>
        </div>
        <div class="mt10 muted" style="font-size:12.5px">
          未处理缺件 <b style="color:var(--bad)">{e.open_missing}</b> ·
          未处理破损 <b style="color:var(--warn)">{e.open_damaged}</b> ·
          经理核销 <b>{e.written_off}</b> ·
          已出馆+核销 = {e.out + e.written_off} ／ 申报 {e.declared}
        </div>
      </div>
    {/each}
  </div>

  <h2 style="margin:18px 0 10px">展位现状（同一件展品仅会出现在一个展位）</h2>
  <div class="grid cols-3">
    {#each Object.entries(data.booths) as [code, info]}
      <div class="card booth-card">
        <h3>{code} · {info.booth}（{info.items.length} 件）</h3>
        {#each info.items as it}
          <span class="exhibit-chip">
            <b>{it.exhibit_code}</b> {it.exhibit_name}
          </span>
        {/each}
      </div>
    {/each}
    {#if Object.keys(data.booths).length === 0}
      <p class="muted">尚无展品布展占位。</p>
    {/if}
  </div>
{:else}
  <p class="muted">加载中…</p>
{/if}
