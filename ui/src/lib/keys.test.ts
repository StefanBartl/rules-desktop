import { describe, expect, it } from 'vitest';
import { CHORD_MS, commandFor, type ChordState, type KeyPress } from './keys';

function press(key: string, extra: Partial<KeyPress> = {}): KeyPress {
  return { key, ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, typing: false, ...extra };
}

/** Feed keys at the given times; the commands that came out. */
function run(keys: [string, number, Partial<KeyPress>?][], shortcuts = true): (string | null)[] {
  let chord: ChordState = null;
  return keys.map(([key, at, extra]) => {
    const result = commandFor(press(key, extra), chord, at, shortcuts);
    chord = result.chord;
    const c = result.command;
    return c === null ? null : c.kind === 'view' ? `view:${c.view}` : c.kind === 'move' ? `move:${c.delta}` : c.kind;
  });
}

describe('the keys of the page', () => {
  it('go to the coverage with g c', () => {
    expect(run([['g', 0], ['c', 10]])).toEqual([null, 'view:coverage']);
  });

  it('go to a view with g and a letter', () => {
    expect(run([['g', 0], ['h', 10], ['g', 20], ['f', 30], ['g', 40], ['l', 50]])).toEqual([
      null,
      'view:overview',
      null,
      'view:families',
      null,
      'view:rules',
    ]);
  });

  it('forget a g that nothing follows in time', () => {
    expect(run([['g', 0], ['h', CHORD_MS]])).toEqual([null, null]);
    expect(run([['g', 0], ['h', CHORD_MS - 1]])).toEqual([null, 'view:overview']);
  });

  it('take another key after g as itself, and wait through a modifier', () => {
    expect(run([['g', 0], ['j', 1]])).toEqual([null, 'move:1']);
    expect(run([['g', 0], ['Shift', 1], ['l', 2]])).toEqual([null, null, 'view:rules']);
    // A letter alone is no chord.
    expect(run([['h', 0], ['l', 1]])).toEqual([null, null]);
  });

  it('move, open, search and close', () => {
    expect(run([['j', 0], ['k', 1], ['Enter', 2], ['/', 3], ['Escape', 4]])).toEqual([
      'move:1',
      'move:-1',
      'open',
      'search',
      'escape',
    ]);
    // `/` is Shift and 7 on a German keyboard; CapsLock makes `J` without Shift.
    expect(run([['/', 0, { shiftKey: true }], ['J', 1]])).toEqual(['search', 'move:1']);
    expect(run([['J', 0, { shiftKey: true }]])).toEqual([null]);
  });

  it('open the palette with Ctrl or Cmd and K, also in a field and with the single keys off', () => {
    expect(run([['k', 0, { ctrlKey: true }], ['K', 1, { metaKey: true, typing: true }]], false)).toEqual([
      'palette',
      'palette',
    ]);
    expect(run([['k', 0, { ctrlKey: true, altKey: true }]])).toEqual([null]);
  });

  it('leave a field and a modified key alone', () => {
    expect(run([['j', 0, { typing: true }], ['Enter', 1, { typing: true }], ['Escape', 2, { typing: true }]])).toEqual([
      null,
      null,
      null,
    ]);
    expect(run([['j', 0, { ctrlKey: true }], ['g', 1, { altKey: true }], ['h', 2]])).toEqual([null, null, null]);
  });

  it('with the single keys off take no letter and no slash, but Enter and Escape', () => {
    expect(run([['g', 0], ['h', 1], ['j', 2], ['/', 3], ['Enter', 4], ['Escape', 5]], false)).toEqual([
      null,
      null,
      null,
      null,
      'open',
      'escape',
    ]);
  });
});
