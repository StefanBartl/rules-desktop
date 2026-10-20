<script lang="ts">
  // One line for what happened that the page cannot show in place (the engine is gone, the classes could not be
  // read, a rule left the rule set). The toast and its live regions are the kit's.
  import KitToast from 'appkit-ui/Toast.svelte';
  import type { App } from '../lib/app.svelte';
  import { translate, type Key } from '../lib/i18n';

  let { app }: { app: App } = $props();
  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);
  const notice = $derived(app.notice);
  const message = $derived(
    notice ? { kind: 'warn' as const, text: t(notice.key, notice.params ?? {}), undo: !!notice.undo } : null,
  );
</script>

<KitToast
  toast={message}
  busy={app.writing}
  onundo={() => app.undo()}
  ondismiss={() => app.dismissNotice()}
  labels={{ undo: t('write.undo'), dismiss: t('detail.close') }}
/>
