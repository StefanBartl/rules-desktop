// A change that met another version of the file: what the conflict center shows (the fields the person changed, next
// to what the file says now) and what can be done about it. Nothing is merged and nothing is decided here: the person
// looks at both and chooses; the engine only ever writes over a version the request names.

import { draftOf, type Body } from './edit';
import type { Conflict, Op, Patch } from './write';

/** One field the person changed, with their text and the text of the file now. */
export interface Row {
  field: 'severity' | 'title' | 'text' | 'check' | 'agent';
  mine: string;
  current: string;
}

const FIELDS = ['severity', 'title', 'text', 'check', 'agent'] as const;

function asText(value: unknown): string {
  if (value === null || value === undefined) return '';
  return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}

/** The fields a `set` changed against the same fields of the rule as it is now. Another operation has none. */
export function rowsOf(op: Op | null, current: Body | null): Row[] {
  if (!op || op.op !== 'set' || typeof op['patch'] !== 'object' || op['patch'] === null) return [];
  const patch = op['patch'] as Patch;
  const now = current ? draftOf(current) : null;
  return FIELDS.filter((field) => field in patch).map((field) => ({
    field,
    mine: asText(patch[field]),
    current: now ? now[field] : '',
  }));
}

/**
 * The same operation again, onto the version the file has now. Only for the two that carry the tag they were
 * written against (`set`, `retire`): it is the person's decision to write over what the other change did.
 */
export function retryOp(conflict: Conflict): Op | null {
  const op = conflict.op;
  if (!op || (op.op !== 'set' && op.op !== 'retire') || !conflict.actual) return null;
  return { ...op, if_match: conflict.actual };
}
