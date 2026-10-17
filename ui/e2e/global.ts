import { writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { sessionToken, startHub, type Running } from './hub';

// One file for one run: the workers inherit the variable from here.
process.env['RULES_DESKTOP_E2E_STATE'] ??= join(tmpdir(), `rules-desktop-e2e-state-${process.pid}.json`);
export const STATE = process.env['RULES_DESKTOP_E2E_STATE'];

let running: (Running & { root: string }) | null = null;

export default async function setup(): Promise<() => Promise<void>> {
  running = await startHub();
  const token = await sessionToken(running.config);
  writeFileSync(STATE, JSON.stringify({ config: running.config, vault: running.vault, token }));
  return async () => {
    running?.stop();
  };
}
