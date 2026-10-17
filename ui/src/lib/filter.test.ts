import { describe, expect, it } from 'vitest';
import {
  applyFilter,
  checkKindOf,
  checkKindsOf,
  emptyFilter,
  familiesOf,
  isActive,
  moveSelection,
  NO_CHECK,
} from './filter';
import { snapshotDoc } from './fixtures';

const rules = snapshotDoc().rules;
const idsOf = (list: { id: string }[]) => list.map((r) => r.id);

describe('the filter of the rule list', () => {
  it('keeps everything when nothing is on', () => {
    expect(isActive(emptyFilter())).toBe(false);
    expect(idsOf(applyFilter(rules, emptyFilter()))).toEqual(['DEP-01', 'DEP-09', 'ERR-100', 'LUA-01']);
  });

  it('narrows by family, severity and kind of check, together', () => {
    const f = { ...emptyFilter(), families: ['DEP', 'LUA'] };
    expect(idsOf(applyFilter(rules, f))).toEqual(['DEP-01', 'DEP-09', 'LUA-01']);
    expect(idsOf(applyFilter(rules, { ...f, severities: ['critical'] }))).toEqual(['DEP-09', 'LUA-01']);
    expect(idsOf(applyFilter(rules, { ...f, severities: ['critical'], checkKinds: [NO_CHECK] }))).toEqual(['DEP-09']);
    expect(isActive(f)).toBe(true);
  });

  it('calls a rule without a check `none`', () => {
    expect(rules.map(checkKindOf)).toEqual(['grep', NO_CHECK, NO_CHECK, 'lua_predicate']);
    expect(checkKindsOf(rules)).toEqual(['grep', 'lua_predicate', NO_CHECK]);
    expect(familiesOf(rules)).toEqual(['DEP', 'ERR', 'LUA']);
  });

  it('matches words in the title without case, and the text through the hits of the hub', () => {
    const f = { ...emptyFilter(), query: '  FUNCTION ' };
    expect(idsOf(applyFilter(rules, f))).toEqual(['LUA-01']);
    const g = { ...emptyFilter(), query: 'Text of' };
    expect(idsOf(applyFilter(rules, g))).toEqual([]);
    expect(idsOf(applyFilter(rules, g, new Set(['DEP-01'])))).toEqual(['DEP-01']);
    // A hit is still narrowed by the other parts of the filter.
    expect(idsOf(applyFilter(rules, { ...g, families: ['ERR'] }, new Set(['DEP-01'])))).toEqual([]);
  });

  it('compares one Unicode form with another', () => {
    const decomposed = [{ ...rules[0]!, title: 'Gro\u0308ße' }];
    expect(applyFilter(decomposed, { ...emptyFilter(), query: 'gr\u00F6' })).toHaveLength(1);
  });
});

describe('moveSelection', () => {
  const ids = ['a', 'b', 'c'];
  it('steps and stops at the ends', () => {
    expect(moveSelection(ids, 'a', 1)).toBe('b');
    expect(moveSelection(ids, 'c', 1)).toBe('c');
    expect(moveSelection(ids, 'a', -1)).toBe('a');
  });
  it('starts at the end it moves from when nothing or something gone is selected', () => {
    expect(moveSelection(ids, null, 1)).toBe('a');
    expect(moveSelection(ids, 'gone', -1)).toBe('c');
    expect(moveSelection([], 'a', 1)).toBeNull();
  });
});
