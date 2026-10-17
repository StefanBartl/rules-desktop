// The rows of the family table, from `families[]` of the snapshot (the engine counts; the page only adds up). Every
// field of an entry is optional in the contract: one that is missing counts 0, and an entry without a name is left
// out (it could not be filtered by).

import type { Snapshot } from './contract';
import { byText } from './filter';

export interface FamilyRow {
  family: string;
  count: number;
  critical: number;
  recommended: number;
  nice_to_have: number;
  with_check: number;
}

export type FamilyTotals = Omit<FamilyRow, 'family'>;

function count(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : 0;
}

/** The families by name. */
export function familyRows(families: Snapshot['families']): FamilyRow[] {
  const rows: FamilyRow[] = [];
  for (const entry of families) {
    if (typeof entry.family !== 'string' || entry.family === '') continue;
    rows.push({
      family: entry.family,
      count: count(entry.count),
      critical: count(entry.critical),
      recommended: count(entry.recommended),
      nice_to_have: count(entry.nice_to_have),
      with_check: count(entry.with_check),
    });
  }
  return rows.sort((a, b) => byText(a.family, b.family));
}

/** The sums over the rows. */
export function familyTotals(rows: readonly FamilyRow[]): FamilyTotals {
  const total: FamilyTotals = { count: 0, critical: 0, recommended: 0, nice_to_have: 0, with_check: 0 };
  for (const row of rows) {
    total.count += row.count;
    total.critical += row.critical;
    total.recommended += row.recommended;
    total.nice_to_have += row.nice_to_have;
    total.with_check += row.with_check;
  }
  return total;
}
