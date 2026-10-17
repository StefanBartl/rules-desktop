import { describe, expect, it } from 'vitest';
import { snapshotDoc } from './fixtures';
import {
  ACTION_PREFIX,
  bumped,
  FRECENCY_MAX,
  matchScore,
  paletteItems,
  parseFrecency,
  stepIndex,
  wordScore,
  type PaletteAction,
} from './palette';

const rules = snapshotDoc().rules;
const action = (key: string, label: string): PaletteAction => ({ key, label, run: () => undefined });
const actions = [action('go.rules', 'Go to: Rules'), action('go.families', 'Go to: Families'), action('rescan', 'Read again')];
const NOW = 1_800_000_000_000;

describe('wordScore', () => {
  it('ranks a start of the text over a start of a word over the inside over the letters in order', () => {
    const start = wordScore('families', 'fam')!;
    const word = wordScore('go to families', 'fam')!;
    const inside = wordScore('xfamilies', 'fam')!;
    const letters = wordScore('f-a-m', 'fam')!;
    expect(start).toBeGreaterThan(word);
    expect(word).toBeGreaterThan(inside);
    expect(inside).toBeGreaterThan(letters);
    expect(wordScore('abc', 'cb')).toBeNull();
    expect(wordScore('abc', '')).toBe(0);
  });

  it('wants every word of the box', () => {
    expect(matchScore('Go to: Rules', 'go rul')).not.toBeNull();
    expect(matchScore('Go to: Rules', 'go zzz')).toBeNull();
  });
});

describe('paletteItems', () => {
  it('finds a rule by id and by title', () => {
    const byId = paletteItems('LUA-01', actions, rules, {}, NOW);
    expect(byId[0]).toMatchObject({ kind: 'rule', id: 'LUA-01' });
    const byTitle = paletteItems('function', actions, rules, {}, NOW);
    expect(byTitle[0]).toMatchObject({ kind: 'rule', id: 'LUA-01' });
  });

  it('shows the actions and no rule for an empty box, those chosen before first', () => {
    const items = paletteItems('', actions, rules, bumped({}, 'rescan', NOW), NOW);
    expect(items.map((i) => i.kind)).toEqual(['action', 'action', 'action']);
    expect(items[0]).toMatchObject({ kind: 'action', key: 'action:rescan' });
  });

  it('puts the actions first when they match better', () => {
    const items = paletteItems('go to', actions, rules, {}, NOW);
    expect(items[0]?.kind).toBe('action');
  });

  it('stops at the limit', () => {
    expect(paletteItems('', actions, rules, {}, NOW, 2)).toHaveLength(2);
  });
});

describe('the frecency', () => {
  it('counts the choices of actions', () => {
    const once = bumped({}, 'rescan', NOW);
    expect(bumped(once, 'rescan', NOW + 1)).toEqual({ 'action:rescan': { n: 2, last: NOW + 1 } });
  });

  it('keeps no more than its most', () => {
    let f = {};
    for (let i = 0; i <= FRECENCY_MAX; i += 1) f = bumped(f, `a${i}`, NOW + i);
    expect(Object.keys(f)).toHaveLength(FRECENCY_MAX);
  });

  it('reads back only entries of actions, so a rule id never comes back from the browser', () => {
    const text = JSON.stringify({
      'action:rescan': { n: 2, last: NOW },
      'rule:DEP-01': { n: 5, last: NOW },
      'DEP-01': { n: 5, last: NOW },
      'action:bad': { n: 'x', last: NOW },
    });
    expect(parseFrecency(text)).toEqual({ 'action:rescan': { n: 2, last: NOW } });
    expect(parseFrecency('[1]')).toEqual({});
    expect(parseFrecency('{')).toEqual({});
    expect(parseFrecency(null)).toEqual({});
    expect(Object.keys(parseFrecency(text)).every((k) => k.startsWith(ACTION_PREFIX))).toBe(true);
  });
});

describe('stepIndex', () => {
  it('stops at the ends', () => {
    expect(stepIndex(0, -1, 3)).toBe(0);
    expect(stepIndex(2, 1, 3)).toBe(2);
    expect(stepIndex(1, 1, 3)).toBe(2);
    expect(stepIndex(5, 1, 0)).toBe(0);
  });
});
