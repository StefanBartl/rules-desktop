import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  App,
  EXCHANGE_RETRIES,
  GONE_SETTLE_MS,
  PREF_KEYS,
  RESCAN_EVERY_MS,
  SEARCH_DELAY_MS,
  SEARCH_RETRIES,
  SEARCH_RETRY_MS,
  TOKEN_KEY,
} from './app.svelte';
import { snapshotDoc } from './fixtures';
import { CODE, FakeHub, TOKEN, testEnv } from './testhub';

const apps: App[] = [];

/** An app on a fake hub, its tab paired already unless `paired` is false. */
function setup(paired = true) {
  const fake = new FakeHub();
  const t = testEnv(fake);
  if (paired) t.session.setItem(TOKEN_KEY, TOKEN);
  const app = new App(t.env);
  apps.push(app);
  return { fake, app, ...t };
}

/** Started, with the stream open. */
async function ready(paired = true) {
  const s = setup(paired);
  await s.app.start(null);
  await vi.waitFor(() => expect(s.fake.openStreams).toBe(1));
  await vi.waitFor(() => expect(s.app.connection).toBe('live'));
  return s;
}

afterEach(() => {
  for (const app of apps.splice(0)) app.stop();
});

describe('the start', () => {
  it('takes the pairing code out of the address, trades it and keeps the token for the tab only', async () => {
    const s = setup(false);
    const replaced: string[] = [];
    const history = { replaceState: (_d: unknown, _u: string, url?: string | URL | null) => void replaced.push(String(url)) };
    await s.app.startFrom({ hash: `#pair=${CODE}`, pathname: '/', search: '' }, history);
    expect(replaced).toEqual(['/']);
    expect(s.fake.count('/auth/exchange', 'POST')).toBe(1);
    expect(s.session.getItem(TOKEN_KEY)).toBe(TOKEN);
    expect(s.app.phase).toBe('ready');
    expect(s.app.snapshot?.rules).toHaveLength(4);
    expect(s.app.etag).toBe('sha256:9609c01610189534');
    expect(s.app.sources?.kind).toBe('hub.sources');
    expect(s.app.classOfRule({ source: 'Checklists/_rules/ERR/ERR-100.md' })).toBe('private');
    await vi.waitFor(() => expect(s.app.connection).toBe('live'));
    // The stream carries the token as a bearer, not in its address.
    const events = s.fake.asked.find((a) => a.path === '/api/rules/events');
    expect(events?.headers.get('Authorization')).toBe(`Bearer ${TOKEN}`);
    expect(events?.query.toString()).toBe('');
  });

  it('asks to be paired without a code and without a token, and asks the hub nothing', async () => {
    const s = setup(false);
    await s.app.start(null);
    expect(s.app.phase).toBe('needs_pairing');
    expect(s.app.pairRefused).toBe(false);
    expect(s.fake.asked).toEqual([]);
  });

  it('says so when the code was refused and the tab has no session', async () => {
    const s = setup(false);
    s.fake.codes.clear();
    await s.app.start(CODE);
    expect(s.app.phase).toBe('needs_pairing');
    expect(s.app.pairRefused).toBe(true);
  });

  it('trades a code again that the hub turned away for too many attempts, and does not call it used up', async () => {
    const s = setup(false);
    s.fake.failWith('/auth/exchange', 429);
    await s.app.start(CODE);
    expect(s.fake.count('/auth/exchange', 'POST')).toBe(2);
    expect(s.app.pairRefused).toBe(false);
    expect(s.app.phase).toBe('ready');
  });

  it('says to wait when the hub keeps turning the code away, not that it was used up', async () => {
    const s = setup(false);
    s.fake.failWith('/auth/exchange', 429, EXCHANGE_RETRIES + 1);
    await s.app.start(CODE);
    expect(s.fake.count('/auth/exchange', 'POST')).toBe(EXCHANGE_RETRIES + 1);
    expect(s.app.phase).toBe('needs_pairing');
    expect(s.app.pairRefused).toBe(false);
    expect(s.app.notice).toEqual({ key: 'notice.pair_rate_limited' });
  });

  it('does not call a code used up for a body the hub could not read', async () => {
    const s = setup(false);
    s.fake.failWith('/auth/exchange', 400);
    await s.app.start(CODE);
    expect(s.app.pairRefused).toBe(false);
  });

  it('keeps the session of the tab when a code is refused', async () => {
    const s = setup(true);
    s.fake.codes.clear();
    await s.app.start(CODE);
    expect(s.app.phase).toBe('ready');
    expect(s.app.pairRefused).toBe(false);
  });

  it('waits for an engine that is not ready, with its problem, and for a hub that is not there', async () => {
    const s = setup();
    s.fake.downFor = 1;
    s.fake.engineReady = false;
    let hellos = 0;
    const sleep = s.env.sleep;
    s.env.sleep = async (ms) => {
      hellos += 1;
      if (hellos === 1) expect(s.app.phase).toBe('offline');
      if (hellos === 2) {
        expect(s.app.phase).toBe('unavailable');
        expect(s.app.problem).toBe('no_engine');
        s.fake.engineReady = true;
      }
      await sleep(ms);
    };
    await s.app.start(null);
    expect(s.slept.slice(0, 2)).toEqual([3000, 5000]);
    expect(s.app.phase).toBe('ready');
    expect(s.app.problem).toBeNull();
  });
});

