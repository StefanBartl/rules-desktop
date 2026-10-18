// The write side of the page as plain functions: what to ask (the operations of rules-ops/1 as the page builds them),
// and what to make of the answer (what was saved, what clashed, what undoes it). The rules are the engine's: nothing
// here formats or parses a rule, it only passes on what the person typed and reads the engine's document.

import type { Result } from './contract';

/** One operation of a request: its name and its fields. The engine checks the rest, strictly. */
export type Op = { op: string } & Record<string, unknown>;

/** The request of `rules-ops/1` as the hub takes it. */
export interface OpsRequest {
  schema: 1;
  client_op_id: string;
  dry_run: boolean;
  ops: Op[];
}

/** An entry of the engine's answer, one per operation. */
export type Entry = Result['results'][number];

/** An operation that met another version of the file than the person saw: nothing was written. */
export interface Conflict {
  /** The rule's id. */
  id: string;
  /** The place of the operation in the request, from 1. */
  n: number;
  /** The tag the person saw, and the tag the file has now. */
  expected: string | null;
  actual: string | null;
  /** What was asked, so that it can be asked again once the person has looked at the other version. */
  op: Op | null;
}

/** An operation that failed for another reason. */
export interface Failure {
  n: number;
  id: string | null;
  code: string;
  message: string;
}

/** What an answer of `ops` comes to. */
export interface Outcome {
  /** Entries that changed a rule (a dry run saves nothing). */
  saved: number;
  changedIds: string[];
  conflicts: Conflict[];
  failures: Failure[];
  warnings: string[];
  /** The operations that take it back, last first; empty for a dry run or when nothing changed. */
  undo: Op[];
}

/** A client id for a request: 1 to 64 of the characters the hub takes. */
export function clientOpId(now: number, n: number): string {
  return `c-${now.toString(36)}-${n.toString(36)}`;
}

/** The operations whose inverse is not complete without the tag the file has after the write. */
const NEEDS_IF_MATCH: readonly string[] = ['set', 'retire'];

function text(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

/** Read the engine's answer for a request. `request` is the list of operations that was sent. */
export function summarize(request: readonly Op[], result: Result): Outcome {
  const out: Outcome = { saved: 0, changedIds: [], conflicts: [], failures: [], warnings: [], undo: [] };
  for (const entry of result.results) {
    const id = entry.id ?? null;
    if (entry.warnings) out.warnings.push(...entry.warnings);
    if (!entry.ok) {
      const error = entry.error ?? {};
      const code = error.code ?? 'failed';
      if (code === 'conflict') {
        out.conflicts.push({
          id: id ?? '',
          n: entry.n,
          expected: text(error.details?.['expected']),
          actual: text(error.details?.['actual']),
          op: request[entry.n - 1] ?? null,
        });
      } else {
        out.failures.push({ n: entry.n, id, code, message: error.message ?? '' });
      }
      continue;
    }
    if (result.dry_run || entry.changed_ids.length === 0) continue;
    out.saved += 1;
    out.changedIds.push(...entry.changed_ids);
    const inverse = entry.inverse;
    const name = text(inverse?.['op']);
    if (inverse && name !== null) {
      const undo: Op = { ...inverse, op: name };
      // The tag the file has now: the person's undo must not overwrite a change made since.
      if (NEEDS_IF_MATCH.includes(name) && undo['if_match'] === undefined && entry.etag_after) {
        undo['if_match'] = entry.etag_after;
      }
      out.undo.push(undo);
    }
  }
  out.undo.reverse();
  return out;
}

// ── the operations the page builds ───────────────────────────────────────────

/** The fields of a rule `set` can change; `null` for `check` or `agent` takes it away. */
export interface Patch {
  severity?: string;
  title?: string;
  text?: string;
  check?: unknown;
  agent?: unknown;
}

/** Change parts of a rule. `ifMatch` is the tag the person saw: without it the engine would write over anything. */
export function setOp(id: string, patch: Patch, ifMatch: string): Op {
  return { op: 'set', id, patch, if_match: ifMatch };
}

/** Retire a rule (its id stays taken), optionally with the rule that replaces it and a reason. */
export function retireOp(id: string, ifMatch: string, extra: { replaced_by?: string; reason?: string } = {}): Op {
  const op: Op = { op: 'retire', id, if_match: ifMatch };
  if (extra.replaced_by) op['replaced_by'] = extra.replaced_by;
  if (extra.reason) op['reason'] = extra.reason;
  return op;
}

/** Bring a retired rule back to the file it came from. */
export function reinstateOp(id: string): Op {
  return { op: 'reinstate', id };
}
