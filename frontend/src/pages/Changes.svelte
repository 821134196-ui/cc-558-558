<script>
  import { onMount, onDestroy } from 'svelte';
  import { createEventDispatcher } from 'svelte';
  import { api, fmtTime } from '../api.js';

  export let bus;
  const notify = createEventDispatcher();

  let changes = [], exhibitors = [], booths = [];
  let kind = 'replace_exhibit';
  let eid = '', oldCode = '', newCode = '', newName = '';
  let loanBooth = '', borrower = '', loanNote = '';
  let earlyCodes = '';

  const KIND_LABEL = {
    replace_exhibit: '临时替换展品',
    loan_booth: '转借展位',
    early_out: '提前撤展',
  };

  async function load() {
    changes = await api.get('/api/changes');
    exhibitors = await api.get('/api/exhibitors');
    booths = await api.get('/api/booths');
    const me = await api.get('/api/me');
    if (me.exhibitor_id) eid = me.exhibitor_id;
    else if (!eid && exhibitors[0]) eid = exhibitors[0].id;
  }

  async function submit() {
    try {
      let payload;
      if (kind === 'replace_exhibit') {
        payload = { old_exhibit_code: oldCode, new_code: newCode, new_name: newName || newCode };
      } else if (kind === 'loan_booth') {
        payload = { booth_id: loanBooth, borrower_exhibitor_id: borrower, note: loanNote || null };
      } else {
        payload = { exhibit_codes: earlyCodes.split(/[\s,，\n]+/).filter(Boolean) };
      }
      await api.post('/api/changes', { kind, exhibitor_id: eid, payload });
      notify('notify', { msg: '变更申请已提交，等待场馆经理审批；入馆原始记录不会被修改', kind: 'ok' });
      oldCode = newCode = newName = earlyCodes = loanNote = '';
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  async function review(id, approve) {
    try {
      await api.post(`/api/changes/${id}/review`, { approve, note: null });
      notify('notify', { msg: approve ? '变更已批准并生效（原记录保留）' : '变更已驳回', kind: 'ok' });
      bus.emit();
    } catch (e) { notify('notify', { msg: e.message }); }
  }

  let unsub;
  onMount(() => { load(); unsub = bus.on(load); });
  onDestroy(() => unsub && unsub());
</script>

<div class="grid cols-2">
  <div class="card">
    <h2>发起变更申请（仅参展商）</h2>
    <label class="field">参展商
      <select bind:value={eid}>
        {#each exhibitors as e}<option value={e.id}>{e.name}</option>{/each}
      </select>
    </label>
    <label class="field">变更类型
      <select bind:value={kind}>
        <option value="replace_exhibit">临时替换展品</option>
        <option value="loan_booth">转借展位</option>
        <option value="early_out">提前撤展</option>
      </select>
    </label>

    {#if kind === 'replace_exhibit'}
      <label class="field">被替换展品编码（原入馆记录保留留痕）
        <input bind:value={oldCode} placeholder="如 A-002" />
      </label>
      <label class="field">新展品编码（替换后需重新按箱入馆核对）
        <input bind:value={newCode} placeholder="如 A-002-X" />
      </label>
      <label class="field">新展品名称
        <input bind:value={newName} />
      </label>
    {:else if kind === 'loan_booth'}
      <label class="field">要转借的展位
        <select bind:value={loanBooth}>
          <option value="">— 选择展位 —</option>
          {#each booths as b}<option value={b.id}>{b.code} {b.name}</option>{/each}
        </select>
      </label>
      <label class="field">借用给哪家参展商
        <select bind:value={borrower}>
          <option value="">— 选择借用方 —</option>
          {#each exhibitors as e}<option value={e.id}>{e.name}</option>{/each}
        </select>
      </label>
      <label class="field">转借说明
        <input bind:value={loanNote} placeholder="如：联合展出 3 天" />
      </label>
    {:else}
      <label class="field">提前撤展展品编码
        <textarea rows="3" bind:value={earlyCodes} placeholder="A-003"></textarea>
      </label>
    {/if}
    <button class="btn" on:click={submit}>提交申请</button>
  </div>

  <div class="card">
    <h2>审批队列（仅场馆经理可批）</h2>
    <table>
      <tr><th>类型</th><th>明细</th><th>申请人</th><th>状态</th><th>操作</th></tr>
      {#each changes as c}
        <tr>
          <td>{KIND_LABEL[c.kind]}</td>
          <td style="font-size:12px" class="mono">{JSON.stringify(c.payload)}</td>
          <td>{c.requested_by}</td>
          <td>
            {#if c.status === 'pending'}<span class="badge b-pending">待审批</span>
            {:else if c.status === 'approved'}<span class="badge b-approved">已批准</span>
            {:else}<span class="badge b-rejected">已驳回</span>{/if}
          </td>
          <td>
            {#if c.status === 'pending'}
              <div class="row">
                <button class="ghost" on:click={() => review(c.id, true)}>批准</button>
                <button class="danger" on:click={() => review(c.id, false)}>驳回</button>
              </div>
            {:else}—{/if}
          </td>
        </tr>
      {/each}
    </table>
  </div>
</div>