describe('a session that is gone', () => {
  it('turns a 401 into the pairing page and forgets the token and what was read', async () => {
    const s = await ready();
    s.fake.tokens.clear();
    await s.app.refresh();
    expect(s.app.phase).toBe('needs_pairing');
    expect(s.app.snapshot).toBeNull();
    expect(s.app.sources).toBeNull();
    expect(s.session.getItem(TOKEN_KEY)).toBeNull();
  });

  it('is told by a refused stream too', async () => {
    const s = setup();
    s.fake.streamStatus = 401;
    await s.app.start(null);
    await vi.waitFor(() => expect(s.app.phase).toBe('needs_pairing'));
    expect(s.session.getItem(TOKEN_KEY)).toBeNull();
  });
});

describe('the stream', () => {
  it('reads the snapshot with its tag on every rules event, also one with empty lists', async () => {
    const s = await ready();
    const before = s.fake.count('/api/rules/snapshot');
    s.fake.send('rules', { rev: 'rev:d62208', seq: 2, cause: 'fs', changed_ids: [], added: [], removed: [], truncated: false });
    await vi.waitFor(() => expect(s.fake.count('/api/rules/snapshot')).toBe(before + 1));
    const last = s.fake.asked.filter((a) => a.path === '/api/rules/snapshot').at(-1);
    expect(last?.headers.get('If-None-Match')).toBe('"sha256:9609c01610189534"');
  });

  it('reads the open rule again when its id changed', async () => {
    const s = await ready();
    await s.app.openDetail('DEP-01');
    expect(s.app.detail.state).toBe('ok');
    const rules = s.fake.count('/api/rules/rule');
    await s.app.onMessage({ event: 'rules', data: JSON.stringify({ changed_ids: ['DEP-09'], removed: [] }) });
    expect(s.fake.count('/api/rules/rule')).toBe(rules);
    await s.app.onMessage({ event: 'rules', data: JSON.stringify({ changed_ids: ['DEP-01', 'DEP-09'], removed: [] }) });
    expect(s.fake.count('/api/rules/rule')).toBe(rules + 1);
    expect(s.app.detail.state).toBe('ok');
  });

  it('keeps the open rule that is left out for one state, as a real hub sends it', async () => {
    const s = await ready();
    await s.app.openDetail('DEP-01');
    // As the hub sees it: the rule is out of its snapshot when it says `removed`, so it answers 404 for it (K8).
    s.fake.dropRule('DEP-01');
    const sleep = s.env.sleep;
    s.env.sleep = async (ms) => {
      // The next state brings it back (a retire or a save that was not done yet).
      if (ms === GONE_SETTLE_MS) s.fake.restore();
      await sleep(ms);
    };
    await s.app.onMessage({ event: 'rules', data: JSON.stringify({ changed_ids: [], removed: ['DEP-01'] }) });
    expect(s.slept).toContain(GONE_SETTLE_MS);
    expect(s.app.detail).toMatchObject({ state: 'ok', id: 'DEP-01' });
    expect(s.app.notice).toBeNull();
  });

  it('closes the open rule that is still gone after the pause, on a 404 of the hub, with a word', async () => {
    const s = await ready();
    await s.app.openDetail('DEP-01');
    s.fake.dropRule('DEP-01');
    const rules = s.fake.count('/api/rules/rule');
    await s.app.onMessage({ event: 'rules', data: JSON.stringify({ changed_ids: [], removed: ['DEP-01'] }) });
    expect(s.fake.count('/api/rules/rule')).toBe(rules + 1);
    expect(s.app.detail.state).toBe('none');
    expect(s.app.selectedId).toBeNull();
    expect(s.app.notice).toEqual({ key: 'detail.gone', params: { id: 'DEP-01' } });
  });

  it('closes an open rule that is gone on a resync or a truncated event as on a removal', async () => {
    for (const message of [
      { event: 'resync', data: '{}' },
      { event: 'rules', data: JSON.stringify({ changed_ids: [], removed: [], truncated: true }) },
    ]) {
      const s = await ready();
      await s.app.openDetail('LUA-01');
      s.fake.dropRule('LUA-01');
      await s.app.onMessage(message);
      expect(s.app.detail.state, message.event).toBe('none');
      expect(s.app.notice, message.event).toEqual({ key: 'detail.gone', params: { id: 'LUA-01' } });
    }
  });

  it('says live while the stream is open, also after a read of the snapshot failed', async () => {
    const s = await ready();
    s.fake.failWith('/api/rules/snapshot', 502);
    await s.app.refresh();
    expect(s.app.connection).toBe('live');
    await s.app.refresh();
    expect(s.app.connection).toBe('live');
    expect(s.fake.openStreams).toBe(1);
  });

  it('shows the rules when their classes cannot be read, all unclassified, and asks for the classes again', async () => {
    const s = setup();
    s.fake.failWith('/api/rules/sources', 502, 2);
    await s.app.start(null);
    expect(s.app.phase).toBe('ready');
    expect(s.app.rules).toHaveLength(4);
    expect(s.app.sources).toBeNull();
    for (const rule of s.app.rules) expect(s.app.classOfRule(rule), rule.id).toBe('unclassified');
    expect(s.app.notice).toEqual({ key: 'notice.sources_failed' });
    await vi.waitFor(() => expect(s.app.connection).toBe('live'));
    // The read on the open stream failed too; the next one, with the snapshot unchanged, gets them.
    await s.app.refresh();
    expect(s.app.sources?.kind).toBe('hub.sources');
    expect(s.app.classOfRule({ source: 'Checklists/_rules/ERR/ERR-100.md' })).toBe('private');
  });

  it('reads everything again on resync', async () => {
    const s = await ready();
    await s.app.openDetail('LUA-01');
    const [snap, src, rule] = ['/api/rules/snapshot', '/api/rules/sources', '/api/rules/rule'].map((p) => s.fake.count(p));
    await s.app.onMessage({ event: 'resync', data: '{}' });
    expect(s.fake.count('/api/rules/snapshot')).toBe(snap! + 1);
    expect(s.fake.count('/api/rules/sources')).toBe(src! + 1);
    expect(s.fake.count('/api/rules/rule')).toBe(rule! + 1);
  });

  it('reads again when the stream stands at another revision', async () => {
    const s = await ready();
    const before = s.fake.count('/api/rules/snapshot');
    await s.app.onMessage({ event: 'hello', data: JSON.stringify({ rev: 'rev:d62208', seq: 1 }) });
    expect(s.fake.count('/api/rules/snapshot')).toBe(before);
    await s.app.onMessage({ event: 'hello', data: JSON.stringify({ rev: 'rev:other', seq: 1 }) });
    expect(s.fake.count('/api/rules/snapshot')).toBe(before + 1);
  });

  it('reads the classes again only with a new snapshot', async () => {
    const s = await ready();
    const before = s.fake.count('/api/rules/sources');
    await s.app.refresh();
    expect(s.fake.count('/api/rules/sources')).toBe(before);
    s.fake.dropRule('LUA-01');
    await s.app.refresh();
    expect(s.fake.count('/api/rules/sources')).toBe(before + 1);
    expect(s.app.rules.map((r) => r.id)).not.toContain('LUA-01');
  });

  it('connects again after the hub closed it', async () => {
    const s = await ready();
    s.fake.closeStreams();
    await vi.waitFor(() => expect(s.fake.openStreams).toBe(1));
    expect(s.slept).toContain(1000);
  });
});

