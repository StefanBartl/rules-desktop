<script lang="ts">
  // The conflict center: a change that met another version of the file is never written over. The dialog is the kit's
  // (every conflict in a list, or one at a time, as the person chooses); this file words each conflict - what was
  // changed next to what the file says now - and names what can be done: write the change onto the new version (their
  // decision to overwrite the other change), look at the rule, or drop the change. Nothing is merged for them.
  import KitConflictCenter from 'appkit-ui/ConflictCenter.svelte';
  import type { ConflictItem } from 'appkit-ui/conflicts';
  import { conflictKey, type App } from '../lib/app.svelte';
  import { retryOp, rowsOf } from '../lib/conflicts';
  import { translate, type Key } from '../lib/i18n';
  import { shownText } from '../lib/ruletext';
  import { isWaiverOp } from '../lib/write';

  let { app }: { app: App } = $props();
  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);

  const items = $derived(
    app.conflicts.map((conflict): ConflictItem => ({
      key: conflictKey(conflict),
      title: conflict.id,
      id: '',
      notes: [isWaiverOp(conflict.op) ? t('conflict.hint_waivers') : t('conflict.hint', { id: shownText(conflict.id) })],
      rows: rowsOf(conflict.op, app.conflictRules[conflict.id] ?? null).map((row) => ({
        key: row.field,
        label: row.field,
        wanted: row.mine,
        now: row.current,
      })),
      actions: [
        ...(retryOp(conflict) !== null
          ? [{ name: 'retry', label: t('conflict.retry'), primary: true, disabled: app.writing }]
          : []),
        { name: 'view', label: isWaiverOp(conflict.op) ? t('conflict.view_waivers') : t('conflict.view') },
        { name: 'drop', label: t('conflict.drop') },
      ],
    })),
  );

  /** The close button drops what is asked: the first conflict in the single mode, all of them in the list. */
  function drop(): void {
    if (app.conflictMode === 'single') void app.resolveConflict('drop');
    else app.clearAllConflicts();
  }

  /** Escape drops the one conflict in front of the person; with a list it drops nothing (a key press must not lose them all). */
  function cancel(): void {
    if (app.conflictMode === 'single') void app.resolveConflict('drop');
  }
</script>

<KitConflictCenter
  open={app.conflicts.length > 0}
  {items}
  mode={app.conflictMode}
  onmode={(mode) => app.setConflictMode(mode)}
  onaction={(key, action) => app.resolveConflict(action as 'retry' | 'view' | 'drop', key)}
  onclose={drop}
  oncancel={cancel}
  show={shownText}
  labels={{
    title: t('conflict.label'),
    intro: t('conflict.intro'),
    field: t('conflict.field'),
    wanted: t('conflict.mine'),
    now: t('conflict.current'),
    more: (n) => t('conflict.more', { n }),
    discardAll: '',
    close: app.conflictMode === 'single' ? t('conflict.drop') : t('conflict.drop_all'),
    view: t('conflict.show'),
    viewList: t('conflict.show_list'),
    viewSingle: t('conflict.show_single'),
  }}
/>
