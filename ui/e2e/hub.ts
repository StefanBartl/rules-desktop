// A real hub for the browser tests: the binary of this repository, the real engine (a real Neovim and rules.nvim),
// a copy of the synthetic fixture vault with hostile rules added. Nothing of a real rule set is ever used.

import { execFileSync, spawn, type ChildProcess } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
export const PORT = 17697;
export const ORIGIN = `http://127.0.0.1:${PORT}`;

const exe = join(repo, 'target', 'debug', process.platform === 'win32' ? 'rules-hub.exe' : 'rules-hub');

/** Titles and texts an attacker could write into a rule set (a rule set is somebody else's text). */
export const HOSTILE: { id: string; title: string; body: string; severity?: string; extra?: string }[] = [
  { id: 'HX-01', title: '"><img src=x onerror=window.__xss=1>', body: 'plain text' },
  { id: 'HX-02', title: '<svg onload=window.__xss=2>', body: 'plain text' },
  {
    id: 'HX-03',
    title: 'Text with raw html',
    body: [
      '<script>window.__xss=3</script>',
      '',
      '<img src=x onerror="window.__xss=4">',
      '',
      '<iframe src="javascript:window.__xss=5"></iframe>',
      '',
      '<a href="javascript:window.__xss=6">raw anchor</a>',
      '',
      '<details open ontoggle="window.__xss=7">d</details>',
    ].join('\n'),
  },
  {
    id: 'HX-04',
    title: 'Links in the text',
    body: ['[js](javascript:window.__xss=8)', '', '![img](https://tracker.invalid/pixel.png)', '', '[ok](https://example.org/page)'].join('\n'),
  },
  {
    id: 'HX-05',
    title: 'A check and a question with markup',
    body: 'fields',
    extra: 'check = { type = "grep", include = "%.lua$", pattern = "<img src=x onerror=window.__xss=9>" },',
  },
  { id: 'HX-06', title: 'Invoice \u202Etxt.exe\u202C \u200Bready', body: 'a\u202Ereversed' },
  { id: 'HX-07', title: 'Long ' + 'x'.repeat(300), body: 'y'.repeat(600) },
];

function hostileFile(): string {
  const blocks = HOSTILE.map((h) =>
    [
      `#### \`${h.id}\` - ${h.title}`,
      '',
      '```rule',
      `id = "${h.id}",`,
      `severity = "${h.severity ?? 'recommended'}",`,
      ...(h.extra ? [h.extra] : []),
      '```',
      '',
      h.body,
      '',
    ].join('\n'),
  );
  return ['# Hostile rules', '', ...blocks].join('\n');
}

/** Is the binary the one of the current build of the interface? */
export async function assertFreshBinary(): Promise<void> {
  const dist = join(repo, 'ui', 'dist', 'index.html');
  if (!existsSync(dist)) throw new Error('ui/dist is not built: run `pnpm build` and `cargo build`');
  const served = await (await fetch(`${ORIGIN}/`)).text();
  if (served.trim() !== readFileSync(dist, 'utf8').trim()) {
    throw new Error('the hub binary holds an older interface: run `cargo build` after `pnpm build`');
  }
}

export interface Running {
  child: ChildProcess;
  config: string;
  vault: string;
  stop(): void;
}

export function prepareVault(): { root: string; vault: string; config: string } {
  const root = mkdtempSync(join(tmpdir(), 'rules-desktop-e2e-'));
  const vault = join(root, 'vault');
  cpSync(join(repo, 'crates', 'rules-hub', 'tests', 'fixtures', 'vault'), vault, { recursive: true });
  writeFileSync(join(vault, 'rulesets', 'alpha', 'regeln', 'hostile.md'), hostileFile());
  const config = join(root, 'config');
  mkdirSync(config);
  writeFileSync(join(config, 'config.json'), JSON.stringify({ vault }));
  writeFileSync(
    join(config, 'sources.json'),
    JSON.stringify({ schema: 1, sources: { alpha: { class: 'public' }, beta: { class: 'internal' } } }),
  );
  return { root, vault, config };
}

export function env(config: string): NodeJS.ProcessEnv {
  return {
    ...process.env,
    RULES_DESKTOP_CONFIG_DIR: config,
    // The editor is not started in a test: the file it would open is written here.
    RULES_DESKTOP_EDITOR_LOG: join(config, '..', 'editor.log'),
    RULES_DESKTOP_PORT: String(PORT),
  };
}

export async function startHub(): Promise<Running & { root: string }> {
  // Something already answers on the port: those tests would run against another hub and another vault.
  const taken = await fetch(`${ORIGIN}/api/rules/hello`).then(
    () => true,
    () => false,
  );
  if (taken) throw new Error(`port ${PORT} is in use: stop the hub that runs there`);
  const { root, vault, config } = prepareVault();
  const child = spawn(exe, ['serve'], { env: env(config), stdio: 'ignore' });
  const deadline = Date.now() + 30_000;
  for (;;) {
    try {
      const r = await fetch(`${ORIGIN}/api/rules/hello`);
      if (r.ok) {
        const doc = (await r.json()) as { engine: { ready: boolean } };
        if (doc.engine.ready) break;
        throw new Error('the engine is not ready: set RULES_ENGINE_DIR and LIB_NVIM_DIR');
      }
    } catch (error) {
      if (Date.now() > deadline || (error instanceof Error && error.message.includes('engine'))) {
        child.kill();
        throw error;
      }
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  // The port answers before `hub.json` is written, and `rules-hub open` (the pairing of the tests) needs that file.
  while (!existsSync(join(config, 'hub.json'))) {
    if (Date.now() > deadline) {
      child.kill();
      throw new Error('the hub answers but wrote no hub.json');
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  try {
    await assertFreshBinary();
  } catch (error) {
    child.kill();
    throw error;
  }
  return {
    child,
    config,
    vault,
    root,
    stop() {
      child.kill();
      try {
        rmSync(root, { recursive: true, force: true });
      } catch {
        // a file the engine still holds; the temp folder is cleaned later
      }
    },
  };
}

/**
 * A session token, got the way the page gets one. The hub allows ten exchanges a minute, so the tests share one
 * session and only the pairing tests go through a link.
 */
export async function sessionToken(config: string): Promise<string> {
  const code = pairingLink(config).split('#pair=')[1] ?? '';
  const response = await fetch(`${ORIGIN}/auth/exchange`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Origin: ORIGIN },
    body: JSON.stringify({ code }),
  });
  if (!response.ok) throw new Error(`exchange refused: ${response.status}`);
  return ((await response.json()) as { token: string }).token;
}

/** A pairing link, as `rules-hub open` prints it. */
export function pairingLink(config: string): string {
  const out = execFileSync(exe, ['open'], { env: env(config), encoding: 'utf8' });
  const link = out.trim().split(/\r?\n/)[0] ?? '';
  if (!link.startsWith(`${ORIGIN}/#pair=`)) throw new Error(`unexpected link: ${link}`);
  return link;
}
