// The global setup of the browser tests when the start fails, without a hub: what was made so far goes, and a
// cleanup that fails is said in the error instead of a folder left behind silently.

import { existsSync, writeFileSync } from 'node:fs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const hub = vi.hoisted(() => ({
  startHub: vi.fn(),
  sessionToken: vi.fn(),
}));
vi.mock('./hub', () => hub);

const { default: setup, STATE } = await import('./global');

describe('a global setup that fails', () => {
  beforeEach(() => {
    hub.startHub.mockReset();
    hub.sessionToken.mockReset();
  });

  it('stops the hub, removes the state file and throws the error of the start', async () => {
    const stop = vi.fn(() => Promise.resolve());
    hub.startHub.mockResolvedValue({ config: 'c', vault: 'v', root: 'r', stop });
    const refused = new Error('exchange refused: 429');
    hub.sessionToken.mockRejectedValue(refused);
    writeFileSync(STATE, '{}');
    await expect(setup()).rejects.toBe(refused);
    expect(stop).toHaveBeenCalledTimes(1);
    expect(existsSync(STATE)).toBe(false);
  });

  it('says so when the folder of the hub could not be removed', async () => {
    hub.startHub.mockResolvedValue({
      config: 'c',
      vault: 'v',
      root: 'r',
      stop: () => Promise.reject(new Error('cannot remove the root')),
    });
    hub.sessionToken.mockRejectedValue(new Error('exchange refused: 429'));
    await expect(setup()).rejects.toThrow(/exchange refused: 429; the cleanup failed too: cannot remove the root/);
    expect(existsSync(STATE)).toBe(false);
  });
});
