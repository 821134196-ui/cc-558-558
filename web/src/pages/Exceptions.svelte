<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '../api.js';
  import { currentUser } from '../stores.js';

  const dispatch = createEventDispatcher();
  let list = [];
  let error = '';
  let ok = '';

  $: canDecide = $currentUser && ['staff', 'manager'].includes($currentUser.role);

  async function load() {
    error = '';
    try { list = await api('/exceptions'); }
    catch (e) { error = e.message; }
  }
  onMount(load);

  async function decide(id, approve) {
    error = ''; ok = '';
    try {
      const res = await api(`/exceptions/${id}/decide`, { method: 'POST', body: { approve } });
      ok = `异常 #${res.exception_id} 已${approve ? '批准' : '驳回'}`;
      await load();
    } catch (e) { error = e.message; }
  }
</script>

<h2 style="font-size:18px">异常处理（缺件 / 破损）</h2>
{#if $currentUser && $currentUser.role === 'exhibitor'}
  <div class="msg error">参展商可上报异常，但无权批准异常放行，须由场馆方审批。</div>
{/if}
{#if error}<div class="msg error">{error}</div>{/if}
{#if ok}<div class="msg ok">{ok}</div>{/if}

{#each list as row}
  <div class="card">
    <div class="row" style="justify-content:space-between">
      <div>
        <b>异常 #{row.exception.id}</b>
        <span class="badge {row.exception.status}">{row.exception.status}</span>
        <span class="badge">{row.exception.kind === 'missing' ? '缺件' : '破损'}</span>
      </div>
      <div class="muted">{row.exception.created_at}</div>
    </div>
    <p style="margin:8px 0">
      展品：<a href={'#/exceptions'} on:click|preventDefault={() => dispatch('detail', row.item.id)}>
        #{row.item.id} {row.item.name}
      </a>
      ｜说明：{row.exception.note || '—'}
      ｜上报人：{row.reporter}
      {#if row.decider}｜审批人：{row.decider}{/if}
    </p>
    {#if row.exception.photo_path}
      <img class="photo" src={row.exception.photo_path} alt="现场照片" />
    {/if}
    {#if row.exception.status === 'open'}
      <div class="row" style="margin-top:8px">
        <button class="ok" on:click={() => decide(row.exception.id, true)} disabled={!canDecide}>批准</button>
        <button class="danger" on:click={() => decide(row.exception.id, false)} disabled={!canDecide}>驳回</button>
        {#if !canDecide}<span class="muted">需场馆人员/主管审批</span>{/if}
      </div>
    {/if}
  </div>
{:else}
  <div class="card muted">暂无异常记录</div>
{/each}
