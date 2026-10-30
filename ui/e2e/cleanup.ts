// What the browser tests leave in the temp dir: nothing. The one place they get a folder or a file there from, and the
// one way they take it away again (the Rust tests have `Scratch`, crates/rules-hub/tests/common/scratch.rs).
//
// A folder a running process still holds cannot be removed on Windows: the hub and the Neovim of its engine keep files
// of the vault open until they have ended. So the hub is stopped and waited for first, and the removal is retried for a
// few seconds (a virus scanner may hold a file just as long); then it fails loudly instead of leaving the folder
// silently. CI counts what is left after the tests (tools/check-temp-leftovers.py).

import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { ChildProcess } from 'node:child_process';

/** What every folder and file of the tests is called first; the leftover check counts it. */
export const PREFIX = 'rules-desktop-';

/** A new, empty folder `rules-desktop-<label>-XXXXXX` in the temp dir. Take it away with {@link removeTree}. */
export function tempRoot(label: string): string {
  return mkdtempSync(join(tmpdir(), `${PREFIX}${label}-`));
}

/** A path `rules-desktop-<label>` in the temp dir for a file of the caller's (nothing is made). */
export function tempFile(label: string): string {
  return join(tmpdir(), `${PREFIX}${label}`);
}

export interface RemoveOptions {
  tries?: number;
  pauseMs?: number;
  /** The removal of one try; `fs.rmSync` (a test hands in one that fails). */
  rm?: (path: string) => void;
}

/**
 * Removes `path` (a folder with everything in it, or a file); one that is not there is fine. Retried, because `maxRetries`
 * of `fs.rmSync` does not cover every error Windows gives for a held file; the last error is thrown.
 */
export async function removeTree(path: string, options: RemoveOptions = {}): Promise<void> {
  const tries = options.tries ?? 50;
  const pauseMs = options.pauseMs ?? 100;
  const rm = options.rm ?? ((p: string) => rmSync(p, { recursive: true, force: true }));
  let last: unknown;
  for (let i = 0; i < tries; i++) {
    try {
      rm(path);
      return;
    } catch (error) {
      last = error;
    }
    await new Promise((r) => setTimeout(r, pauseMs));
  }
  const why = last instanceof Error ? last.message : String(last);
  throw new Error(`cannot remove ${path}: ${why} (a process of the tests still holds a file in it?)`);
}

/** Ends `child` and waits until it has ended (at most `timeoutMs`), so that its files are no longer held. */
export async function stopChild(child: ChildProcess, timeoutMs = 10_000): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) return;
  const ended = new Promise<void>((resolve) => child.once('exit', () => resolve()));
  child.kill();
  let timer: NodeJS.Timeout | undefined;
  const late = new Promise<void>((resolve) => {
    timer = setTimeout(resolve, timeoutMs);
  });
  await Promise.race([ended, late]);
  clearTimeout(timer);
}
