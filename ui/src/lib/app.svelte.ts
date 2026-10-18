// The state of the page and what keeps it true: pairing, the held snapshot and the classes of its files, the event
// stream, the detail of the open rule and the search of the texts. Nothing of a rule set is kept in the browser:
// `prefs` holds the language, the theme, the single-key switch and the frecency of actions, `session` the token.

import { takePairingCode, type HistoryLike, type LocationLike } from 'appkit-ui/pair';
import { backoffSeconds, readStream, StreamRefused, type SseMessage } from 'appkit-ui/sse';
import { ApiError, type Hub, type HubSources } from './api';
import type { Rule, RuleSummary, Snapshot, Waivers } from './contract';
import { familyRows, type FamilyRow } from './families';
import { applyFilter, emptyFilter, moveSelection, type Filter } from './filter';
import { detectLang, isLang, type Key, type Lang } from './i18n';
import type { Command } from './keys';
import { bumped, parseFrecency, type Frecency, type PaletteItem } from './palette';
import { classOf, sourceIndex } from './sources';
import { retryOp } from './conflicts';
import type { Body } from './edit';
import {
  clientOpId,
  isWaiverOp,
  nextOf,
  retireOp,
  setOp,
  summarize,
  type Conflict,
  type Op,
  type Outcome,
} from './write';

export type Phase = 'starting' | 'needs_pairing' | 'offline' | 'unavailable' | 'ready';
export type Connection = 'live' | 'reconnecting' | 'offline';
export type View = 'overview' | 'families' | 'rules' | 'coverage' | 'waivers';
export type Theme = 'auto' | 'light' | 'dark';

export const VIEWS: readonly View[] = ['overview', 'families', 'rules', 'coverage', 'waivers'];

/**
 * A place the person can go back to: the view and the rule that is open in it. Filters, the search and the cursor are
 * not places (typing a letter must not make an entry), and nothing of this goes into the address.
 */
export interface NavState {
  view: View;
  id: string | null;
}

export function isNavState(value: unknown): value is NavState {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as { view?: unknown; id?: unknown };
  return VIEWS.includes(v.view as View) && (v.id === null || (typeof v.id === 'string' && v.id.length <= 100));
}

export type Detail =
  | { state: 'none' }
  | { state: 'loading'; id: string }
  | { state: 'ok'; id: string; doc: Rule }
  | { state: 'failed'; id: string };

/** One line for the toast: a key of the dictionary and its values. */
export interface Notice {
  key: Key;
  params?: Record<string, string>;
  /** The operations that take back what the notice reports: the toast offers them as Undo. */
  undo?: Op[];
}

/** What the page needs from its surroundings; each part is replaced in the tests. */
export interface Env {
  hub: Hub;
  /** Per-tab storage: the session token only. */
  session: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
  /** Per-browser storage: language, theme, the single-key switch, the frecency of actions. Never a rule. */
  prefs: Pick<Storage, 'getItem' | 'setItem'>;
  preferredLanguages: readonly string[];
  fetchFn: typeof fetch;
  sleep(ms: number): Promise<void>;
  now(): number;
}

export const TOKEN_KEY = 'rules-desktop.session';
export const PREF_KEYS = {
  lang: 'rules-desktop.lang',
  theme: 'rules-desktop.theme',
  shortcuts: 'rules-desktop.shortcuts',
  frecency: 'rules-desktop.frecency',
} as const;

/** The pause before the text of the rules is searched for the words typed, and the fewest letters searched for. */
export const SEARCH_DELAY_MS = 200;
export const MIN_QUERY_CHARS = 2;
/** A window that gets the focus back asks for a scan at most this often. */
export const RESCAN_EVERY_MS = 5000;
/** Waits before the hub is asked again: when it did not answer, and when its engine was not ready. */
export const OFFLINE_RETRY_MS = 3000;
export const UNAVAILABLE_RETRY_MS = 5000;
/** A search the hub answers with `building` (it is still reading the texts) is asked again, as in tasks. */
export const SEARCH_RETRY_MS = 1500;
export const SEARCH_RETRIES = 8;
/** A pairing code the hub turned away for too many attempts (429) is still good: it is traded again a few times. */
export const EXCHANGE_RETRIES = 3;
/**
 * The open rule left the snapshot: the page waits this long and reads again before it asks for it. The hub answers
 * 404 for an id it does not hold (K8), and a retire or a file saved half way leaves a rule out for one state.
 */
