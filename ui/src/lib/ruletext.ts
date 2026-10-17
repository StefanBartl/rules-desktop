// The words of a rule as the page shows them. Text from a rule set is untrusted: it is only ever shown as text, and
// characters that hide or reorder what is read are made visible (the guard of appkit-ui).

import { hasHidden, markHidden } from 'appkit-ui/text';

/**
 * The title without the id the engine puts in front of it (`` `DEP-01` — one `` is `one`): the id stands in a column
 * of its own. A title that does not start with its own id, or that would be empty without it, stays as it is.
 */
export function titleText(title: string, id: string): string {
  const head = `\`${id}\``;
  if (!title.startsWith(head)) return title;
  const rest = title.slice(head.length);
  const match = /^\s*[\u2014\u2013-]\s*/.exec(rest);
  if (!match) return title;
  const text = rest.slice(match[0].length);
  return text.trim() === '' ? title : text;
}

/** Text as it is shown: every hidden character as ⟦U+XXXX⟧. */
export function shownText(text: string): string {
  return markHidden(text);
}

/** Does the text hold a hidden character (the detail then says so)? */
export function holdsHidden(text: string): boolean {
  return hasHidden(text);
}

/** `Checklists/DEP.md:5`: where the rule is, to copy. */
export function sourceLine(source: string, line: number): string {
  return Number.isInteger(line) && line > 0 ? `${source}:${line}` : source;
}

/** A check or an agent of a rule as indented JSON, for a `<pre>`: data to read, never code to run. */
export function asJson(value: unknown): string {
  return markHidden(JSON.stringify(value, null, 2) ?? '');
}
