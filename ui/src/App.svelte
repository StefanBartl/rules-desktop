<script lang="ts">
  // The shell: the menu bar, the three views, the rule that is open, the palette and the toast. What is shown
  // depends on the phase of the page (pairing, offline, engine not ready, ready); everything else is the state of
  // `app` (lib/app.svelte.ts).
  import MenuBar, { type Menu } from 'appkit-ui/MenuBar.svelte';
  import { installNav } from 'appkit-ui/nav';
  import { isNavState, VIEWS, type App, type NavState, type Theme } from './lib/app.svelte';
  import { LANGS, translate, type Key, type Lang } from './lib/i18n';
  import { commandFor, type ChordState } from './lib/keys';
  import Detail from './views/Detail.svelte';
  import Families from './views/Families.svelte';
  import Overview from './views/Overview.svelte';
  import ConflictCenter from './views/ConflictCenter.svelte';
  import Coverage from './views/Coverage.svelte';
  import NewRule from './views/NewRule.svelte';
  import Palette from './views/Palette.svelte';
  import Rules from './views/Rules.svelte';
  import Toast from './views/Toast.svelte';
  import Waivers from './views/Waivers.svelte';

  let { app, code = null }: { app: App; code?: string | null } = $props();

  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);
  const LANG_NAMES: Record<Lang, string> = { de: 'Deutsch', en: 'English' };
  const THEMES: readonly Theme[] = ['auto', 'light', 'dark'];

  // The page is started once, from its address.
  $effect(() => {
    void app.start(code);
    return () => app.stop();
  });

  // Back and Forward: the view and the open rule are places of the tab's history. The state is in `history.state`,
  // not in the address (appkit-ui/nav).
  const nav = installNav<NavState>({
    history: window.history,
    window,
    isState: isNavState,
    restore: (place) => app.restoreNav(place),
  });
  $effect(() => () => nav.stop());
  $effect(() => {
    // Only a page that shows something has a place: a pairing page or an offline page has none to go back to.
    if (app.phase === 'ready') nav.record(app.navState(), { replace: app.takeNavReplace() });
  });

  // The language and the theme of the document follow the state.
  $effect(() => {
    document.documentElement.lang = app.lang;
    if (app.theme === 'auto') delete document.documentElement.dataset['theme'];
    else document.documentElement.dataset['theme'] = app.theme;
    document.title = t('app.title');
  });

  const menus = $derived<Menu[]>([
    {
      id: 'view',
      label: t('menu.view'),
      items: [
        ...VIEWS.map((view) => ({
          kind: 'radio' as const,
          label: t(`nav.${view}`),
          checked: app.view === view,
          onselect: () => app.setView(view),
        })),
        { kind: 'separator' as const },
        { kind: 'action' as const, label: t('menu.rescan'), onselect: () => void app.reload() },
      ],
    },
    {
      id: 'language',
      label: t('menu.language'),
      items: LANGS.map((lang) => ({
        kind: 'radio' as const,
        label: LANG_NAMES[lang],
        checked: app.lang === lang,
        onselect: () => app.setLang(lang),
      })),
    },
    {
      id: 'theme',
      label: t('menu.theme'),
      items: THEMES.map((theme) => ({
        kind: 'radio' as const,
        label: t(`theme.${theme}`),
        checked: app.theme === theme,
        onselect: () => app.setTheme(theme),
      })),
    },
    {
      id: 'help',
      label: t('menu.help'),
      items: [
        {
          kind: 'check' as const,
          label: t('menu.shortcuts'),
          checked: app.shortcuts,
          onselect: () => app.setShortcuts(!app.shortcuts),
        },
        { kind: 'info' as const, label: t('help.keys') },
      ],
    },
  ]);

  // The keyboard. A letter in a field, a menu or a dialog is typed, not a shortcut.
  let chord: ChordState = null;
  function onKeydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    const target = event.target instanceof HTMLElement ? event.target : null;
    const typing =
      target !== null && target.closest('input, textarea, select, [contenteditable="true"], [role="menu"], dialog') !== null;
    const result = commandFor(
      {
        key: event.key,
        ctrlKey: event.ctrlKey,
        metaKey: event.metaKey,
        altKey: event.altKey,
        shiftKey: event.shiftKey,
        typing,
      },
      chord,
      Date.now(),
      app.shortcuts,
    );
    chord = result.chord;
    if (result.command === null) return;
    // Enter on a button or a link is that control's own; only the list and the table take it as "open".
    if (result.command.kind === 'open' && target?.closest('button, a, summary') != null) return;
    event.preventDefault();
    app.command(result.command);
  }