export const GONE_SETTLE_MS = 2000;

function safe<T>(work: () => T): T | null {
  try {
    return work();
  } catch {
    return null;
  }
}

/** The hits of a text search the list uses; `cut`: the hub found the words in more rules than it named. */
export interface SearchHits {
  query: string;
  ids: ReadonlySet<string>;
  status: 'ready' | 'building' | 'failed';
  /** How many rules hold the words, as the hub counted them. */
  total: number;
  cut: boolean;
}

/** The ids of an event of the stream; anything that is not a list of strings is no id. */
function ids(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
}

export class App {
  phase = $state<Phase>('starting');
  /** The engine's problem while it is not ready (a word of the hub). */
  problem = $state<string | null>(null);
  /** The pairing code of the address was refused and the tab has no session. */
  pairRefused = $state(false);
  connection = $state<Connection>('offline');
  view = $state<View>('overview');
  lang = $state<Lang>('en');
  theme = $state<Theme>('auto');
  shortcuts = $state(true);

  snapshot = $state.raw<Snapshot | null>(null);
  etag = $state<string | null>(null);
  sources = $state.raw<HubSources | null>(null);
  filter = $state.raw<Filter>(emptyFilter());
  selectedId = $state<string | null>(null);
  /** The family the cursor of the family table is on. */
  selectedFamily = $state<string | null>(null);
  detail = $state.raw<Detail>({ state: 'none' });
  /** The ids the hub found the words of the filter in, with the state of its index; `null`: no text search. */
  searchHits = $state.raw<SearchHits | null>(null);
  paletteOpen = $state(false);
  notice = $state.raw<Notice | null>(null);
  /** Goes up by one each time the search box should take the focus (`/`). */
  searchFocus = $state(0);
  frecency = $state.raw<Frecency>({});

  private stopped = false;
  /** The event stream is open; only the stream says `live`. */
  private streamOpen = false;
  private abort: AbortController | null = null;
  private detailAbort: AbortController | null = null;
  private searchAbort: AbortController | null = null;
  private searchSeq = 0;
  private lastRescan = -Infinity;
  /** The read in progress; a change while it runs makes exactly one more. */
  private loop: Promise<void> | null = null;
  private dirty = false;

  constructor(private readonly env: Env) {
    const lang = safe(() => env.prefs.getItem(PREF_KEYS.lang));
    this.lang = isLang(lang) ? lang : detectLang(env.preferredLanguages);
    const theme = safe(() => env.prefs.getItem(PREF_KEYS.theme));
    this.theme = theme === 'light' || theme === 'dark' ? theme : 'auto';
    this.shortcuts = safe(() => env.prefs.getItem(PREF_KEYS.shortcuts)) !== 'off';
    this.frecency = parseFrecency(safe(() => env.prefs.getItem(PREF_KEYS.frecency)));
  }

  // ── derived ───────────────────────────────────────────────────────────

  get rules(): readonly RuleSummary[] {
    return this.snapshot?.rules ?? [];
  }

  /** The rules the list shows: the filter, with the hits of the text search when they are for the words typed. */
  shown = $derived.by(() => {
    const hits = this.searchHits;
    const current = hits && hits.query === this.filter.query.trim() ? hits.ids : null;
    return applyFilter(this.snapshot?.rules ?? [], this.filter, current);
  });

  families = $derived.by<FamilyRow[]>(() => familyRows(this.snapshot?.families ?? []));

  private index = $derived.by(() => sourceIndex(this.sources));

  /** The class of a rule, by its file as the hub classified it. */
  classOfRule(rule: Pick<RuleSummary, 'source'>): ReturnType<typeof classOf> {
    return classOf(this.index, rule.source);
  }

  // ── settings ──────────────────────────────────────────────────────────

  setLang(lang: Lang): void {
    this.lang = lang;
    safe(() => this.env.prefs.setItem(PREF_KEYS.lang, lang));
  }

  setTheme(theme: Theme): void {
    this.theme = theme;
    safe(() => this.env.prefs.setItem(PREF_KEYS.theme, theme));
  }

  setShortcuts(on: boolean): void {
    this.shortcuts = on;
    safe(() => this.env.prefs.setItem(PREF_KEYS.shortcuts, on ? 'on' : 'off'));
  }

  setView(view: View): void {
    this.view = view;
  }

