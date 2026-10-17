<script lang="ts">
  // The command palette (Ctrl+K): one box, the actions of the page and the rules in one list. The arrow keys
  // choose, Enter runs, Escape closes. An action that was chosen comes first next time; a rule is not remembered.
  import type { App } from '../lib/app.svelte';
  import { translate, type Key } from '../lib/i18n';
  import { paletteItems, stepIndex, type PaletteItem } from '../lib/palette';
  import { paletteActions } from '../lib/paletteActions';
  import { shownText } from '../lib/ruletext';

  let { app }: { app: App } = $props();
  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);

  let dialog: HTMLDialogElement | undefined = $state();
  let input: HTMLInputElement | undefined = $state();
  let query = $state('');
  let at = $state(0);

  const open = $derived(app.paletteOpen);
  const actions = $derived(open ? paletteActions(app) : []);
  const items = $derived(open ? paletteItems(query, actions, app.rules, app.frecency, Date.now()) : []);
  // The choice stays inside the list when the words narrow it.
  $effect(() => {
    if (at >= items.length) at = Math.max(0, items.length - 1);
  });

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) {
      query = '';
      at = 0;
      dialog.showModal();
      input?.focus();
    }
    if (!open && dialog.open) dialog.close();
  });

  function label(item: PaletteItem): string {
    return item.kind === 'rule' ? `${item.id} ${item.label}` : item.action.label;
  }

  function onKey(event: KeyboardEvent): void {
    if (event.isComposing) return;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      at = stepIndex(at, event.key === 'ArrowDown' ? 1 : -1, items.length);
      document.getElementById(`palette-option-${at}`)?.scrollIntoView({ block: 'nearest' });
    } else if (event.key === 'Enter') {
      event.preventDefault();
      const item = items[at];
      if (item) void app.choose(item);
    }
  }
</script>

<dialog
  bind:this={dialog}
  aria-label={t('palette.title')}
  oncancel={() => app.closePalette()}
  onclose={() => {
    if (!dialog?.open) app.closePalette();
  }}
>
  <input
    bind:this={input}
    type="text"
    role="combobox"
    aria-label={t('palette.label')}
    aria-expanded="true"
    aria-controls="palette-list"
    aria-activedescendant={items.length > 0 ? `palette-option-${at}` : undefined}
    aria-autocomplete="list"
    autocomplete="off"
    spellcheck="false"
    placeholder={t('palette.placeholder')}
    bind:value={query}
    oninput={() => (at = 0)}
    onkeydown={onKey}
  />
  <ul id="palette-list" role="listbox" aria-label={t('palette.title')}>
    {#each items as item, i (item.kind === 'rule' ? `rule:${item.id}` : `action:${item.action.key}`)}
      <li
        id={`palette-option-${i}`}
        role="option"
        aria-selected={i === at}
        class:chosen={i === at}
        onmousemove={() => (at = i)}
        onclick={() => void app.choose(item)}
        onkeydown={null}
      >
        <span class="kind">{item.kind === 'rule' ? t('palette.rule') : t('palette.action')}</span>
        <span class="label">{shownText(label(item))}</span>
      </li>
    {:else}
      <li class="none" role="presentation">{t('palette.empty')}</li>
    {/each}
  </ul>
</dialog>

<style>
  dialog {
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.5rem;
    width: min(38rem, calc(100vw - 2rem));
    margin-top: 12vh;
  }
  dialog::backdrop {
    background: var(--backdrop);
  }
  input {
    width: 100%;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.5rem 0.6rem;
  }
  ul {
    list-style: none;
    margin: 0.4rem 0 0;
    padding: 0;
    max-height: 50vh;
    overflow: auto;
  }
  li {
    display: grid;
    grid-template-columns: 4.5rem minmax(0, 1fr);
    gap: 0.5rem;
    align-items: baseline;
    padding: 0.35rem 0.5rem;
    border-radius: var(--radius);
    cursor: pointer;
  }
  li.chosen {
    background: var(--surface-2);
    outline: 1px solid var(--accent);
  }
  .kind {
    color: var(--muted);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    color: var(--muted);
    cursor: default;
    display: block;
  }
</style>