describe('the focus', () => {
  it('asks for a scan when the window comes back, at most every few seconds', async () => {
    const s = await ready();
    await s.app.focused();
    expect(s.fake.count('/api/rules/rescan', 'POST')).toBe(1);
    s.now.t += RESCAN_EVERY_MS - 1;
    await s.app.focused();
    expect(s.fake.count('/api/rules/rescan', 'POST')).toBe(1);
    s.now.t += 1;
    const snaps = s.fake.count('/api/rules/snapshot');
    await s.app.focused();
    expect(s.fake.count('/api/rules/rescan', 'POST')).toBe(2);
    expect(s.fake.count('/api/rules/snapshot')).toBe(snaps + 1);
  });
});

describe('the search', () => {
  it('searches the texts from two letters on, after a pause, and keeps the title hits', async () => {
    const s = await ready();
    await s.app.setQuery('f');
    expect(s.fake.count('/api/rules/search')).toBe(0);
    s.fake.searchIds = ['DEP-01'];
    await s.app.setQuery('one');
    expect(s.slept).toContain(SEARCH_DELAY_MS);
    expect(s.fake.count('/api/rules/search')).toBe(1);
    expect(s.app.searchHits?.status).toBe('ready');
    // DEP-01 by its title and by its text; nothing else.
    expect(s.app.shown.map((r) => r.id)).toEqual(['DEP-01']);
    s.fake.searchIds = ['ERR-100'];
    await s.app.setQuery('of');
    expect(s.app.shown.map((r) => r.id)).toEqual(['ERR-100']);
  });

  it('asks again while the hub is still reading the texts', async () => {
    const s = await ready();
    s.fake.searchStates = ['building', 'ready'];
    s.fake.searchIds = ['DEP-01'];
    await s.app.setQuery('text');
    expect(s.fake.count('/api/rules/search')).toBe(2);
    expect(s.slept).toContain(SEARCH_RETRY_MS);
    expect(s.app.searchHits?.status).toBe('ready');
    expect(s.app.shown.map((r) => r.id)).toEqual(['DEP-01']);
  });

  it('stops asking a hub that keeps building after a few times', async () => {
    const s = await ready();
    s.fake.searchStates = Array<'building'>(SEARCH_RETRIES + 5).fill('building');
    await s.app.setQuery('text');
    expect(s.fake.count('/api/rules/search')).toBe(SEARCH_RETRIES + 1);
    expect(s.app.searchHits?.status).toBe('building');
  });

  it('says when the hub found the words in more rules than it named', async () => {
    const s = await ready();
    s.fake.searchIds = ['DEP-01'];
    await s.app.setQuery('text');
    expect(s.app.searchHits).toMatchObject({ total: 1, cut: false });
    s.fake.searchTotal = 120;
    await s.app.setQuery('texts');
    expect(s.app.searchHits).toMatchObject({ total: 120, cut: true });
  });

  it('asks only for the last words when they come quickly', async () => {
    const s = await ready();
    let release = (): void => undefined;
    const gate = new Promise<void>((r) => (release = r));
    s.env.sleep = () => gate;
    const first = s.app.setQuery('fu');
    const second = s.app.setQuery('fun');
    release();
    await Promise.all([first, second]);
    const asked = s.fake.asked.filter((a) => a.path === '/api/rules/search').map((a) => a.query.get('q'));
    expect(asked).toEqual(['fun']);
  });
});

