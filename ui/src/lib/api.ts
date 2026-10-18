// The hub's routes, as typed calls. Only the shapes the page needs. Every request carries the session as a bearer
// token and nothing else: no cookie, no cache, no referrer.

import type { NextId, Result, Rule, Rulesets, Snapshot, Waivers } from './contract';
import type { OpsRequest } from './write';

/** Where the routes of this app live (`IDENTITY.route` of rules-hub). */
export const API = '/api/rules';

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

/** `GET /api/rules/hello` of the core: the hub and whether its engine is ready. Readable without a session. */
export interface Hello {
  instance: string;
  version: string;
  engine: { ready: boolean; problem: string | null; capabilities: string[] | null };
}

/** `GET /api/rules/about`: the build and the engine that answers (only the three words of its `hello`). */
export interface About {
  app: string;
  version: string;
  engine: { version: string | null; git: string | null; nvim: string | null } | null;
  /** A short word for why the engine did not answer; `null` when it did or there is none. */
  engineProblem: string | null;
}

/** The class a file is shown with on this device (`sources.json`); `unclassified` is not released. */
export type DataClass = 'public' | 'internal' | 'private' | 'unclassified';

/** `GET /api/rules/sources`: every file of the held snapshot with its class, worked out by the hub. */
export interface HubSources {
  schema: 1;
  kind: 'hub.sources';
  files: { path: string; data_class: DataClass; rules: number; key: string | null }[];
  keys: { key: string; class: Exclude<DataClass, 'unclassified'>; files: number }[];
  /** The class of a file no key names; none: such a file is not classified. */
  default: Exclude<DataClass, 'unclassified'> | null;
  unclassified: string[];
  private: string[];
}

/** A change of a data class: one target (a key, or the default) and the class; `class` null takes it away. */
export type ClassChange = (
  | { key: string }
  | { default: true }
) & {
  class: Exclude<DataClass, 'unclassified'> | null;
  confirm_loosen?: boolean;
};

/** A piece of a rule's text around a match; `marks` are `[start, end)` in UTF-16 units of `text`. */
export interface SearchSnippet {
  text: string;
  marks: [number, number][];
  more_before: boolean;
  more_after: boolean;
}

export interface SearchHit {
  id: string;
  score: number;
  snippet: SearchSnippet;
}

/** `GET /api/rules/search`: the answer of the kit's search index, as in tasks. */
export interface SearchResult {
  /** `building`: the hub has not read the texts yet; `failed`: it could not. */
  status: 'ready' | 'building' | 'failed';
  indexed: number;
  partial?: boolean;
  total: number;
  hits: SearchHit[];
}

export type SnapshotResult = { kind: 'fresh'; doc: Snapshot; etag: string | null } | { kind: 'not_modified' };

/** Hits asked for at most. */
export const SEARCH_LIMIT = 50;

const PLAIN: Pick<RequestInit, 'cache' | 'credentials' | 'referrerPolicy'> = {
  cache: 'no-store',
  credentials: 'omit',
  referrerPolicy: 'no-referrer',
};

export class Hub {
  private token: string | null = null;
  /** Called when the hub says the session is gone. */
  onUnauthorized: () => void = () => {};

  constructor(
    private readonly fetchFn: typeof fetch,
    private readonly base: string = '',
  ) {}

  setToken(token: string | null): void {
    this.token = token;
  }

  get hasToken(): boolean {
    return this.token !== null;
  }

  /** The session token, for the request the stream reader makes by itself. */
  get bearer(): string {
    return this.token ?? '';
  }

  url(path: string): string {
    return this.base + path;
  }

  private async request(path: string, init: RequestInit = {}): Promise<Response> {
    const headers = new Headers(init.headers);
    const sent = this.token;
    if (sent) headers.set('Authorization', `Bearer ${sent}`);
    const response = await this.fetchFn(this.url(path), { ...init, headers, ...PLAIN });
    // Only a refusal of the token that is still held means the session is gone: a late answer to a question asked
    // with an older token says nothing about the new one.
    if (response.status === 401 && this.token === sent) this.onUnauthorized();
    return response;
  }

