// A typed dictionary, English and German. English is the source: its keys are the type, and German must have every
// one of them. The language of the page is not the language of the data: titles and texts stay as written.

import { fill, pickLang } from 'appkit-ui/i18n';

export type Lang = 'de' | 'en';
export const LANGS: readonly Lang[] = ['de', 'en'];

const en = {
  'app.title': 'Rules',
  'nav.overview': 'Overview',
  'nav.families': 'Families',
  'nav.rules': 'Rules',
  'menu.view': 'View',
  'menu.language': 'Language',
  'menu.theme': 'Theme',
  'menu.help': 'Help',
  'menu.shortcuts': 'Single-key shortcuts',
  'menu.rescan': 'Read again',
  'menu.about': 'About',
  'theme.auto': 'as the system',
  'theme.light': 'light',
  'theme.dark': 'dark',
  'phase.starting': 'Starting …',
  'phase.needs_pairing': 'This tab is not paired with the hub.',
  'phase.needs_pairing.how': 'Run rules-hub serve and open the link it prints.',
  'phase.pair_refused': 'The pairing code was used up or too old. Ask the hub for a new link.',
  'phase.offline': 'The hub does not answer.',
  'phase.unavailable': 'The hub runs, but its engine is not ready: {problem}',
  'connection.live': 'live',
  'connection.reconnecting': 'connecting again …',
  'connection.offline': 'offline',
  'overview.rules': 'Rules',
  'overview.families': 'Families',
  'overview.files': 'Files',
  'overview.retired': 'Retired rules',
  'overview.load_errors': 'What failed to load',
  'overview.gates': 'Gates',
  'overview.gate.unknown': 'Families of the gate without a rule: {families}',
  'overview.unclassified': '{n} files have no class on this device and are not released.',
  'families.family': 'Family',
  'families.count': 'Rules',
  'families.with_check': 'with check',
  'families.total': 'All families',
  'severity.critical': 'critical',
  'severity.recommended': 'recommended',
  'severity.nice-to-have': 'nice to have',
  'check.none': 'no check',
  'filter.family': 'Family',
  'filter.severity': 'Severity',
  'filter.check': 'Check',
  'filter.search': 'Search title and text',
  'filter.clear': 'Clear all filters',
  'list.count': '{shown} of {total}',
  'list.empty': 'No rule matches the filter.',
  'search.building': 'The hub is still reading the texts; only titles are searched.',
  'search.failed': 'The texts cannot be searched; only titles are.',
  'class.public': 'public',
  'class.internal': 'internal',
  'class.private': 'private',
  'class.unclassified': 'not classified',
  'detail.close': 'Close',
  'detail.loading': 'Reading the rule …',
  'detail.failed': 'The rule could not be read.',
  'detail.gone': 'The rule {id} is no longer in the rule set.',
  'detail.source': 'Source',
  'detail.copy': 'Copy',
  'detail.version': 'Version',
  'detail.scope.file': 'file',
  'detail.scope.rule': 'rule',
  'detail.section': 'Section',
  'detail.check': 'Check (shown, never run)',
  'detail.agent': 'Agent',
  'detail.not_editable': 'Its check is a Lua function: change it in Neovim only.',
  'detail.hidden': 'The text holds hidden characters; they are shown as ⟦U+…⟧.',
  'palette.title': 'Command palette',
  'palette.placeholder': 'Type a command or a rule',
  'palette.empty': 'Nothing matches.',
  'palette.go': 'Go to: {view}',
  'palette.lang': 'Language: {name}',
  'palette.theme': 'Theme: {name}',
  'palette.shortcuts.off': 'Turn single-key shortcuts off',
  'palette.shortcuts.on': 'Turn single-key shortcuts on',
  'palette.filter_reset': 'Clear all filters',
  'palette.rule': 'Rule',
  'notice.engine_gone': 'The engine does not answer.',
};

export type Key = keyof typeof en;

