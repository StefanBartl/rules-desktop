<script lang="ts">
  // One line for what happened that the page cannot show in place (the engine is gone, the classes could not be
  // read, a rule left the rule set). The status region stands from the start so that a screen reader hears it.
  import type { App } from '../lib/app.svelte';
  import { translate, type Key } from '../lib/i18n';

  let { app }: { app: App } = $props();
  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);
  const notice = $derived(app.notice);
</script>

<div class="toasts" role="status" aria-live="polite">
  {#if notice}
    <div class="toast">
      <span>{t(notice.key, notice.params ?? {})}</span>
      <button type="button" class="close" aria-label={t('detail.close')} onclick={() => app.dismissNotice()}>×</button>
    </div>
  {/if}
</div>

<style>
  .toasts {
    position: fixed;
    left: 1rem;
    bottom: 1rem;
    z-index: 30;
    max-width: min(34rem, calc(100vw - 2rem));
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    background: var(--surface);
    border: 1px solid var(--line);
    border-left: 4px solid var(--warn);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    padding: 0.5rem 0.75rem;
  }
  .close {
    background: none;
    border: 0;
    color: var(--muted);
    cursor: pointer;
    font-size: 1.1rem;
    line-height: 1;
  }
</style>
