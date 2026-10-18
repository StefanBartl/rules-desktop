<script lang="ts">
  // The conflict center: a change that met another version of the file is never written over. The page shows what
  // was changed next to what the file says now, and the person chooses: write the change onto the new version (their
  // decision to overwrite the other change), look at the rule, or drop their change. Nothing is merged for them.
  import type { App } from '../lib/app.svelte';
  import { retryOp, rowsOf } from '../lib/conflicts';
  import { translate, type Key } from '../lib/i18n';
  import { shownText } from '../lib/ruletext';

  let { app }: { app: App } = $props();
  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);

  let dialog: HTMLDialogElement | undefined = $state();
  const conflict = $derived(app.conflict);
  const rows = $derived(conflict ? rowsOf(conflict.op, app.conflictRule) : []);
  const canRetry = $derived(conflict ? retryOp(conflict) !== null : false);

  $effect(() => {
    if (!dialog) return;
    if (conflict && !dialog.open) dialog.showModal();
    if (!conflict && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  aria-label={t('conflict.label')}
  oncancel={(event) => {
    event.preventDefault();
    void app.resolveConflict('drop');
  }}
>
  {#if conflict}
    <h2>{t('conflict.label')}</h2>
    <p>{t('conflict.hint', { id: shownText(conflict.id) })}</p>
    {#if app.conflicts.length > 1}<p class="muted">{t('conflict.more', { n: app.conflicts.length - 1 })}</p>{/if}
    {#if rows.length > 0}
      <table>
        <thead>
          <tr>
            <th scope="col">{t('conflict.field')}</th>
            <th scope="col">{t('conflict.mine')}</th>
            <th scope="col">{t('conflict.current')}</th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.field)}
            <tr>
              <th scope="row">{row.field}</th>
              <td><pre>{shownText(row.mine)}</pre></td>
              <td><pre>{shownText(row.current)}</pre></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
    <div class="row">
      {#if canRetry}
        <button type="button" disabled={app.writing} onclick={() => void app.resolveConflict('retry')}>{t('conflict.retry')}</button>
      {/if}
      <button type="button" onclick={() => void app.resolveConflict('view')}>{t('conflict.view')}</button>
      <button type="button" onclick={() => void app.resolveConflict('drop')}>{t('conflict.drop')}</button>
    </div>
  {/if}
</dialog>

<style>
  dialog {
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: var(--radius-lg);
    padding: 1rem 1.2rem;
    width: min(44rem, calc(100vw - 2rem));
  }
  dialog::backdrop {
    background: var(--backdrop);
  }
  h2 {
    margin: 0 0 0.4rem;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    margin: 0.5rem 0;
  }
  th,
  td {
    border-bottom: 1px solid var(--line);
    padding: 0.25rem 0.5rem;
    text-align: left;
    vertical-align: top;
  }
  pre {
    margin: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    font-family: var(--mono);
  }
  .row {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  .muted {
    color: var(--muted);
  }
  button {
    background: none;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.15rem 0.6rem;
    cursor: pointer;
    color: var(--text);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
