<script lang="ts">
  // The command palette (Ctrl+K) of the page: the box, the list and the keys are the kit's; this file gives it the
  // actions of the page, the rules and the words. An action that was chosen comes first next time; a rule is not
  // remembered (no `entryPrefix`), since nothing of a rule set is kept in the browser.
  import KitPalette from 'appkit-ui/Palette.svelte';
  import { paletteItems, type PaletteItem } from 'appkit-ui/palette';
  import type { App } from '../lib/app.svelte';
  import { translate, type Key } from '../lib/i18n';
  import { paletteActions } from '../lib/paletteActions';
  import { shownText } from '../lib/ruletext';

  let { app }: { app: App } = $props();
  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);

  function items(query: string): PaletteItem[] {
    return paletteItems(query, paletteActions(app), app.rules, app.frecency, Date.now());
  }
</script>

<KitPalette
  open={app.paletteOpen}
  {items}
  onchoose={(item) => app.choose(item)}
  onclose={() => app.closePalette()}
  show={shownText}
  labels={{
    title: t('palette.title'),
    label: t('palette.label'),
    placeholder: t('palette.placeholder'),
    none: t('palette.empty'),
    action: t('palette.action'),
    entry: t('palette.rule'),
  }}
/>