  private async json<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await this.request(path, init);
    if (!response.ok) throw await failure(response);
    return (await response.json()) as T;
  }

  async hello(): Promise<Hello> {
    const doc = await this.json<{
      instance: string;
      version: string;
      engine: { ready: boolean; problem: string | null; capabilities?: string[] | null };
    }>(`${API}/hello`);
    return {
      instance: doc.instance,
      version: doc.version,
      engine: { ready: doc.engine.ready, problem: doc.engine.problem, capabilities: doc.engine.capabilities ?? null },
    };
  }

  /** Trade a pairing code for a session token. */
  async exchange(code: string): Promise<string> {
    const response = await this.fetchFn(this.url('/auth/exchange'), {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code }),
      ...PLAIN,
    });
    if (!response.ok) throw await failure(response);
    const doc = (await response.json()) as { token?: unknown };
    if (typeof doc.token !== 'string') throw new ApiError(502, 'bad_reply', 'no token');
    return doc.token;
  }

  /** The snapshot; with the tag of the one held, a 304 says it is still the current one. */
  async snapshot(etag: string | null): Promise<SnapshotResult> {
    const headers: Record<string, string> = {};
    if (etag) headers['If-None-Match'] = `"${etag}"`;
    const response = await this.request(`${API}/snapshot`, { headers });
    if (response.status === 304) return { kind: 'not_modified' };
    if (!response.ok) throw await failure(response);
    const raw = response.headers.get('etag');
    const tag = raw ? raw.replace(/^W\//, '').replace(/^"|"$/g, '') : null;
    return { kind: 'fresh', doc: (await response.json()) as Snapshot, etag: tag };
  }

  /** One rule with its text. A `signal` that aborts drops the request. */
  rule(id: string, signal?: AbortSignal): Promise<Rule> {
    return this.json<Rule>(`${API}/rule?id=${encodeURIComponent(id)}`, signal ? { signal } : undefined);
  }

  sources(): Promise<HubSources> {
    return this.json<HubSources>(`${API}/sources`);
  }

  /** The rules whose text holds the words (the hub searches the texts it keeps). */
  search(query: string, signal?: AbortSignal): Promise<SearchResult> {
    const path = `${API}/search?q=${encodeURIComponent(query)}&limit=${SEARCH_LIMIT}`;
    return this.json<SearchResult>(path, signal ? { signal } : undefined);
  }

  /** Ask the hub to look at the files again (the window got the focus back). */
  async rescan(): Promise<void> {
    const response = await this.request(`${API}/rescan`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{}',
    });
    if (!response.ok) throw await failure(response);
  }

  /** Show the file of a rule in Neovim, at its line (the hub starts the editor on its own machine). */
  async open(id: string): Promise<void> {
    const response = await this.request(`${API}/open`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id }),
    });
    if (!response.ok) throw await failure(response);
  }

  /**
   * Ask the engine to change rules (`rules-ops/1`) through the hub. A refusal of the whole request is an
   * `ApiError` (`write_refused` for a rule set without a class, `bad_request`, `rate_limited` ...); a part that
   * failed is told per operation in the document, which the page reads (`write.ts`).
   */
  async ops(request: OpsRequest): Promise<Result> {
    return this.json<Result>(`${API}/ops`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(request),
    });
  }

  /**
   * Set the class of a key of `sources.json`, take the key away (`class` null), or do the same for the default. The
   * hub refuses a change that releases more than before (`confirm_needed`, 409) until `confirm_loosen` is true.
   */
  async setClass(change: ClassChange): Promise<void> {
    const response = await this.request(`${API}/sources/class`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(change),
    });
    if (!response.ok) throw await failure(response);
  }

  /** The waivers of the project of this device; a 409 `no_project` when the person set none. */
  waivers(): Promise<Waivers> {
    return this.json<Waivers>(`${API}/waivers`);
  }

  /** The rule sets that are loaded (an empty one too) and the ones in the archive of the vault. */
  rulesets(): Promise<Rulesets> {
    return this.json<Rulesets>(`${API}/rulesets`);
  }

  /** The next free id of a family, or of every family: the engine says it, nothing is reserved. */
  nextId(family?: string): Promise<NextId> {
    const query = family ? `?family=${encodeURIComponent(family)}` : '';
    return this.json<NextId>(`${API}/next_id${query}`);
  }

  /** What this build is and which engine answers: only the words the page shows are taken. */
  async about(): Promise<About> {
    const doc = await this.json<Record<string, unknown>>(`${API}/about`);
    const text = (v: unknown): string | null => (typeof v === 'string' ? v : null);
    const engine = doc['engine'];
    const part = (key: string): string | null => text((engine as Record<string, unknown>)[key]);
    return {
      app: text(doc['app']) ?? 'rules-desktop',
      version: text(doc['version']) ?? '',
      engine:
        typeof engine === 'object' && engine !== null && !Array.isArray(engine)
          ? { version: part('version'), git: part('git'), nvim: part('nvim') }
          : null,
      engineProblem: text(doc['engine_problem']),
    };
  }

  /** The address of the event stream. */
  get eventsUrl(): string {
    return this.url(`${API}/events`);
  }
}

/** The refusal of the hub as an error: its code and message, or the status when the body is not the hub's. */
export async function failure(response: Response): Promise<ApiError> {
  let code = 'http_error';
  let message = `HTTP ${response.status}`;
  try {
    const doc = (await response.json()) as { error?: { code?: unknown; message?: unknown } };
    if (typeof doc.error?.code === 'string') code = doc.error.code;
    if (typeof doc.error?.message === 'string') message = doc.error.message;
  } catch {
    // not JSON: keep the generic words
  }
  return new ApiError(response.status, code, message);
}