  // ── start ─────────────────────────────────────────────────────────────

  /** Bring the page up from its address: a pairing code in the fragment is taken out of it at once and traded. */
  startFrom(location: LocationLike, history: HistoryLike): Promise<void> {
    return this.start(takePairingCode(location, history));
  }

  /** Bring the page up. `code` is a pairing code taken from the address. */
  async start(code: string | null): Promise<void> {
    this.stopped = false;
    this.phase = 'starting';
    this.pairRefused = false;
    let token = safe(() => this.env.session.getItem(TOKEN_KEY));
    if (code) {
      for (let attempt = 0; ; attempt++) {
        try {
          const traded = await this.env.hub.exchange(code);
          token = traded;
          safe(() => this.env.session.setItem(TOKEN_KEY, traded));
          break;
        } catch (error) {
          // A code that is spent or too old (401, the one answer of the hub for both): a session the tab has is still
          // good. Only that refusal says the code is bad.
          if (error instanceof ApiError && error.status === 401) {
            this.pairRefused = token === null;
            break;
          }
          // Too many attempts: the hub did not look at the code, which is still good but no longer in the address.
          const limited = error instanceof ApiError && error.status === 429;
          if (limited && attempt < EXCHANGE_RETRIES) {
            await this.env.sleep(OFFLINE_RETRY_MS);
            if (this.stopped) return;
            continue;
          }
          if (token !== null) break;
          if (limited) {
            this.phase = 'needs_pairing';
            this.notice = { key: 'notice.pair_rate_limited' };
          } else {
            this.phase = 'offline';
          }
          return;
        }
      }
    }
    if (token === null) {
      this.phase = 'needs_pairing';
      return;
    }
    this.env.hub.setToken(token);
    this.env.hub.onUnauthorized = () => this.lostSession();

    for (;;) {
      try {
        const hello = await this.env.hub.hello();
        if (hello.engine.ready) break;
        this.problem = hello.engine.problem ?? 'unknown';
        this.phase = 'unavailable';
        await this.env.sleep(UNAVAILABLE_RETRY_MS);
      } catch {
        this.connection = 'offline';
        this.phase = 'offline';
        await this.env.sleep(OFFLINE_RETRY_MS);
      }
      if (this.stopped) return;
    }
    this.problem = null;
    await this.refresh();
    if (this.stopped || !this.env.hub.hasToken) return;
    void this.followStream();
  }

  stop(): void {
    this.stopped = true;
    this.abort?.abort();
    this.detailAbort?.abort();
    this.searchAbort?.abort();
  }

  private lostSession(): void {
    safe(() => this.env.session.removeItem(TOKEN_KEY));
    this.env.hub.setToken(null);
    this.abort?.abort();
    this.detailAbort?.abort();
    this.phase = 'needs_pairing';
    this.connection = 'offline';
    this.snapshot = null;
    this.etag = null;
    this.sources = null;
    this.searchHits = null;
    this.selectedId = null;
    this.detail = { state: 'none' };
    this.paletteOpen = false;
  }

  // ── reading ───────────────────────────────────────────────────────────

  /**
   * Read the snapshot again, with the tag of the one held. One at a time: a change that arrives while a read runs
   * makes exactly one more read after it, and a caller then waits for that one too.
   */
  refresh(): Promise<void> {
    if (this.loop) {
      this.dirty = true;
      return this.loop;
    }
    this.loop = (async () => {
      try {
        do {
          this.dirty = false;
          await this.readOnce();
        } while (this.dirty && !this.stopped);
      } finally {
        this.loop = null;
      }
    })();
    return this.loop;
  }

