// Test helper: a hub in a fetch function. It answers the routes of rules-hub with the golden documents, counts what
// it was asked, and holds the event stream open until the test writes to it or closes it.

import { Hub } from './api';
import type { Env } from './app.svelte';
import { errorDoc, goldenText, hubSourcesDoc, snapshotDoc } from './fixtures';

export const CODE = 'ab'.repeat(16);
export const TOKEN = 'token-1';

export class MemoryStorage {
  readonly items = new Map<string, string>();
  getItem(key: string): string | null {
    return this.items.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.items.set(key, value);
  }
  removeItem(key: string): void {
    this.items.delete(key);
  }
}

export interface Asked {
  method: string;
  path: string;
  query: URLSearchParams;
  headers: Headers;
}

export class FakeHub {
  asked: Asked[] = [];
  engineReady = true;
  /** The next hellos fail as a hub that is not there does. */
  downFor = 0;
  /** Codes the hub takes. */
  codes = new Set([CODE]);
  /** Tokens the hub takes; a request with another one is a 401. */
  tokens = new Set([TOKEN]);
  snapshotText = goldenText('snapshot');
  etag = 'sha256:9609c01610189534';
  /** Ids `rule` answers 404 for although they are in the snapshot. */
  goneIds = new Set<string>();
  searchIds: string[] = [];
  /** The stream is refused with this status (0: it is opened). */
  streamStatus = 0;
  private streams: ReadableStreamDefaultController<Uint8Array>[] = [];

  readonly fetchFn: typeof fetch = async (input, init = {}) => {
    const url = new URL(String(input), 'http://hub.test');
    const headers = new Headers(init.headers);
    const method = init.method ?? 'GET';
    this.asked.push({ method, path: url.pathname, query: url.searchParams, headers });
    const token = (headers.get('Authorization') ?? '').replace(/^Bearer /, '');
    if (url.pathname === '/api/rules/hello') {
      if (this.downFor > 0) {
        this.downFor -= 1;
        throw new TypeError('network');
      }
      return json({
        schema: 1,
        kind: 'hub.hello',
        app: 'rules-desktop',
        version: '0.1.0',
        instance: 'i1',
        engine: { ready: this.engineReady, problem: this.engineReady ? null : 'no_engine', capabilities: null },
      });
    }
    if (url.pathname === '/auth/exchange') {
      const { code } = JSON.parse(String(init.body)) as { code: string };
      if (!this.codes.delete(code)) return json({ error: { code: 'bad_code', message: 'no' } }, 401);
      return json({ token: TOKEN });
    }
    if (!this.tokens.has(token)) return json({ error: { code: 'unauthorized', message: 'no session' } }, 401);
    switch (url.pathname) {
      case '/api/rules/snapshot': {
        if (headers.get('If-None-Match') === `"${this.etag}"`) return new Response(null, { status: 304 });
        return json(this.snapshotText, 200, { ETag: `"${this.etag}"` });
      }
      case '/api/rules/sources':
        return json(hubSourcesDoc());
      case '/api/rules/rule': {
        const id = url.searchParams.get('id') ?? '';
        const held = (JSON.parse(this.snapshotText) as { rules: { id: string }[] }).rules.some((r) => r.id === id);
        if (!held || this.goneIds.has(id)) return json({ error: { code: 'not_found', message: errorDoc().message } }, 404);
        const doc = JSON.parse(goldenText('rule')) as { rule: { id: string } };
        doc.rule.id = id;
        return json(doc);
      }
      case '/api/rules/search':
        return json({
          status: 'ready',
          indexed: 4,
          total: this.searchIds.length,
          hits: this.searchIds.map((id) => ({
            id,
            score: 1,
            snippet: { text: '', marks: [], more_before: false, more_after: false },
          })),
        });
      case '/api/rules/rescan':
        return new Response(null, { status: 202 });
      case '/api/rules/events': {
        if (this.streamStatus !== 0) return new Response(null, { status: this.streamStatus });
        let controller: ReadableStreamDefaultController<Uint8Array> | null = null;
        const body = new ReadableStream<Uint8Array>({ start: (c) => void (controller = c) });
        const opened = controller as ReadableStreamDefaultController<Uint8Array> | null;
        if (opened) this.streams.push(opened);
        init.signal?.addEventListener('abort', () => {
          this.streams = this.streams.filter((c) => c !== opened);
          try {
            opened?.error(new DOMException('aborted', 'AbortError'));
          } catch {
            // closed already
          }
        });
        return new Response(body, { status: 200, headers: { 'Content-Type': 'text/event-stream' } });
      }
    }
    return json({ error: { code: 'not_found', message: 'no such route' } }, 404);
  };

  /** Write an event to every open stream. */
  send(event: string, data: unknown): void {
    const bytes = new TextEncoder().encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
    for (const c of this.streams) c.enqueue(bytes);
  }

  get openStreams(): number {
    return this.streams.length;
  }

  /** The hub closes every stream (as when it stops). */
  closeStreams(): void {
    for (const c of this.streams) c.close();
    this.streams = [];
  }

  count(path: string, method = 'GET'): number {
    return this.asked.filter((a) => a.path === path && a.method === method).length;
  }

  /** The snapshot without one rule, under a new tag. */
  dropRule(id: string): void {
    const doc = snapshotDoc();
    doc.rules = doc.rules.filter((r) => r.id !== id);
    this.snapshotText = JSON.stringify(doc);
    this.etag = `${this.etag}-${id}`;
  }
}

function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(typeof body === 'string' ? body : JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json', ...headers },
  });
}

/** An environment around a fake hub; `sleep` returns at once and keeps what it was asked to wait. */
export function testEnv(fake: FakeHub, now: { t: number } = { t: 1_800_000_000_000 }) {
  const session = new MemoryStorage();
  const prefs = new MemoryStorage();
  const slept: number[] = [];
  const env: Env = {
    hub: new Hub(fake.fetchFn),
    session,
    prefs,
    preferredLanguages: ['en'],
    fetchFn: fake.fetchFn,
    sleep: async (ms) => {
      slept.push(ms);
    },
    now: () => now.t,
  };
  return { env, session, prefs, slept, now };
}
