import { describe, expect, it } from 'vitest';
import { retryOp, rowsOf } from './conflicts';
import type { Body } from './edit';
import type { Conflict, Op } from './write';

const current = {
  id: 'DEP-01',
  title: '`DEP-01` — theirs',
  severity: 'recommended',
  text: 'Their text.',
  editable: true,
  etag: 'sha256:b',
  etag_scope: 'rule',
} as unknown as Body;

const set: Op = { op: 'set', id: 'DEP-01', patch: { severity: 'critical', title: 'mine', check: null }, if_match: 'sha256:a' };
const conflict = (op: Op | null, actual: string | null = 'sha256:b'): Conflict => ({ id: 'DEP-01', n: 1, expected: 'sha256:a', actual, op });

describe('the rows of a conflict', () => {
  it('put what the person changed next to what the file says now', () => {
    expect(rowsOf(set, current)).toEqual([
      { field: 'severity', mine: 'critical', current: 'recommended' },
      { field: 'title', mine: 'mine', current: 'theirs' },
      { field: 'check', mine: '', current: '' },
    ]);
  });

  it('are empty for an operation that changes no field, or when nothing was asked', () => {
    expect(rowsOf({ op: 'retire', id: 'DEP-01' }, current)).toEqual([]);
    expect(rowsOf(null, current)).toEqual([]);
    expect(rowsOf(set, null)[0]?.current).toBe('');
  });
});

describe('asking again', () => {
  it('is the same operation onto the version the file has now, for set and retire only', () => {
    expect(retryOp(conflict(set))).toEqual({ ...set, if_match: 'sha256:b' });
    expect(retryOp(conflict({ op: 'retire', id: 'DEP-01', if_match: 'sha256:a' }))).toMatchObject({ if_match: 'sha256:b' });
    expect(retryOp(conflict({ op: 'move', id: 'DEP-01', after: 'DEP-02' }))).toBeNull();
    expect(retryOp(conflict(null))).toBeNull();
    // Without the tag the file has now there is nothing to write onto.
    expect(retryOp(conflict(set, null))).toBeNull();
  });
});