describe('the keyboard', () => {
  it('moves the cursor through the shown rules, opens, and closes the detail before the selection', async () => {
    const s = await ready();
    s.app.command({ kind: 'view', view: 'rules' });
    s.app.command({ kind: 'move', delta: 1 });
    s.app.command({ kind: 'move', delta: 1 });
    expect(s.app.selectedId).toBe('DEP-09');
    // A move reads no rule: each read is a question to the engine.
    expect(s.fake.count('/api/rules/rule')).toBe(0);
    s.app.command({ kind: 'open' });
    await vi.waitFor(() => expect(s.app.detail.state).toBe('ok'));
    s.app.command({ kind: 'palette' });
    s.app.command({ kind: 'escape' });
    expect(s.app.paletteOpen).toBe(false);
    expect(s.app.detail.state).toBe('ok');
    s.app.command({ kind: 'escape' });
    expect(s.app.detail.state).toBe('none');
    expect(s.app.selectedId).toBe('DEP-09');
    s.app.command({ kind: 'escape' });
    expect(s.app.selectedId).toBeNull();
  });

  it('opens the rules of a family from the family table', async () => {
    const s = await ready();
    s.app.command({ kind: 'view', view: 'families' });
    s.app.command({ kind: 'move', delta: 1 });
    s.app.command({ kind: 'move', delta: 1 });
    expect(s.app.selectedFamily).toBe('ERR');
    s.app.command({ kind: 'open' });
    expect(s.app.view).toBe('rules');
    expect(s.app.shown.map((r) => r.id)).toEqual(['ERR-100']);
  });

  it('sends the search key to the search box of the rules', async () => {
    const s = await ready();
    s.app.command({ kind: 'search' });
    expect(s.app.view).toBe('rules');
    expect(s.app.searchFocus).toBe(1);
  });
});

