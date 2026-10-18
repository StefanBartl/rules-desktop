// The keyboard of the page as a plain mapping from a key press to a command, so that it is tested without a DOM.
// `g` starts a chord (`g h` overview, `g f` families, `g l` rules) that is forgotten after 1500 ms; `j`/`k` move the
// selection, `Enter` opens, `/` goes to the search, `Escape` closes; Ctrl or Cmd with K opens the palette.

import type { View } from './app.svelte';

export type Command =
  | { kind: 'view'; view: View }
  | { kind: 'move'; delta: 1 | -1 }
  | { kind: 'open' }
  | { kind: 'search' }
  | { kind: 'escape' }
  | { kind: 'palette' };

/** What the page knows of a key press. */
export interface KeyPress {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  /** The focus is in a field, a menu or a dialog: a letter there is typed, not a shortcut. */
  typing: boolean;
}

/** When the `g` of a chord was pressed (ms), or `null`. */
export type ChordState = number | null;

/** A `g` that nothing follows within this is forgotten (as in tasks-desktop). */
export const CHORD_MS = 1500;

const CHORD: Readonly<Record<string, View>> = { h: 'overview', f: 'families', l: 'rules', c: 'coverage', w: 'waivers', s: 'sources' };

const MODIFIERS = new Set(['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'AltGraph']);

export interface KeyResult {
  /** The command, or `null` when the key is not one (the page lets the browser have it). */
  command: Command | null;
  /** Whether a `g` is now waiting for its second key. */
  chord: ChordState;
}

/**
 * The command of a key press. `shortcuts` off turns off the single keys that are letters or `/` (assistive software
 * uses them); `Enter`, `Escape` and the palette stay, they are not characters typed by accident.
 */
export function commandFor(press: KeyPress, chord: ChordState, now: number, shortcuts: boolean): KeyResult {
  // The palette opens with Ctrl or Cmd and K, also in a field and with the single keys off.
  if ((press.ctrlKey || press.metaKey) && !press.altKey && press.key.toLowerCase() === 'k') {
    return { command: { kind: 'palette' }, chord: null };
  }
  // A modifier on its own is no key yet: a chord waits through it.
  if (MODIFIERS.has(press.key)) return { command: null, chord };
  if (press.ctrlKey || press.metaKey || press.altKey || press.typing) return { command: null, chord: null };
  if (press.key === 'Enter') return { command: { kind: 'open' }, chord: null };
  if (press.key === 'Escape') return { command: { kind: 'escape' }, chord: null };
  if (!shortcuts) return { command: null, chord: null };
  // `/` is typed with Shift on many layouts; a letter with Shift is another key. CapsLock gives `J` without Shift.
  if (press.key === '/') return { command: { kind: 'search' }, chord: null };
  if (press.shiftKey || press.key.length !== 1) return { command: null, chord: null };
  const letter = press.key.toLowerCase();
  if (chord !== null && now - chord < CHORD_MS) {
    const view = CHORD[letter];
    if (view !== undefined) return { command: { kind: 'view', view }, chord: null };
    // Any other key ends the chord and is taken as itself.
  }
  switch (letter) {
    case 'g':
      return { command: null, chord: now };
    case 'j':
      return { command: { kind: 'move', delta: 1 }, chord: null };
    case 'k':
      return { command: { kind: 'move', delta: -1 }, chord: null };
    default:
      return { command: null, chord: null };
  }
}