  private async readOnce(): Promise<void> {
    const hub = this.env.hub;
    try {
      const result = await hub.snapshot(this.snapshot ? this.etag : null);
      if (result.kind === 'fresh') {
        this.snapshot = result.doc;
        this.etag = result.etag;
        // The classes are of the files of a snapshot: a new snapshot may have other files.
        this.sources = null;
      }
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) return;
      if (error instanceof ApiError && error.code === 'engine_unavailable') {
        this.phase = 'unavailable';
        this.notice = { key: 'notice.engine_gone' };
        return;
      }
      if (this.snapshot === null) this.phase = 'offline';
      // The hub did not answer this read; whether the stream is open is the stream's to say.
      if (!this.streamOpen) this.connection = 'offline';
      return;
    }
    // The rules do not wait for their classes: without them every rule is unclassified (fail-closed), and the next
    // read asks again.
    if (this.sources === null) await this.readSources();
    if (!hub.hasToken) return;
    this.phase = 'ready';
    this.connection = this.streamOpen ? 'live' : 'reconnecting';
    this.keepSelection();
  }

  private async readSources(): Promise<void> {
    try {
      this.sources = await this.env.hub.sources();
    } catch (error) {
      this.sources = null;
      if (error instanceof ApiError && error.status === 401) return;
      if (this.notice?.key !== 'notice.sources_failed') this.notice = { key: 'notice.sources_failed' };
    }
  }

  /** A selected rule that left the list is not selected any more; the open detail is the stream's business. */
  private keepSelection(): void {
    if (this.selectedId !== null && !this.rules.some((r) => r.id === this.selectedId)) this.selectedId = null;
    if (this.marked.size > 0 && [...this.marked].some((id) => !this.rules.some((r) => r.id === id))) {
      this.marked = new Set([...this.marked].filter((id) => this.rules.some((r) => r.id === id)));
    }
    if (this.selectedFamily !== null && !this.families.some((f) => f.family === this.selectedFamily)) {
      this.selectedFamily = null;
    }
  }

  // ── the stream ────────────────────────────────────────────────────────

  private async followStream(): Promise<void> {
    let attempt = 0;
    while (!this.stopped && this.env.hub.hasToken) {
      this.abort = new AbortController();
      try {
        await readStream(
          this.env.fetchFn,
          this.env.hub.eventsUrl,
          this.env.hub.bearer,
          {
            onOpen: () => {
              attempt = 0;
              this.streamOpen = true;
              this.connection = 'live';
              // What happened while the stream was down.
              void this.refresh();
            },
            onMessage: (m) => void this.onMessage(m),
          },
          this.abort.signal,
        );
      } catch (error) {
        if (this.stopped) return;
        if (error instanceof StreamRefused && error.status === 401) {
          this.lostSession();
          return;
        }
      } finally {
        this.streamOpen = false;
      }
      if (this.stopped || !this.env.hub.hasToken) return;
      this.connection = 'reconnecting';
      await this.env.sleep(backoffSeconds(attempt++) * 1000);
    }
  }

  /** One event of the stream. Returns when what it asked for is read (the tests wait for it). */
  async onMessage(message: SseMessage): Promise<void> {
    if (message.event === 'rules') {
      let doc: Record<string, unknown> = {};
      try {
        const parsed: unknown = JSON.parse(message.data);
        if (parsed !== null && typeof parsed === 'object') doc = parsed as Record<string, unknown>;
      } catch {
        doc = { truncated: true };
      }
      // Always the snapshot: the ids follow the etag of the file, and a change of the gates alone comes with empty
      // lists. The tag makes it cheap when nothing changed.
      await this.refresh();
      const open = this.detail.state === 'none' ? null : this.detail.id;
      if (open === null) return;
      if (ids(doc['removed']).includes(open)) {
        await this.settleGone(open);
      } else if (ids(doc['changed_ids']).includes(open) || doc['truncated'] === true) {
        await this.loadDetail(open);
      }
      return;
    }
    if (message.event === 'resync') {
      // The stream lost events: everything is read again.
      this.sources = null;
      await this.refresh();
      if (this.detail.state !== 'none') await this.loadDetail(this.detail.id);
      return;
    }
    if (message.event === 'hello') {
      // Where the stream stands: another revision than ours came between our read and the stream.
      try {
        const doc = JSON.parse(message.data) as { rev?: unknown };
        if (typeof doc.rev === 'string' && this.snapshot && doc.rev !== this.snapshot.rev) await this.refresh();
      } catch {
        await this.refresh();
      }
    }
  }

  /**
   * The open rule is not in the new snapshot. It is not closed at once: a retire or a file saved half way leaves it
   * out for one state, and the hub answers 404 for any id it does not hold (K8), so asking at once would close it
   * every time. The page waits a moment and reads the snapshot again, then asks for the rule once; only a 404 closes
   * the detail, with a word why (`loadDetail`).
   */
  private async settleGone(id: string): Promise<void> {
    await this.env.sleep(GONE_SETTLE_MS);
    if (this.stopped || !this.isOpen(id)) return;
    await this.refresh();
    if (this.isOpen(id)) await this.loadDetail(id);
  }

  /** The hub does not hold the open rule any more: the detail closes with a word why. */
  private closeGone(id: string): void {
    // The page left the place by itself: the history entry of that rule is replaced, not followed by a new one
    // (Back would lead to the entry of a rule that is gone and the page would leave it again at once).
    this.navReplace = true;
    this.detail = { state: 'none' };
    if (this.selectedId === id) this.selectedId = null;
    this.notice = { key: 'detail.gone', params: { id } };
  }

  /** The window got the focus back: ask for a scan, at most every few seconds, then read. */
  async focused(): Promise<void> {
    if (this.phase !== 'ready') return;
    const now = this.env.now();
    if (now - this.lastRescan < RESCAN_EVERY_MS) return;
    this.lastRescan = now;
    try {
      await this.env.hub.rescan();
    } catch {
      // A hub that is gone shows itself through the stream.
    }
    await this.refresh();
  }

  /** Read again on request (the menu, the palette): a scan of the hub, then the snapshot, without the pause. */
  async reload(): Promise<void> {
    if (this.phase !== 'ready') return;
    this.lastRescan = this.env.now();
    try {
      await this.env.hub.rescan();
    } catch {
      // A hub that is gone shows itself through the stream.
    }
    await this.refresh();
  }

  // ── filter and search ─────────────────────────────────────────────────

  setFilter(filter: Filter): void {
    const before = this.filter.query;
    this.filter = filter;
    if (filter.query !== before) void this.searchText(filter.query);
  }

  setQuery(text: string): Promise<void> {
    this.filter = { ...this.filter, query: text };
    return this.searchText(text);
  }

  resetFilter(): void {
    this.setFilter(emptyFilter());
  }

  /** Narrow the list to one family and show it (Enter in the family table). */
  openFamily(family: string): void {
    this.setFilter({ ...emptyFilter(), families: [family] });
    this.view = 'rules';
  }

  /** The words are searched in the texts by the hub a moment after the last key, from two letters on. */
  private async searchText(text: string): Promise<void> {
    const seq = ++this.searchSeq;
    this.searchAbort?.abort();
    const query = text.trim();
    if ([...query].length < MIN_QUERY_CHARS || this.phase !== 'ready') {
      this.searchHits = null;
      return;
    }
    for (let attempt = 0; ; attempt++) {
      // The hub reads the texts a moment after it starts: a `building` answer is asked again, a few times.
      await this.env.sleep(attempt === 0 ? SEARCH_DELAY_MS : SEARCH_RETRY_MS);
      if (seq !== this.searchSeq) return;
      const abort = new AbortController();
      this.searchAbort = abort;
      try {
        const result = await this.env.hub.search(query, abort.signal);
        if (seq !== this.searchSeq) return;
        const found = new Set(result.hits.map((h) => h.id));
        const total = Math.max(result.total, found.size);
        this.searchHits = {
          query,
          ids: found,
          status: result.status,
          total,
          // More rules hold the words than the hub named (it names SEARCH_LIMIT at most): the list must say so.
          cut: result.partial === true || total > found.size,
        };
        if (result.status !== 'building' || attempt >= SEARCH_RETRIES) return;
      } catch {
        if (seq === this.searchSeq) this.searchHits = { query, ids: new Set(), status: 'failed', total: 0, cut: false };
        return;
      }
    }
  }

  // ── selection and detail ──────────────────────────────────────────────

  /** Put the cursor on a rule. The rule is read only when it is opened: each read is a question to the engine. */
  select(id: string | null): void {
    this.selectedId = id;
  }

  async openDetail(id: string): Promise<void> {
    this.selectedId = id;
    await this.loadDetail(id);
  }

  closeDetail(): void {
    this.detailAbort?.abort();
    this.detail = { state: 'none' };
  }

  /** Set when the page left a place by itself; `takeNavReplace` reads it once. */
  private navReplace = false;

  /** Is the change of place that is recorded next one the page made by itself (a replaced entry)? Reads once. */
  takeNavReplace(): boolean {
    const replace = this.navReplace;
    this.navReplace = false;
    return replace;
  }

  /** The place the page is at now, for the history of the tab. */
  navState(): NavState {
    return { view: this.view, id: this.detail.state === 'none' ? null : this.detail.id };
  }

  /** Back or Forward brought the tab to this place. */
  restoreNav(place: NavState): void {
    this.view = place.view;
    if (place.id === null) this.closeDetail();
    else void this.openDetail(place.id);
  }

  /** Is this rule the one the detail shows or reads? */
  private isOpen(id: string): boolean {
    const detail = this.detail;
    return detail.state !== 'none' && detail.id === id;
  }

  private async loadDetail(id: string): Promise<void> {
    // What is shown stays while the same rule is read again.
    if (!(this.detail.state === 'ok' && this.detail.id === id)) this.detail = { state: 'loading', id };
    this.detailAbort?.abort();
    const abort = new AbortController();
    this.detailAbort = abort;
    try {
      const doc = await this.env.hub.rule(id, abort.signal);
      if (!abort.signal.aborted && this.isOpen(id)) {
        this.detail = { state: 'ok', id, doc };
      }
    } catch (error) {
      if (abort.signal.aborted || !this.isOpen(id)) return;
      // The hub holds no such rule: after a removal, a resync or a truncated event alike, the detail closes.
      if (error instanceof ApiError && error.status === 404) this.closeGone(id);
      else this.detail = { state: 'failed', id };
    }
  }

  // ── keys and palette ──────────────────────────────────────────────────

  /** Run a command of the keyboard (`keys.ts`). */
  command(command: Command): void {
    if (this.phase !== 'ready') return;
    switch (command.kind) {
      case 'palette':
        this.paletteOpen = true;
        return;
      case 'view':
        this.view = command.view;
        return;
      case 'search':
        this.view = 'rules';
        this.searchFocus += 1;
        return;
      case 'move':
        if (this.view === 'rules') {
          this.selectedId = moveSelection(
            this.shown.map((r) => r.id),
            this.selectedId,
            command.delta,
          );
        } else if (this.view === 'families') {
          this.selectedFamily = moveSelection(
            this.families.map((f) => f.family),
            this.selectedFamily,
            command.delta,
          );
        }
        return;
      case 'open':
        if (this.view === 'rules' && this.selectedId !== null) void this.openDetail(this.selectedId);
        else if (this.view === 'families' && this.selectedFamily !== null) this.openFamily(this.selectedFamily);
        return;
      case 'escape':
        // The palette first, then the detail, then the selection.
        if (this.paletteOpen) this.paletteOpen = false;
        else if (this.detail.state !== 'none') this.closeDetail();
        else if (this.view === 'families') this.selectedFamily = null;
        else this.selectedId = null;
        return;
    }
  }

  openPalette(): void {
    if (this.phase === 'ready') this.paletteOpen = true;
  }

  closePalette(): void {
    this.paletteOpen = false;
  }

  /** Run what was chosen in the palette. Only an action is remembered; a rule is opened and forgotten. */
  async choose(item: PaletteItem): Promise<void> {
    this.paletteOpen = false;
    if (item.kind === 'rule') {
      this.view = 'rules';
      await this.openDetail(item.id);
      return;
    }
    this.frecency = bumped(this.frecency, item.action.key, this.env.now());
    safe(() => this.env.prefs.setItem(PREF_KEYS.frecency, JSON.stringify(this.frecency)));
    await item.action.run();
  }

  /** Show the file of a rule in the editor of the machine the hub runs on; the answer is a word in the toast. */
  async openInEditor(id: string): Promise<void> {
    try {
      await this.env.hub.open(id);
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) return;
      const unavailable = error instanceof ApiError && error.code === 'open_unavailable';
      this.notice = { key: unavailable ? 'notice.open_unavailable' : 'notice.open_failed' };
    }
  }

  dismissNotice(): void {
    this.notice = null;
  }

  // ── writing ─────────────────────────────────────────────────────────────

  /** A request is on its way: the page does not send a second one beside it. */
  writing = $state(false);
  /** The operations that met another version of the file, first first: the conflict center shows them; nothing was written. */
  conflicts = $state.raw<Conflict[]>([]);
  /** The rule as the file has it now, for the first conflict (read when the conflict arrived). */
  conflictRule = $state.raw<Body | null>(null);

  get conflict(): Conflict | null {
    return this.conflicts[0] ?? null;
  }
  private opCount = 0;

  // ── the waivers of the project of the device ─────────────────────────────

  /** What the waivers file of the project says; `null` until it was read. */
  waivers = $state.raw<Waivers | null>(null);
  /** `no_project`: the person set none (config.json); `failed`: the engine could not read the file. */
  waiversState = $state<'idle' | 'ready' | 'no_project' | 'failed'>('idle');
  /** The rule a "Waive …" button was pressed for: the form of the waivers view starts with its id. */
  waiveDraft = $state<string | null>(null);

  async loadWaivers(): Promise<void> {
    try {
      this.waivers = await this.env.hub.waivers();
      this.waiversState = 'ready';
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) return;
      this.waivers = null;
      this.waiversState = error instanceof ApiError && error.code === 'no_project' ? 'no_project' : 'failed';
    }
  }

  /** Waive a rule: go to the waivers with its id in the form. */
  startWaiver(id: string): void {
    this.waiveDraft = id;
    this.view = 'waivers';
  }

  /** The tag of the waivers file the person saw: what a change must be sent with. */
  waiversEtag(): string | null {
    return this.waivers?.etag ?? null;
  }

  /** The dialog for a new rule is open. */
  newRuleOpen = $state(false);

  openNewRule(): void {
    if (this.phase === 'ready') this.newRuleOpen = true;
  }

  closeNewRule(): void {
    this.newRuleOpen = false;
  }

  /** The next free id of a family, as the engine says it; `null` when it names none or cannot be asked. */
  async nextId(family: string): Promise<string | null> {
    try {
      return nextOf(await this.env.hub.nextId(family), family);
    } catch {
      return null;
    }
  }

  /** The tag of the file a rule lives in, as the snapshot gave it: what a change must be sent with. */
  etagOf(id: string): string | null {
    return this.rules.find((r) => r.id === id)?.etag ?? null;
  }

  /**
   * Ask the engine to change rules. The page never decides what a change is: it sends the operations, reads the
   * document the engine answers with, and shows what came of each. Every change goes with the tag the person saw
   * (`if_match`), so that nothing is written over a change made since. Returns what came of it, or `null` when the
   * hub refused the whole request (told in a notice).
   */
  async write(ops: Op[], options: { dryRun?: boolean; undoable?: boolean } = {}): Promise<Outcome | null> {
    if (this.phase !== 'ready' || this.writing || ops.length === 0) return null;
    const dryRun = options.dryRun === true;
    this.writing = true;
    try {
      this.opCount += 1;
      const result = await this.env.hub.ops({
        schema: 1,
        client_op_id: clientOpId(this.env.now(), this.opCount),
        dry_run: dryRun,
        ops,
      });
      const outcome = summarize(ops, result);
      if (dryRun) return outcome;
      this.reportWrite(outcome, options.undoable !== false);
      // The waivers file is not watched: read it again after a change of it, or after a clash (it says what it is now).
      if (ops.some(isWaiverOp)) await this.loadWaivers();
      if (outcome.saved > 0) await this.afterWrite(outcome.changedIds);
      return outcome;
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) return null;
      const key = error instanceof ApiError ? WRITE_KEYS[error.code] : undefined;
      this.notice = key
        ? { key }
        : { key: 'write.failed', params: { message: error instanceof Error ? error.message : 'failed' } };
      return null;
    } finally {
      this.writing = false;
    }
  }

  /** Say what came of a write in one notice; a conflict is also kept for the conflict center. */
  private reportWrite(outcome: Outcome, undoable: boolean): void {
    const first = outcome.conflicts[0];
    this.conflicts = outcome.conflicts;
    this.conflictRule = null;
    if (first) {
      if (!isWaiverOp(first.op)) void this.readConflictRule(first.id);
      this.notice = isWaiverOp(first.op)
        ? { key: 'write.conflict_waivers' }
        : { key: 'write.conflict', params: { id: first.id } };
    } else if (outcome.failures[0]) {
      this.notice = { key: 'write.failed', params: { message: outcome.failures[0].message || outcome.failures[0].code } };
    } else if (outcome.saved > 1) {
      this.notice = { key: 'write.saved_many', params: { n: String(outcome.saved) }, ...(undoable && outcome.undo.length ? { undo: outcome.undo } : {}) };
    } else if (outcome.saved === 1) {
      this.notice = { key: 'write.saved', ...(undoable && outcome.undo.length ? { undo: outcome.undo } : {}) };
    }
  }

  /** The hub has read the new state before it answered: read it here, and the rule that is open again. */
  private async afterWrite(changed: readonly string[]): Promise<void> {
    await this.refresh();
    const open = this.detail.state === 'none' ? null : this.detail.id;
    if (open === null || !changed.includes(open)) return;
    // A rule that left the rule set (retired) is not asked for: the hub would answer 404. Its detail just closes.
    if (this.rules.some((r) => r.id === open)) await this.loadDetail(open);
    else this.closeDetail();
  }

  /** Take back what the notice reports: its operations go as a request of their own, with no Undo for the undo. */
  async undo(): Promise<void> {
    const ops = this.notice?.undo;
    if (!ops || ops.length === 0) return;
    this.notice = null;
    const outcome = await this.write(ops, { undoable: false });
    if (outcome && outcome.saved > 0 && outcome.conflicts.length === 0 && outcome.failures.length === 0) {
      this.notice = { key: 'write.undone' };
    }
  }

  private async readConflictRule(id: string): Promise<void> {
    try {
      const doc = await this.env.hub.rule(id);
      if (this.conflict?.id === id) this.conflictRule = doc.rule;
    } catch {
      // The rule may be gone: the conflict center then shows only what was asked.
    }
  }

  /** Forget the first conflict (the person looked at it or went on); the next one, if any, comes up. */
  clearConflict(): void {
    this.conflicts = this.conflicts.slice(1);
    this.conflictRule = null;
    const next = this.conflict;
    if (next && !isWaiverOp(next.op)) void this.readConflictRule(next.id);
  }

  /**
   * What the person chose for the first conflict: write the same change onto the version the file has now (their
   * decision to write over the other change), look at the rule as it is, or drop their change.
   */
  async resolveConflict(choice: 'retry' | 'view' | 'drop'): Promise<void> {
    const conflict = this.conflict;
    if (!conflict) return;
    if (choice === 'retry') {
      const op = retryOp(conflict);
      this.clearConflict();
      if (op) await this.write([op]);
    } else if (choice === 'view') {
      this.clearConflict();
      if (isWaiverOp(conflict.op)) {
        this.view = 'waivers';
        await this.loadWaivers();
      } else {
        this.setView('rules');
        await this.openDetail(conflict.id);
      }
    } else {
      this.clearConflict();
    }
    if (this.notice?.key === 'write.conflict') this.notice = null;
  }

  // ── the marked rules (several at once) ──────────────────────────────────

  /** The ids the person marked in the list. */
  marked = $state.raw<ReadonlySet<string>>(new Set());

  /** May the page write this rule? A file of its own, in a rule set that has a class. */
  canWrite(rule: Pick<RuleSummary, 'source' | 'etag_scope'>): boolean {
    return rule.etag_scope === 'rule' && this.classOfRule(rule) !== 'unclassified';
  }

  toggleMark(id: string): void {
    const next = new Set(this.marked);
    if (!next.delete(id)) next.add(id);
    this.marked = next;
  }

  markShown(on: boolean): void {
    this.marked = on ? new Set(this.shown.filter((r) => this.canWrite(r)).map((r) => r.id)) : new Set();
  }

  clearMarks(): void {
    this.marked = new Set();
  }

  /** The marked rules the page may write, with the tag it saw for each. */
  private markedWritable(): { id: string; etag: string; severity: string }[] {
    return this.rules
      .filter((r) => this.marked.has(r.id) && this.canWrite(r))
      .map((r) => ({ id: r.id, etag: r.etag, severity: r.severity }));
  }

  /** One request for all marked rules: the same severity, or retired. A rule that is already so, or not writable, is left out. */
  async writeMarked(change: { severity: string } | { retire: true }): Promise<Outcome | null> {
    const rules = this.markedWritable();
    const ops =
      'retire' in change
        ? rules.map((r) => retireOp(r.id, r.etag))
        : rules.filter((r) => r.severity !== change.severity).map((r) => setOp(r.id, { severity: change.severity }, r.etag));
    if (ops.length === 0) {
      this.notice = { key: 'bulk.none' };
      return null;
    }
    const outcome = await this.write(ops);
    if (outcome && outcome.conflicts.length === 0 && outcome.failures.length === 0) this.clearMarks();
    return outcome;
  }
}

/** The words for the refusals of the hub that have their own (by the code of the error document). */
const WRITE_KEYS: Record<string, Key> = {
  write_refused: 'write.refused',
  rate_limited: 'write.rate',
  engine_unavailable: 'write.unavailable',
  not_found: 'write.not_held',
};

