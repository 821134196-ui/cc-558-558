<script>
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api, fmtTime } from '../api.js';

  export let bus;
  const notify = createEventDispatcher();

  let exceptions = [], exhibits = [];
  let exCode = '', kind = 'missing', note = '', file = null;

  async function load() {
    exceptions = await api.get('/api/exceptions');
    exhibits = await api.get('/api/exhibits');
  }

  async function openOne() {
    if (!exCode) return notify('notify', { msg: '请填写展品编码' });
    try {
      await api.post(`/api/exhibits/${exCode}/exception`, { kind, note: note || null });
      notify('notify', { msg: '异常已登记，可继续上传现场照片', kind: 'ok' });
      note = '';
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  async function uploadPhoto() {
    if (!exCode || !file) return notify('notify', { msg: '请填写展品编码并选择照片' });
    const fd = new FormData();
    fd.append('photo', file);
    try {
      const r = await api.upload(`/api/exhibits/${exCode}/photo`, fd);
      notify('notify', { msg: `现场照片已保存到本地（${r.photos.length} 张）`, kind: 'ok' });
      file = null;
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  async function resolve(id, status) {
    const label = status === 'resolved_found' ? '确认已找到' : '经理核销（缺件平账）';
    if (!confirm(`以场馆经理身份执行：${label}？参展商无权自行批准。`)) return;
    try {
      await api.post(`/api/exceptions/${id}/resolve`, { status, note: null });
      notify('notify', { msg: `异常已处理：${label}`, kind: 'ok' });
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  const exOptions = () => exhibits.filter((e) => e.active && e.status !== 'OUT');
  let unsub;
  onMount(() => { load(); unsub = bus.on(load); });
  onDestroy(() => unsub && unsub());
</script>

<div class="card mb8">
  <h2>登记缺件 / 破损并保存现场照片</h2>
  <div class="row" style="align-items:flex-end">
    <label class="field" style="flex:1;min-width:220px">展品编码
      <select bind:value={exCode}>
        <option value="">— 选择展品 —</option>
        {#each exOptions() as e}<option value={e.code}>{e.code} · {e.name}</option>{/each}
      </select>
    </label>
    <label class="field">异常类型
      <select bind:value={kind}>
        <option value="missing">缺件</option>
        <option value="damaged">破损</option>
      </select>
    </label>
    <label class="field" style="flex:1;min-width:220px">现场说明
      <input bind:value={note} placeholder="如：开箱发现外箱破损、内件缺失" />
    </label>
    <button class="btn" on:click={openOne}>登记异常</button>
  </div>
  <div class="row mt10">
    <input type="file" accept="image/*" on:change={(e) => (file = e.target.files[0])} style="width:auto" />
    <button class="ghost" on:click={uploadPhoto}>上传现场照片（存本地 uploads/）</button>
  </div>
  <p class="muted mt10" style="font-size:12.5px">
    异常未处理期间，该展品不能出馆；放行/核销只能由<b>场馆经理</b>操作，参展商自行申请会被拒绝（403）。
  </p>
</div>

<div class="card">
  <h2>异常台账</h2>
  <table>
    <tr><th>展品</th><th>类型</th><th>状态</th><th>说明</th><th>登记人</th><th>时间</th><th>处理（经理）</th></tr>
    {#each exceptions as x}
      <tr>
        <td><b class="mono">{x.exhibit_code}</b><br /><span class="muted">{x.exhibit_name}</span></td>
        <td><span class="badge b-{x.kind}">{x.kind === 'missing' ? '缺件' : '破损'}</span></td>
        <td>
          {#if x.status === 'open'}<span class="badge b-pending">未处理</span>
          {:else if x.status === 'resolved_found'}<span class="badge b-ok">已找到</span>
          {:else}<span class="badge b-approved">经理核销</span>{/if}
        </td>
        <td>{x.note}</td>
        <td>{x.opened_by}</td>
        <td>{fmtTime(x.created_at)}</td>
        <td>
          {#if x.status === 'open'}
            <div class="row">
              <button class="ghost" on:click={() => resolve(x.id, 'resolved_found')}>已找到</button>
              <button class="danger" on:click={() => resolve(x.id, 'written_off')}>核销平账</button>
            </div>
          {:else}—{/if}
        </td>
      </tr>
    {/each}
  </table>
</div>
