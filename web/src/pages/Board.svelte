<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '../api.js';

  const dispatch = createEventDispatcher();
  let board = null;
  let error = '';

  async function load() {
    error = '';
    try { board = await api('/board'); }
    catch (e) { error = e.message; }
  }
  onMount(load);

  $: halls = board
    ? [...new Set(board.booths.map((b) => b.booth.hall))].sort()
    : [];
</script>

<div class="row" style="justify-content:space-between">
  <h2 style="margin:0;font-size:18px">展位与流转看板</h2>
  <button class="ghost" on:click={load}>刷新</button>
</div>
{#if error}<div class="msg error">{error}</div>{/if}

{#if board}
  <div class="card">
    <h3>数量核对（入馆 = 在场 + 已核实 + 已出馆 + 缺件结案）</h3>
    <table>
      <thead>
        <tr><th>参展商</th><th>清单</th><th>入馆</th><th>在场</th><th>已核实</th><th>已出馆</th><th>缺件结案</th><th>未处理异常</th><th>状态</th></tr>
      </thead>
      <tbody>
        {#each board.exhibitors as row}
          <tr>
            <td>{row.exhibitor.name}</td>
            <td>{row.reconciliation.declared}</td>
            <td>{row.reconciliation.received}</td>
            <td>{row.reconciliation.on_site}</td>
            <td>{row.reconciliation.verified_out}</td>
            <td>{row.reconciliation.exited}</td>
            <td>{row.reconciliation.missing_closed}</td>
            <td>{row.reconciliation.open_exceptions}</td>
            <td>
              {#if row.reconciliation.balanced}<span class="badge approved">相符</span>
              {:else}<span class="badge open">不符</span>{/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>

  {#each halls as hall}
    <div class="hall-title">{hall} 馆</div>
    <div class="grid cols-3">
      {#each board.booths.filter((b) => b.booth.hall === hall) as b}
        <div class="booth">
          <span class="code">{b.booth.code}</span>
          <span class="owner">{b.owner_name ? `展商：${b.owner_name}` : '公共展位'}</span>
          <div>
            {#each b.items as it}
              <span class="chip" on:click={() => dispatch('detail', it.id)}>
                #{it.id} {it.name}
                {#if it.open_exceptions > 0}<span class="ex">!</span>{/if}
                <span class="muted">{it.exhibitor_name}</span>
              </span>
            {:else}
              <div class="muted">（空）</div>
            {/each}
          </div>
        </div>
      {/each}
    </div>
  {/each}

  <div class="card">
    <h3>已入馆未占位（在库）</h3>
    {#each board.unplaced as it}
      <span class="chip" on:click={() => dispatch('detail', it.id)}>
        #{it.id} {it.name} <span class="muted">{it.exhibitor_name}｜箱 {it.box_code}</span>
      </span>
    {:else}
      <div class="muted">无</div>
    {/each}
  </div>
{/if}
