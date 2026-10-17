import { describe, expect, it } from 'vitest';
import { familyRows, familyTotals } from './families';
import { snapshotDoc } from './fixtures';

describe('the family table', () => {
  it('has a row per family of the snapshot, by name, with the sums', () => {
    const rows = familyRows(snapshotDoc().families);
    expect(rows.map((r) => r.family)).toEqual(['DEP', 'ERR', 'LUA']);
    expect(rows[0]).toEqual({ family: 'DEP', count: 2, critical: 1, recommended: 1, nice_to_have: 0, with_check: 1 });
    expect(familyTotals(rows)).toEqual({ count: 4, critical: 2, recommended: 1, nice_to_have: 1, with_check: 2 });
  });

  it('counts a missing field as 0 and leaves out an entry without a name', () => {
    const rows = familyRows([{ family: 'ZED', count: 3 }, { count: 9 }, { family: '' }, { family: 'ABC', critical: -1 }]);
    expect(rows).toEqual([
      { family: 'ABC', count: 0, critical: 0, recommended: 0, nice_to_have: 0, with_check: 0 },
      { family: 'ZED', count: 3, critical: 0, recommended: 0, nice_to_have: 0, with_check: 0 },
    ]);
    expect(familyTotals([])).toEqual({ count: 0, critical: 0, recommended: 0, nice_to_have: 0, with_check: 0 });
  });

  it('sorts by the name itself, not by a locale', () => {
    expect(familyRows([{ family: 'b' }, { family: 'B' }, { family: 'a' }]).map((r) => r.family)).toEqual(['B', 'a', 'b']);
  });
});