const de: Record<Key, string> = {
  'app.title': 'Regeln',
  'nav.overview': 'Übersicht',
  'nav.families': 'Familien',
  'nav.rules': 'Regeln',
  'menu.view': 'Ansicht',
  'menu.language': 'Sprache',
  'menu.theme': 'Farben',
  'menu.help': 'Hilfe',
  'menu.shortcuts': 'Einzeltasten',
  'menu.rescan': 'Neu einlesen',
  'menu.about': 'Über',
  'theme.auto': 'wie das System',
  'theme.light': 'hell',
  'theme.dark': 'dunkel',
  'phase.starting': 'Startet …',
  'phase.needs_pairing': 'Dieser Tab ist nicht mit dem Hub verbunden.',
  'phase.needs_pairing.how': 'rules-hub serve starten und den ausgegebenen Link öffnen.',
  'phase.pair_refused': 'Der Kopplungscode war verbraucht oder zu alt. Einen neuen Link vom Hub holen.',
  'phase.offline': 'Der Hub antwortet nicht.',
  'phase.unavailable': 'Der Hub läuft, aber seine Engine ist nicht bereit: {problem}',
  'connection.live': 'live',
  'connection.reconnecting': 'verbindet neu …',
  'connection.offline': 'offline',
  'overview.rules': 'Regeln',
  'overview.families': 'Familien',
  'overview.files': 'Dateien',
  'overview.retired': 'Zurückgezogene Regeln',
  'overview.load_errors': 'Was nicht geladen werden konnte',
  'overview.gates': 'Gates',
  'overview.gate.unknown': 'Familien des Gates ohne Regel: {families}',
  'overview.unclassified': '{n} Dateien haben auf diesem Gerät keine Klasse und sind nicht freigegeben.',
  'families.family': 'Familie',
  'families.count': 'Regeln',
  'families.with_check': 'mit Check',
  'families.total': 'Alle Familien',
  'severity.critical': 'kritisch',
  'severity.recommended': 'empfohlen',
  'severity.nice-to-have': 'wünschenswert',
  'check.none': 'kein Check',
  'filter.family': 'Familie',
  'filter.severity': 'Schweregrad',
  'filter.check': 'Check',
  'filter.search': 'Titel und Text durchsuchen',
  'filter.clear': 'Alle Filter aus',
  'list.count': '{shown} von {total}',
  'list.empty': 'Keine Regel passt zum Filter.',
  'search.building': 'Der Hub liest die Texte noch; gesucht wird nur in den Titeln.',
  'search.failed': 'Die Texte lassen sich nicht durchsuchen, nur die Titel.',
  'class.public': 'öffentlich',
  'class.internal': 'intern',
  'class.private': 'privat',
  'class.unclassified': 'nicht klassifiziert',
  'detail.close': 'Schließen',
  'detail.loading': 'Liest die Regel …',
  'detail.failed': 'Die Regel ließ sich nicht lesen.',
  'detail.gone': 'Die Regel {id} ist nicht mehr im Regelwerk.',
  'detail.source': 'Quelle',
  'detail.copy': 'Kopieren',
  'detail.version': 'Version',
  'detail.scope.file': 'Datei',
  'detail.scope.rule': 'Regel',
  'detail.section': 'Abschnitt',
  'detail.check': 'Check (angezeigt, nie ausgeführt)',
  'detail.agent': 'Agent',
  'detail.not_editable': 'Ihr Check ist eine Lua-Funktion: nur in Neovim änderbar.',
  'detail.hidden': 'Der Text enthält versteckte Zeichen; sie stehen als ⟦U+…⟧ da.',
  'palette.title': 'Befehlspalette',
  'palette.placeholder': 'Befehl oder Regel eingeben',
  'palette.empty': 'Nichts passt.',
  'palette.go': 'Gehe zu: {view}',
  'palette.lang': 'Sprache: {name}',
  'palette.theme': 'Farben: {name}',
  'palette.shortcuts.off': 'Einzeltasten ausschalten',
  'palette.shortcuts.on': 'Einzeltasten einschalten',
  'palette.filter_reset': 'Alle Filter aus',
  'palette.rule': 'Regel',
  'notice.engine_gone': 'Die Engine antwortet nicht.',
};

export const dictionaries: Record<Lang, Record<Key, string>> = { en, de };

export function isLang(value: unknown): value is Lang {
  return value === 'de' || value === 'en';
}

/** The language of the browser when the page has it, else English. */
export function detectLang(preferred: readonly string[]): Lang {
  return pickLang(preferred, LANGS, 'en');
}

export function translate(lang: Lang, key: Key, params: Record<string, string | number> = {}): string {
  return fill(dictionaries[lang][key], params);
}

/**
 * The word for a value the engine or the hub named (`severity.critical`, `class.private`). A value this version
 * does not know shows as it is: the contract may grow a word, and the page must not break on it.
 */
export function labelOf(lang: Lang, prefix: string, value: string | undefined): string {
  if (value === undefined) return '?';
  const key = `${prefix}.${value}`;
  return Object.hasOwn(dictionaries[lang], key) ? translate(lang, key as Key) : value;
}
