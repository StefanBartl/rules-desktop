// What the palette can do: the actions of the page for the state it is in. The rules themselves are the palette's
// second kind of answer (`palette.ts`); here are only the actions, each with a stable key for the frecency.

import { VIEWS, type App, type Theme } from './app.svelte';
import { isActive } from './filter';
import { translate, type Key } from './i18n';
import type { PaletteAction } from 'appkit-ui/palette';

const THEMES: readonly Theme[] = ['auto', 'light', 'dark'];

export function paletteActions(app: App): PaletteAction[] {
  const t = (key: Key, params: Record<string, string | number> = {}) => translate(app.lang, key, params);
  const out: PaletteAction[] = [];
  const add = (key: string, label: string, run: () => void | Promise<void>, hint = ''): void => {
    out.push({ key, label, hint, run });
  };
  for (const view of VIEWS) {
    if (view === app.view) continue;
    add(`go.${view}`, t('palette.go', { view: t(`nav.${view}`) }), () => app.setView(view));
  }
  add('rescan', t('menu.rescan'), () => app.reload());
  add('rule.new', t('palette.new'), () => app.openNewRule());
  if (isActive(app.filter)) add('filter.reset', t('palette.filter_reset'), () => app.resetFilter());
  if (app.shortcuts) add('shortcuts.off', t('palette.shortcuts.off'), () => app.setShortcuts(false));
  else add('shortcuts.on', t('palette.shortcuts.on'), () => app.setShortcuts(true));
  if (app.lang !== 'de') add('lang.de', t('palette.lang', { name: 'Deutsch' }), () => app.setLang('de'));
  if (app.lang !== 'en') add('lang.en', t('palette.lang', { name: 'English' }), () => app.setLang('en'));
  for (const theme of THEMES) {
    if (theme === app.theme) continue;
    add(`theme.${theme}`, t('palette.theme', { name: t(`theme.${theme}`) }), () => app.setTheme(theme));
  }
  return out;
}
