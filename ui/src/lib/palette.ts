// The command palette: one box, two kinds of answers - an action of the page and a rule (by id or title). Words match
// loosely (the letters in order are enough). What the person chose before comes first, for actions only: the palette
// remembers no rule id, since nothing of a rule set is kept in the browser.

import type { RuleSummary } from './contract';

/** One thing the palette can run. */
export interface PaletteAction {
  /** A stable name (the key of the frecency). */
  key: string;
  label: string;
  /** A second line, may be empty. */
  hint?: string;
  run: () => void | Promise<void>;
}

export type PaletteItem =
  | { kind: 'action'; key: string; label: string; hint: string; action: PaletteAction; score: number }
  | { kind: 'rule'; key: string; label: string; hint: string; id: string; score: number };

/** How often and when last (ms since the epoch) an action was chosen, by `action:<key>`. */
export type Frecency = Record<string, { n: number; last: number }>;

/** The prefix of every key of the frecency: only actions are remembered. */
export const ACTION_PREFIX = 'action:';
/** Entries kept; the weakest are dropped past this. */
export const FRECENCY_MAX = 100;
/** Answers shown at most. */
export const PALETTE_LIMIT = 30;

const DAY = 24 * 60 * 60 * 1000;

/**
 * How well `text` matches one word: higher is better, `null` is no match. A start of the text beats a start of a
 * word, that beats a place inside a word, and the letters in order with gaps come last.
 */
export function wordScore(text: string, word: string): number | null {
  const t = text.toLowerCase();
  const w = word.toLowerCase();
  if (w === '') return 0;
  if (t.startsWith(w)) return 1000 - Math.min(t.length - w.length, 200);
  let best: number | null = null;
  for (let at = t.indexOf(w); at >= 0; at = t.indexOf(w, at + 1)) {
    // A letter or a digit before the place means it is inside a word.
    const startsWord = at === 0 || !/[\p{L}\p{N}]/u.test(t.charAt(at - 1));
    const score = (startsWord ? 700 : 500) - Math.min(at, 200);
    if (best === null || score > best) best = score;
  }
  if (best !== null) return best;
  // The letters of the word in order, anywhere: the fewer the gaps, the better.
  let from = 0;
  let gaps = 0;
  let last = -1;
  for (const ch of w) {
    const found = t.indexOf(ch, from);
    if (found < 0) return null;
    if (last >= 0) gaps += found - last - 1;
    last = found;
    from = found + ch.length;
  }
  return Math.max(1, 300 - gaps);
}

/** Every word of the box must match somewhere in `text`; the scores add up. */
export function matchScore(text: string, query: string): number | null {
  const words = query.split(/\s+/).filter((w) => w !== '');
  if (words.length === 0) return 0;
  let total = 0;
  for (const word of words) {
    const score = wordScore(text, word);
    if (score === null) return null;
    total += score;
  }
  return total;
}

/** What earlier choices add: more for often, less as the days go by (a half life of two weeks). */
export function frecencyBonus(entry: Frecency[string] | undefined, now: number): number {
  if (!entry) return 0;
  const days = Math.max(0, now - entry.last) / DAY;
  return Math.min(entry.n, 20) * 25 * 0.5 ** (days / 14);
}

/** The action `key` chosen once more; the weakest entries go when there are too many. */
export function bumped(frecency: Frecency, key: string, now: number): Frecency {
  const name = `${ACTION_PREFIX}${key}`;
  const next: Frecency = { ...frecency, [name]: { n: (frecency[name]?.n ?? 0) + 1, last: now } };
  const keys = Object.keys(next);
  if (keys.length <= FRECENCY_MAX) return next;
  const weakest = keys
    .map((k) => [k, frecencyBonus(next[k], now)] as const)
    .sort((a, b) => a[1] - b[1])
    .slice(0, keys.length - FRECENCY_MAX);
  for (const [k] of weakest) delete next[k];
  return next;
}

/** The stored text read back. Anything that is not an action's entry is dropped, so a rule id never comes back. */
export function parseFrecency(text: string | null): Frecency {
  if (!text) return {};
  try {
    const raw = JSON.parse(text) as unknown;
    if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) return {};
    const out: Frecency = {};
    for (const [key, value] of Object.entries(raw as Record<string, unknown>)) {
      if (!key.startsWith(ACTION_PREFIX)) continue;
      const entry = value as { n?: unknown; last?: unknown } | null;
      if (entry && typeof entry.n === 'number' && typeof entry.last === 'number' && entry.n > 0) {
        out[key] = { n: entry.n, last: entry.last };
      }
    }
    return out;
  } catch {
    return {};
  }
}

/**
 * The list for the words in the box. An empty box shows the actions (those chosen before first) and no rule; words
 * match the label of an action, and the id and the title of a rule.
 */
export function paletteItems(
  query: string,
  actions: readonly PaletteAction[],
  rules: readonly RuleSummary[],
  frecency: Frecency,
  now: number,
  limit = PALETTE_LIMIT,
): PaletteItem[] {
  const actionItems: PaletteItem[] = [];
  actions.forEach((action, index) => {
    const match = matchScore(`${action.label} ${action.hint ?? ''}`, query);
    if (match === null) return;
    // The order the page gave them breaks a tie.
    const key = `${ACTION_PREFIX}${action.key}`;
    const score = match + frecencyBonus(frecency[key], now) - index * 0.01;
    actionItems.push({ kind: 'action', key, label: action.label, hint: action.hint ?? '', action, score });
  });
  actionItems.sort((a, b) => b.score - a.score);
  const words = query.trim();
  if (words === '') return actionItems.slice(0, limit);
  const ruleItems: PaletteItem[] = [];
  for (const rule of rules) {
    // The words may be split between the id and the title: the two together count as a third text.
    const best = Math.max(
      matchScore(rule.id, words) ?? -1,
      matchScore(rule.title, words) ?? -1,
      matchScore(`${rule.id} ${rule.title}`, words) ?? -1,
    );
    if (best < 0) continue;
    ruleItems.push({ kind: 'rule', key: `rule:${rule.id}`, label: rule.title, hint: rule.id, id: rule.id, score: best });
  }
  ruleItems.sort((a, b) => b.score - a.score);
  // Actions first when the words name one well, rules first when a rule matches better than any action does.
  const topAction = actionItems[0]?.score ?? -Infinity;
  const topRule = ruleItems[0]?.score ?? -Infinity;
  const merged = topRule > topAction ? [...ruleItems, ...actionItems] : [...actionItems, ...ruleItems];
  return merged.slice(0, limit);
}

/** The index after a step in the list: it stops at the ends. */
export function stepIndex(current: number, delta: 1 | -1, length: number): number {
  if (length === 0) return 0;
  return Math.min(length - 1, Math.max(0, current + delta));
}
