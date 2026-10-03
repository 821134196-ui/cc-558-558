<script>
  import { onMount, onDestroy } from 'svelte';
  import { api } from '../api.js';

  export let bus;
  import { createEventDispatcher } from 'svelte';
  const notify = createEventDispatcher();

  let exhibitors = [], eid = '';
  let title = '';
  let itemsText = 'A-001,青花瓷瓶\nA-002,玉雕摆件\nA-003,丝绸长卷';
  let detail = null;

  async function loadBase() {
    exhibitors = await api.get('/api/exhibitors');
    const me = await api.get('/api/me');
    if (me.exhibitor_id) eid = me.exhibitor_id;
    else if (!eid && exhibitors[0]) eid = exhibitors[0].id;
    await loadDetail();
  }
  async function loadDetail() {
    if (!eid) return;
    detail = await api.get(`/api/manifests/${eid}`);
  }
  async function submit() {
    try {
      const items = itemsText.split('\n').map((l) => l.trim()).filter(Boolean).map((l) => {
        const [code, ...rest] = l.split(/[,，]/);
        return { code: code.trim(), name: rest.join(',').trim() || code.trim() };
      });
      await api.post('/api/manifests', { exhibitor_id: eid, title: title || '参展逐件清单', items });
      notify('notify', { msg: `清单已提交，共 ${items.length} 件`, kind: 'ok' });
      bus.emit();
      await loadDetail();
    } catch (e) { notify('notify', { msg: e.message }); }
  }
  $: if (eid) loadDetail();

  let unsub;
  onMount(() => { loadBase(); unsub = bus.on(loadBase); });
  onDestroy(() => unsub && unsub());
</script>

<div class="grid cols-2">
  <div class="card">
    <h2>提前提交逐件清单</h2>
    <label class="field">参展商
      <select bind:value={eid}>
        {#each exhibitors as e}<option value={e.id}>{e.name}</option>{/each}
      </select>
    </label>
    <label class="field">清单标题
      <input bind:value={title} placeholder="参展逐件清单" />
    </label>
    <label class="field">逐件清单（每行：编码,名称）
      <textarea rows="8" bind:value={itemsText}></textarea>
    </label>
    <button class="btn" on:click={submit}>提交清单（仅参展商本人）</button>
    <p class="muted mt10" style="font-size:12.5px">
      清单提交后不可直接覆盖重报；临时替换展品请到「变更审批」发起申请，审批后生成清单新版本，原始入馆记录保留。
    </p>
  </div>

  <div class="card">
    <h2>清单版本与逐件状态</h2>
    {#if detail}
      <table>
        <tr><th>版本</th><th>标题</th><th>件数</th><th>状态</th></tr>
        {#each detail.versions as v}
          <tr>
            <td>v{v.version}</td><td>{v.title}</td><td>{v.declared_count}</td>
            <td><span class="badge {v.status === 'submitted' ? 'b-approved' : 'b-replaced'}">
              {v.status === 'submitted' ? '当前有效' : '已被替代（留档）'}</span></td>
          </tr>
        {/each}
      </table>
      <h3 class="mt10">展品明细</h3>
      <table>
        <tr><th>编码</th><th>名称</th><th>状态</th></tr>
        {#each detail.items as it}
          <tr>
            <td class="mono">{it.code}</td><td>{it.name}</td>
            <td>
              {#if !it.active}<span class="badge b-replaced">已替换失效</span>
              {:else}<span class="badge b-{it.status.toLowerCase()}">{it.status}</span>{/if}
            </td>
          </tr>
        {/each}
      </table>
    {/if}
  </div>
</div>
