// The cleanup of the browser tests, without a hub: the folder and the state file go, also when something fails, and a
// folder a process still holds goes once that process has been stopped and waited for.

import { spawn } from 'node:child_process';
import { existsSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { PREFIX, failAfterCleanup, removeTree, stopChild, tempFile, tempRoot } from './cleanup';
import { teardown } from './global';
import type { Running } from './hub';

describe('the temp dir of the browser tests', () => {
  it('a root is named for the leftover check and goes with everything in it', async () => {
    const root = tempRoot('cleanup-test');
    expect(basename(root).startsWith(`${PREFIX}cleanup-test-`)).toBe(true);
    writeFileSync(join(root, 'file'), 'x');
    await removeTree(root);
    expect(existsSync(root)).toBe(false);
    // One that is not there any more is no error.
    await removeTree(root);
  });

  it('a removal that fails is retried and then an error, never a silent leftover', async () => {
    let calls = 0;
    await removeTree('x', {
      pauseMs: 1,
      rm: () => {
        calls++;
        if (calls < 4) throw new Error('EBUSY');
      },
    });
    expect(calls).toBe(4);

    calls = 0;
    const failing = removeTree('held', {
      tries: 3,
      pauseMs: 1,
      rm: () => {
        calls++;
        throw new Error('EBUSY');
      },
    });
    await expect(failing).rejects.toThrow(/cannot remove held: EBUSY/);
    expect(calls).toBe(3);
  });

  it('a failed cleanup after a failed start is named in the error, a good one leaves it as it was', async () => {
    const started = new Error('the engine is not ready');
    await expect(failAfterCleanup(started, () => Promise.resolve())).rejects.toBe(started);
    const both = failAfterCleanup(started, () => Promise.reject(new Error('cannot remove held: EBUSY')));
    await expect(both).rejects.toThrow('the engine is not ready; the cleanup failed too: cannot remove held: EBUSY');
    await expect(both).rejects.toHaveProperty('cause', started);
  });

  it('a folder a running process holds goes once the process is stopped and waited for', async () => {
    const root = tempRoot('cleanup-held');
    // The working directory of a process is held by it on Windows: the folder cannot go while it runs.
    const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { cwd: root, stdio: 'ignore' });
    await new Promise((r) => child.once('spawn', r));
    try {
      await stopChild(child);
      expect(child.exitCode !== null || child.signalCode !== null).toBe(true);
      await removeTree(root, { tries: 1 });
      expect(existsSync(root)).toBe(false);
    } finally {
      child.kill();
      await removeTree(root);
    }
  });

  it('the teardown removes the state file also when the hub cannot be removed, and says so', async () => {
    const state = tempFile(`e2e-state-test-${process.pid}.json`);
    writeFileSync(state, '{}');
    const hub = {
      stop: () => Promise.reject(new Error('cannot remove the root')),
    } as unknown as Running;
    await expect(teardown(hub, state)).rejects.toThrow(/cannot remove the root/);
    expect(existsSync(state)).toBe(false);

    writeFileSync(state, '{}');
    await teardown(null, state);
    expect(existsSync(state)).toBe(false);
  });
});