</script>

<svelte:window onkeydown={onKeydown} onfocus={() => void app.focused()} />

<a class="skip" href="#content">{t('app.skip')}</a>

<header>
  <h1>{t('app.title')}</h1>
  <MenuBar {menus} label={t('menu.label')} />
  {#if app.phase === 'ready'}
    <nav aria-label={t('nav.label')}>
      {#each VIEWS as view (view)}
        <button type="button" aria-current={app.view === view ? 'page' : undefined} onclick={() => app.setView(view)}
          >{t(`nav.${view}`)}</button
        >
      {/each}
    </nav>
  {/if}
  <span class="connection" class:live={app.connection === 'live'} role="status">{t(`connection.${app.connection}`)}</span>
</header>

<main id="content">
  {#if app.phase === 'starting'}
    <p role="status">{t('phase.starting')}</p>
  {:else if app.phase === 'needs_pairing'}
    <section class="state">
      <p>{app.pairRefused ? t('phase.pair_refused') : t('phase.needs_pairing')}</p>
      <p class="muted"><code>{t('phase.needs_pairing.how')}</code></p>
    </section>
  {:else if app.phase === 'offline'}
    <p role="alert">{t('phase.offline')}</p>
  {:else if app.phase === 'unavailable'}
    <p role="alert">{t('phase.unavailable', { problem: app.problem ?? '?' })}</p>
  {:else}
    <div class="layout" class:open={app.detail.state !== 'none'}>
      <div class="view">
        {#if app.view === 'overview'}
          <Overview {app} />
        {:else if app.view === 'families'}
          <Families {app} />
        {:else if app.view === 'coverage'}
          <Coverage {app} />
        {:else if app.view === 'waivers'}
          <Waivers {app} />
        {:else}
          <Rules {app} />
        {/if}
      </div>
      {#if app.detail.state !== 'none'}
        <Detail {app} />
      {/if}
    </div>
  {/if}
</main>

{#if app.phase === 'ready'}
  <Palette {app} />
  <NewRule {app} />
  <ConflictCenter {app} />
{/if}
<Toast {app} />

<style>
  .skip {
    position: absolute;
    left: -999px;
    top: 0;
    background: var(--surface);
    padding: 0.4rem 0.8rem;
    z-index: 50;
  }
  .skip:focus {
    left: 0.5rem;
    top: 0.5rem;
  }
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.5rem 1rem;
    padding: 0.4rem 1rem;
    border-bottom: 1px solid var(--line);
    background: var(--surface);
  }
  header h1 {
    margin: 0;
  }
  nav {
    display: flex;
    gap: 0.25rem;
  }
  nav button {
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius);
    padding: 0.25rem 0.7rem;
    cursor: pointer;
  }
  nav button[aria-current='page'] {
    background: var(--accent-soft);
    border-color: var(--accent);
  }
  .connection {
    margin-left: auto;
    color: var(--muted);
    font-size: 0.85rem;
  }
  .connection.live {
    color: var(--ok);
  }
  main {
    padding: 1rem;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 1rem;
  }
  .layout.open {
    grid-template-columns: minmax(0, 1fr) minmax(18rem, 34rem);
  }
  @media (max-width: 52rem) {
    .layout.open {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .muted {
    color: var(--muted);
  }
</style>