describe('what the browser keeps', () => {
  it('is the token in the tab and the settings, never a rule', async () => {
    const s = await ready();
    s.app.setLang('de');
    s.app.setTheme('dark');
    s.app.setShortcuts(false);
    await s.app.choose({ kind: 'rule', key: 'rule:LUA-01', label: 'x', hint: 'LUA-01', id: 'LUA-01', score: 1 });
    expect(s.app.detail).toMatchObject({ state: 'ok', id: 'LUA-01' });
    let ran = 0;
    const action = { key: 'rescan', label: 'Read again', run: () => void (ran += 1) };
    await s.app.choose({ kind: 'action', key: 'action:rescan', label: action.label, hint: '', action, score: 1 });
    expect(ran).toBe(1);
    expect([...s.session.items.keys()]).toEqual([TOKEN_KEY]);
    expect([...s.prefs.items.keys()].sort()).toEqual(Object.values(PREF_KEYS).sort());
    const kept = [...s.prefs.items.values(), ...s.session.items.values()].join('\n');
    for (const rule of snapshotDoc().rules) {
      expect(kept).not.toContain(rule.id);
      expect(kept).not.toContain(rule.source);
    }
    // A new page reads the settings back.
    const again = new App(s.env);
    expect([again.lang, again.theme, again.shortcuts]).toEqual(['de', 'dark', false]);
    expect(Object.keys(again.frecency)).toEqual(['action:rescan']);
  });
});
