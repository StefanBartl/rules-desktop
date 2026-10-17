// The filter of the rule list: families, severities and kinds of check to keep, and words. Plain functions; the
// state that holds a filter is the page's (`app.svelte.ts`).

import type { RuleSummary, Severity } from './contract';

export interface Filter {
  /** Empty: every family. */
  families: string[];
  /** Empty: every severity. */
  severities: string[];
  /** Empty: every kind of check; `none` is a rule without one. */
  checkKinds: string[];
  /** Words: the title is searched here, the text by the hub. */
  query: string;
}

export const SEVERITIES: readonly Severity[] = ['critical', 'recommended', 'nice-to-have'];

/** The word for a rule without a check. */
export const NO_CHECK = 'none';

export function emptyFilter(): Filter {
  return { families: [], severities: [], checkKinds: [], query: '' };
}

/** Is any part of the filter on? */
export function isActive(filter: Filter): boolean {
  return (
    filter.families.length > 0 ||
    filter.severities.length > 0 ||
    filter.checkKinds.length > 0 ||
    filter.query.trim() !== ''
  );
}

/** The kind of a rule's check, `none` without one. */
export function checkKindOf(rule: RuleSummary): string {
  return rule.check_type ?? NO_CHECK;
}

/** Text compared without case (and in one Unicode form, so that an `ä` typed is the `ä` written). */
export function fold(text: string): string {
  return text.normalize('NFC').toLowerCase();
}

/**
 * The rules the filter keeps, in their order. Words match the title here (without case); `textHits` are the ids the
 * hub found the words in the text of, and a rule that is one of them is kept too. Without hits only titles count.
 */
export function applyFilter(
  rules: readonly RuleSummary[],
  filter: Filter,
  textHits: ReadonlySet<string> | null = null,
): RuleSummary[] {
  const families = new Set(filter.families);
  const severities = new Set(filter.severities);
  const kinds = new Set(filter.checkKinds);
  const words = fold(filter.query.trim());
  return rules.filter((rule) => {
    if (families.size > 0 && !families.has(rule.family)) return false;
    if (severities.size > 0 && !severities.has(rule.severity)) return false;
    if (kinds.size > 0 && !kinds.has(checkKindOf(rule))) return false;
    if (words === '') return true;
    return fold(rule.title).includes(words) || (textHits?.has(rule.id) ?? false);
  });
}

/** The families the rules have, sorted: the choices of the family filter. */
export function familiesOf(rules: readonly RuleSummary[]): string[] {
  return [...new Set(rules.map((r) => r.family))].sort(byText);
}

/** The kinds of check the rules have, sorted, `none` last: the choices of the check filter. */
export function checkKindsOf(rules: readonly RuleSummary[]): string[] {
  const kinds = [...new Set(rules.map(checkKindOf))].filter((k) => k !== NO_CHECK).sort(byText);
  return rules.some((r) => r.check_type === undefined) ? [...kinds, NO_CHECK] : kinds;
}

/** One step through a list of ids: at an end it stays, from nothing it starts at the end it moves from. */
export function moveSelection(ids: readonly string[], current: string | null, delta: number): string | null {
  if (ids.length === 0) return null;
  const at = current === null ? -1 : ids.indexOf(current);
  if (at === -1) return ids[delta >= 0 ? 0 : ids.length - 1] ?? null;
  return ids[Math.min(ids.length - 1, Math.max(0, at + delta))] ?? null;
}

/** A fixed order for names, the same on every machine (not the order of a locale). */
export function byText(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}
