<script>
  import { createEventDispatcher, onMount } from 'svelte';
  import { api } from '../api.js';
  import { statusLabels, eventLabels, roleLabels } from '../stores.js';

  export let itemId;
  const dispatch = createEventDispatcher();

  let data = null;
  let error = '';
  onMount(async () => {
    try { data = await api(`/items/${itemId}`); }
    catch (e) { error = e.message; }
  });
</script>

<div class="modal-mask" on:click={() => dispatch('close')}>
  <div class="modal" on:click|stopPropagation>
    {#if error}<div class="msg error">{error}</div>{/if}
    {#if data}
      <h3 style="margin-top:0">#{data.item.id} {data.item.name}
        <span class="badge {data.item.status}">{statusLabels[data.item.status] || data.item.status}</span>
      </h3>
      <p class="muted">
        展商：{data.exhibitor.name} ｜ 序列号：{data.item.serial_no || '—'} ｜
        当前展位：{data.current_booth ? data.current_booth.code : '—'} ｜
        入馆箱号：{data.box_code || '—'}
        {#if data.item.origin === 'replacement'}｜ 临时替换入场{/if}
      </p>
      {#if data.item.description}<p>{data.item.description}</p>{/if}

      {#if data.last_handler}
        <div class="msg ok">
          最后经手人：<b>{data.last_handler.name}</b>
          （{eventLabels[data.last_handler.event_type] || data.last_handler.event_type}，{data.last_handler.at}）
        </div>
      {/if}

      {#if data.exceptions.length}
        <h4>异常记录</h4>
        {#each data.exceptions as ex}
          <div class="card" style="margin-bottom:8px">
            <span class="badge {ex.status}">{ex.status}</span>
            {ex.kind === 'missing' ? '缺件' : '破损'} — {ex.note}
            <div class="muted">{ex.created_at}</div>
            {#if ex.photo_path}<img class="photo" src={ex.photo_path} alt="现场照片" />{/if}
          </div>
        {/each}
      {/if}

      <h4>流转记录</h4>
      <ul class="timeline">
        {#each data.events as ev}
          <li>
            <b>{eventLabels[ev.event_type] || ev.event_type}</b> — {ev.actor_name}
            <span class="muted">（{roleLabels[ev.actor_role]}）</span>
            {#if ev.booth_code}<span class="badge">{ev.booth_code}</span>{/if}
            {#if ev.box_code}<span class="badge">箱 {ev.box_code}</span>{/if}
            {#if ev.pass_code}<span class="badge">{ev.pass_code}</span>{/if}
            <div>{ev.note}</div>
            <div class="t">{ev.created_at}</div>
          </li>
        {/each}
      </ul>
    {/if}
    <div style="text-align:right; margin-top:10px">
      <button class="ghost" on:click={() => dispatch('close')}>关闭</button>
    </div>
  </div>
</div>
