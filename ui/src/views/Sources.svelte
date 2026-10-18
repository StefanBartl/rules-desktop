<script lang="ts">
  // The data classes of this device: which rule set may be shown as what. The classes decide what may leave the hub, so
  // a change that RELEASES MORE is asked first (the hub says when a change does: it knows the policy in force). The
  // hub writes sources.json itself, in the config folder; the vault is not touched. A rule set with no class is
  // shown, not released and not written.
  import type { ClassChange, DataClass } from '../lib/api';
  import type { App } from '../lib/app.svelte';
  import { translate, type Key } from '../lib/i18n';
  import { shownText } from '../lib/ruletext';
  import { classLabel } from '../lib/sources';

  let { app }: { app: App } = $props();
  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);

  type Class = Exclude<DataClass, 'unclassified'>;
  type ClassChangeTarget = { key: string } | { default: true };
  const CLASSES: readonly Class[] = ['public', 'internal', 'private'];
  const doc = $derived(app.sources);
  let newKey = $state('');
  let newClass = $state<Class>('private');

  /** The first part of the sources of the files: a start for a key. */
  const startsOf = $derived([...new Set((doc?.files ?? []).map((f) => f.path.split('/')[0] ?? ''))].filter((s) => s !== ''));

  /** What a select says: a class, or none. */
  function pick(select: HTMLSelectElement): Class | null {
    return select.value === '' ? null : (select.value as Class);
  }

  /**
   * A select shows the class in force. When a change was not written (the person is asked first, or it failed) the
   * select goes back to it, so that it never shows a class that does not hold.
   */
  async function change(select: HTMLSelectElement, target: ClassChangeTarget, inForce: string): Promise<void> {
    const written = await app.setClass({ ...target, class: pick(select) });
    if (!written) select.value = inForce;
  }

  async function add(): Promise<void> {
    const key = newKey.trim();
    if (key === '') return;
    if (await app.setClass({ key, class: newClass })) newKey = '';
  }

  function describe(change: ClassChange | null): string {
    if (!change) return '';
    return 'key' in change ? change.key : t('classes.default');
  }
</script>

<section aria-labelledby="src-h">
  <h2 id="src-h">{t('nav.sources')}</h2>
  <p class="muted">{t('classes.hint')}</p>

  {#if app.classConfirm}
    <div role="alertdialog" aria-label={describe(app.classConfirm)} class="confirm">
      <p><strong>{shownText(describe(app.classConfirm))}</strong>: {app.classConfirm.class ? t(`class.${app.classConfirm.class}`) : t('classes.none')}</p>
      <p>{t('classes.confirm')}</p>
      <div class="row">
        <button type="button" disabled={app.writing} onclick={() => void app.confirmClass()}>{t('classes.confirm_yes')}</button>
        <button type="button" onclick={() => app.cancelClass()}>{t('classes.confirm_no')}</button>
      </div>
    </div>
  {/if}

  {#if doc}
    <h3>{t('classes.keys')}</h3>
    <table aria-label={t('classes.keys')}>
      <thead>
        <tr>
          <th scope="col">{t('classes.key')}</th>
          <th scope="col">{t('classes.class')}</th>
          <th scope="col">{t('classes.files')}</th>
          <th scope="col"><span class="sr">{t('waiver.actions')}</span></th>
        </tr>
      </thead>
      <tbody>
        {#each doc.keys as entry (entry.key)}
          <tr>
            <th scope="row"><code>{shownText(entry.key)}</code></th>
            <td>
              <select aria-label={`${entry.key}: ${t('classes.class')}`} value={entry.class} onchange={(e) => void change(e.currentTarget, { key: entry.key }, entry.class)}>
                {#each CLASSES as value (value)}<option {value}>{t(classLabel(value))}</option>{/each}
              </select>
            </td>
            <td>{entry.files}</td>
            <td><button type="button" onclick={() => void app.setClass({ key: entry.key, class: null })}>{t('classes.remove')}</button></td>
          </tr>
        {/each}
        <tr>
          <th scope="row">{t('classes.default')}</th>
          <td>
            <select
              aria-label={`${t('classes.default')}: ${t('classes.class')}`}
              value={doc.default ?? ''}
              onchange={(e) => void change(e.currentTarget, { default: true }, doc.default ?? '')}
            >
              <option value="">{t('classes.none')}</option>
              {#each CLASSES as value (value)}<option {value}>{t(classLabel(value))}</option>{/each}
            </select>
          </td>
          <td></td>
          <td></td>
        </tr>
      </tbody>
    </table>

    <h3>{t('classes.add')}</h3>
    <form
      class="add"
      onsubmit={(event) => {
        event.preventDefault();
        void add();
      }}
    >
      <label>
        {t('classes.add_key')}
        <input type="text" list="source-starts" bind:value={newKey} autocomplete="off" />
        <datalist id="source-starts">
          {#each startsOf as start (start)}<option value={start}></option>{/each}
        </datalist>
      </label>
      <label>
        {t('classes.class')}
        <select bind:value={newClass}>
          {#each CLASSES as value (value)}<option {value}>{t(classLabel(value))}</option>{/each}
        </select>
      </label>
      <div><button type="submit" disabled={app.writing || newKey.trim() === ''}>{t('classes.add')}</button></div>
    </form>

    <h3>{t('classes.unclassified')}</h3>
    {#if doc.unclassified.length === 0}
      <p class="muted">{t('classes.no_files')}</p>
    {:else}
      <ul class="plain">
        {#each doc.unclassified as path (path)}<li><code>{shownText(path)}</code></li>{/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .muted {
    color: var(--muted);
  }
  .confirm {
    border: 1px solid var(--warn);
    border-radius: var(--radius);
    padding: 0.5rem 0.8rem;
    margin: 0.5rem 0;
    background: var(--surface);
  }
  .confirm p {
    margin: 0.2rem 0;
  }
  .row {
    display: flex;
    gap: 0.4rem;
    margin-top: 0.4rem;
  }
  table {
    border-collapse: collapse;
    width: 100%;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  th,
  td {
    padding: 0.3rem 0.6rem;
    border-bottom: 1px solid var(--line);
    text-align: left;
  }
  thead th {
    color: var(--muted);
    font-size: 0.8rem;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  form.add {
    display: grid;
    gap: 0.5rem;
    max-width: 30rem;
  }
  label {
    display: grid;
    gap: 0.15rem;
    font-size: 0.85rem;
    color: var(--muted);
  }
  input[type='text'],
  select {
    font: inherit;
    color: var(--text);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.25rem 0.4rem;
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
  ul.plain {
    list-style: none;
    padding: 0;
  }
</style>
