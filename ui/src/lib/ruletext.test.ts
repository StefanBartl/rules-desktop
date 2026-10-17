import { describe, expect, it } from 'vitest';
import { snapshotDoc } from './fixtures';
import { asJson, holdsHidden, shownText, sourceLine, titleText } from './ruletext';

describe('titleText', () => {
  it('takes the id off the front of every golden title', () => {
    expect(snapshotDoc().rules.map((r) => titleText(r.title, r.id))).toEqual([
      'one',
      'nine',
      'a rule of its own',
      'function check',
    ]);
  });

  it('leaves a title that does not start with its own id', () => {
    expect(titleText('`DEP-02` — one', 'DEP-01')).toBe('`DEP-02` — one');
    expect(titleText('one', 'DEP-01')).toBe('one');
    expect(titleText('`DEP-01`one', 'DEP-01')).toBe('`DEP-01`one');
  });

  it('takes a plain or an en dash too, and keeps a title that would be empty', () => {
    expect(titleText('`A-1` - x', 'A-1')).toBe('x');
    expect(titleText('`A-1` \u2013 x', 'A-1')).toBe('x');
    expect(titleText('`A-1` — ', 'A-1')).toBe('`A-1` — ');
  });
});

describe('the hidden characters', () => {
  it('are found and shown', () => {
    const text = 'a\u202Eb\u200Bc';
    expect(holdsHidden(text)).toBe(true);
    expect(shownText(text)).toBe('a⟦U+202E⟧b⟦U+200B⟧c');
    expect(holdsHidden('plain')).toBe(false);
  });

  it('are shown in a check as JSON too', () => {
    expect(asJson({ type: 'grep', pattern: 'x\u2066' })).toBe('{\n  "type": "grep",\n  "pattern": "x⟦U+2066⟧"\n}');
    expect(asJson(undefined)).toBe('');
  });
});

describe('sourceLine', () => {
  it('names the file and the line', () => {
    expect(sourceLine('Checklists/DEP.md', 5)).toBe('Checklists/DEP.md:5');
    expect(sourceLine('Checklists/DEP.md', 0)).toBe('Checklists/DEP.md');
  });
});
