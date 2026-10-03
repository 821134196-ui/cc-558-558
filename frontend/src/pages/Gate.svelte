<script>
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api, fmtTime } from '../api.js';

  export let bus;
  const notify = createEventDispatcher();

  let code = '', last = null, scans = [];

  async function scan() {
    if (!code.trim()) return;
    try {
      last = await api.post('/api/gate/scan', { code: code.trim() });
      if (last.result === 'allow') {
        notify('notify', { msg: `闸口放行 ✓ ${last.detail.message || ''}`, kind: 'ok' });
      } else {
        notify('notify', { msg: `闸口拒绝 ✗ ${last.detail.reason || ''}` });
      }
      code = '';
      await loadScans();
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  async function loadScans() {
    scans = await api.get('/api/gate/scans');
  }

  let unsub;
  onMount(() => { loadScans(); unsub = bus.on(loadScans); });
  onDestroy(() => unsub && unsub());
</script>

<div class="grid cols-2">
  <div class="card">
    <h2>闸口扫码（模拟）</h2>
    <p class="muted" style="font-size:12.5px">
      可扫：箱号（入馆）、展品编码（单件核验）、放行凭证码（撤展出馆，一次性有效，重复使用拒绝）。
      仅入口闸口身份可操作。
    </p>
    <div class="row">
      <input bind:value={code} placeholder="扫描 / 输入条码…" on:keydown={(e) => e.key === 'Enter' && scan()}
             style="flex:1;font-family:monospace" />
      <button class="btn" on:click={scan}>扫码核验</button>
    </div>

    {#if last}
      <div class="card mt10" style="background:var(--panel-2)">
        {#if last.result === 'allow'}
          <h3 class="scan-allow">✅ ALLOW 放行</h3>
        {:else}
          <h3 class="scan-deny">⛔ DENY 拒绝</h3>
        {/if}
        <pre class="mono muted" style="white-space:pre-wrap;font-size:12.5px">{JSON.stringify(last.detail, null, 2)}</pre>
      </div>
    {/if}
  </div>

  <div class="card">
    <h2>扫码记录（最近 100 条）</h2>
    <table>
      <tr><th>时间</th><th>条码</th><th>结果</th><th>闸口员</th><th>说明</th></tr>
      {#each scans as s}
        <tr>
          <td style="white-space:nowrap">{fmtTime(s.at)}</td>
          <td class="mono">{s.code}</td>
          <td><span class="badge {s.result === 'allow' ? 'b-ok' : 'b-bad'}">
            {s.result === 'allow' ? '放行' : '拒绝'}</span></td>
          <td>{s.scanner}</td>
          <td class="muted" style="font-size:12px">{s.detail.message || s.detail.reason || s.detail.kind || ''}</td>
        </tr>
      {/each}
    </table>
  </div>
</div>
