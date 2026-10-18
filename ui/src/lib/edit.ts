// What the editor of a rule hands to the engine: a draft of the fields as the person sees them, and the patch of
// `set` that follows from what they changed. Nothing here knows what a good rule is: the engine checks the patch
// strictly and reads it back before it writes; this only keeps what did not change out of the request, and reads the
// two fields that are data (check, agent) as JSON.

import type { Rule } from './contract';
import { titleText } from './ruletext';
import type { Patch } from './write';

/** The rule itself, inside the engine's document. */
export type Body = Rule['rule'];

/** The fields of the editor, as text. */
export interface Draft {
  severity: string;
  title: string;
  text: string;
  /** JSON; empty: the rule has none. */
  check: string;
  agent: string;
}

export type PatchResult =
  | { ok: true; patch: Patch }
  | { ok: false; field: 'title' | 'check' | 'agent'; reason: 'multiline' | 'empty' | 'json' };

function dataText(value: unknown): string {
  return value === undefined || value === null ? '' : JSON.stringify(value, null, 2);
}

/** The draft a rule starts the editor with. */
export function draftOf(rule: Body): Draft {
  return {
    severity: rule.severity,
    title: titleText(rule.title, rule.id),
    text: rule.text,
    check: dataText(rule.check),
    agent: dataText(rule.agent),
  };
}

/** Is the draft what the rule is? */
export function unchanged(rule: Body, draft: Draft): boolean {
  const start = draftOf(rule);
  return (
    start.severity === draft.severity &&
    start.title === draft.title.trim() &&
    start.text === draft.text &&
    start.check === draft.check.trim() &&
    start.agent === draft.agent.trim()
  );
}

/** Read a field that is data: empty takes it away (`null`), anything else must be a JSON object. */
function dataPatch(draft: string): { ok: true; value: unknown } | { ok: false } {
  const trimmed = draft.trim();
  if (trimmed === '') return { ok: true, value: null };
  try {
    const value: unknown = JSON.parse(trimmed);
    return typeof value === 'object' && value !== null && !Array.isArray(value) ? { ok: true, value } : { ok: false };
  } catch {
    return { ok: false };
  }
}

/**
 * The patch of what changed. `check` and `agent` are only looked at for a rule whose check is data: one that holds a
 * Lua function (`editable` false) is sent without them and they are never written back.
 */
export function patchOf(rule: Body, draft: Draft): PatchResult {
  const start = draftOf(rule);
  const patch: Patch = {};
  if (draft.severity !== start.severity) patch.severity = draft.severity;
  const title = draft.title.trim();
  if (title !== start.title) {
    if (title === '') return { ok: false, field: 'title', reason: 'empty' };
    if (/[\r\n]/.test(title)) return { ok: false, field: 'title', reason: 'multiline' };
    patch.title = title;
  }
  if (draft.text !== start.text) patch.text = draft.text;
  if (rule.editable) {
    for (const field of ['check', 'agent'] as const) {
      if (draft[field].trim() === start[field]) continue;
      const read = dataPatch(draft[field]);
      if (!read.ok) return { ok: false, field, reason: 'json' };
      patch[field] = read.value;
    }
  }
  return { ok: true, patch };
}
