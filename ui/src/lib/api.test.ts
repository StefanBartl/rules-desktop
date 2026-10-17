import { describe, expect, it } from 'vitest';
import { ApiError, Hub } from './api';
import { goldenText } from './fixtures';

interface Sent {
  url: string;
  init: RequestInit;
  headers: Headers;
}

/** A fetch that answers each call with the next response and keeps what was sent. */
function recording(answers: (() => Response | Promise<Response>)[]): { fetchFn: typeof fetch; sent: Sent[] } {
  const sent: Sent[] = [];
  const fetchFn = (async (url: unknown, init: RequestInit = {}) => {
    sent.push({ url: String(url), init, headers: new Headers(init.headers) });
    const answer = answers.shift();
    return answer ? answer() : json({});
  }) as typeof fetch;
  return { fetchFn, sent };
}

function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(typeof body === 'string' ? body : JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json', ...headers },
  });
}

describe('the hub client', () => {
  it('sends the session as a bearer token, without cache, cookie or referrer', async () => {
    const { fetchFn, sent } = recording([() => json(goldenText('snapshot'), 200, { ETag: '"sha256:9609c01610189534"' })]);
    const hub = new Hub(fetchFn);
    hub.setToken('tok');
    const result = await hub.snapshot(null);
    expect(sent[0]?.url).toBe('/api/rules/snapshot');
    expect(sent[0]?.headers.get('Authorization')).toBe('Bearer tok');
    expect(sent[0]?.headers.get('If-None-Match')).toBeNull();
    expect(sent[0]?.init.cache).toBe('no-store');
    expect(sent[0]?.init.credentials).toBe('omit');
    expect(sent[0]?.init.referrerPolicy).toBe('no-referrer');
    expect(result.kind).toBe('fresh');
    if (result.kind === 'fresh') {
      expect(result.etag).toBe('sha256:9609c01610189534');
      expect(result.doc.rules).toHaveLength(4);
    }
  });

  it('asks with the tag it holds and takes a 304 as not modified', async () => {
    const { fetchFn, sent } = recording([() => new Response(null, { status: 304 })]);
    const hub = new Hub(fetchFn);
    hub.setToken('tok');
    expect(await hub.snapshot('sha256:abc')).toEqual({ kind: 'not_modified' });
    expect(sent[0]?.headers.get('If-None-Match')).toBe('"sha256:abc"');
  });

  it('reads a weak tag without its marks', async () => {
    const { fetchFn } = recording([() => json(goldenText('snapshot'), 200, { ETag: 'W/"x1"' })]);
    const result = await new Hub(fetchFn).snapshot(null);
    expect(result.kind === 'fresh' && result.etag).toBe('x1');
  });

  it('escapes the id of a rule, every character that is not plain', async () => {
    const { fetchFn, sent } = recording([() => json(goldenText('rule'))]);
    const doc = await new Hub(fetchFn).rule('a/b+c d&id=x');
    expect(sent[0]?.url).toBe('/api/rules/rule?id=a%2Fb%2Bc%20d%26id%3Dx');
    expect(doc.rule.id).toBe('DEP-01');
  });

  it('turns a refusal into its code and message', async () => {
    const { fetchFn } = recording([
      () => json({ error: { code: 'not_found', message: 'no rule with this id' } }, 404),
      () => new Response('gateway', { status: 502 }),
    ]);
    const hub = new Hub(fetchFn);
    const first = await hub.rule('NOPE').catch((e: unknown) => e);
    expect(first).toBeInstanceOf(ApiError);
    expect(first).toMatchObject({ status: 404, code: 'not_found', message: 'no rule with this id' });
    // A body that is not the hub's keeps the status and the general words.
    await expect(hub.sources()).rejects.toMatchObject({ status: 502, code: 'http_error' });
  });

  it('trades a code without a token and refuses an answer without one', async () => {
    const { fetchFn, sent } = recording([() => json({ token: 'fresh' }), () => json({})]);
    const hub = new Hub(fetchFn);
    hub.setToken('old');
    expect(await hub.exchange('ab'.repeat(16))).toBe('fresh');
    expect(sent[0]?.url).toBe('/auth/exchange');
    expect(sent[0]?.init.method).toBe('POST');
    expect(sent[0]?.headers.get('Authorization')).toBeNull();
    expect(sent[0]?.init.body).toBe(JSON.stringify({ code: 'ab'.repeat(16) }));
    await expect(hub.exchange('cd'.repeat(16))).rejects.toMatchObject({ code: 'bad_reply' });
  });

  it('reads hello, about, search and rescan at their routes', async () => {
    const { fetchFn, sent } = recording([
      () => json({ instance: 'i', version: '0.1.0', engine: { ready: false, problem: 'no_vault' } }),
      () => json({ app: 'rules-desktop', version: '0.1.0', engine: null, engine_problem: 'engine_timeout', extra: 1 }),
      () => json({ status: 'ready', indexed: 4, total: 1, hits: [] }),
      () => new Response(null, { status: 202 }),
    ]);
    const hub = new Hub(fetchFn, 'http://127.0.0.1:17616');
    expect(await hub.hello()).toEqual({
      instance: 'i',
      version: '0.1.0',
      engine: { ready: false, problem: 'no_vault', capabilities: null },
    });
    expect(await hub.about()).toEqual({
      app: 'rules-desktop',
      version: '0.1.0',
      engine: null,
      engineProblem: 'engine_timeout',
    });
    expect((await hub.search('ä b')).status).toBe('ready');
    await hub.rescan();
    expect(sent.map((s) => s.url)).toEqual([
      'http://127.0.0.1:17616/api/rules/hello',
      'http://127.0.0.1:17616/api/rules/about',
      'http://127.0.0.1:17616/api/rules/search?q=%C3%A4%20b&limit=50',
      'http://127.0.0.1:17616/api/rules/rescan',
    ]);
    expect(sent[3]?.init.method).toBe('POST');
    expect(hub.eventsUrl).toBe('http://127.0.0.1:17616/api/rules/events');
  });

  it('posts the id of a rule to open and nothing else', async () => {
    const { fetchFn, sent } = recording([() => json({ ok: true }), () => json({ error: { code: 'open_failed' } }, 502)]);
    const hub = new Hub(fetchFn);
    hub.setToken('tok');
    await hub.open('DEP-01');
    expect(sent[0]?.url).toBe('/api/rules/open');
    expect(sent[0]?.init.method).toBe('POST');
    expect(JSON.parse(String(sent[0]?.init.body))).toEqual({ id: 'DEP-01' });
    await expect(hub.open('DEP-01')).rejects.toMatchObject({ status: 502, code: 'open_failed' });
  });

  it('takes only the three words of the engine in about', async () => {
    const { fetchFn } = recording([
      () => json({ app: 'rules-desktop', version: '1', engine: { version: '0.1.0', git: 7, nvim: 'v0.12.2', path: '/x' } }),
    ]);
    expect((await new Hub(fetchFn).about()).engine).toEqual({ version: '0.1.0', git: null, nvim: 'v0.12.2' });
  });
});

describe('the hub client and a refusal of the session', () => {
  it('tells the page the session is gone when the token sent is the one it holds', async () => {
    const { fetchFn } = recording([() => json({ error: { code: 'unauthorized' } }, 401)]);
    const hub = new Hub(fetchFn);
    hub.setToken('old');
    let told = 0;
    hub.onUnauthorized = () => (told += 1);
    await expect(hub.sources()).rejects.toMatchObject({ status: 401 });
    expect(told).toBe(1);
  });

  it('ignores a late refusal of a question asked with a token no longer held', async () => {
    let release = (): void => undefined;
    const gate = new Promise<void>((r) => (release = r));
    const { fetchFn, sent } = recording([
      async () => {
        await gate;
        return json({}, 401);
      },
    ]);
    const hub = new Hub(fetchFn);
    hub.setToken('old');
    let told = 0;
    hub.onUnauthorized = () => (told += 1);
    const late = hub.sources().catch(() => undefined);
    hub.setToken('fresh');
    release();
    await late;
    expect(sent[0]?.headers.get('Authorization')).toBe('Bearer old');
    expect(told).toBe(0);
  });
});
