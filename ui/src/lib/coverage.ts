// Which rule applies how: a check that runs by itself, a question for an agent, or manual. The engine says it for
// each rule (`has_check`, `has_agent`); the page only adds up, per family and per gate. A rule with a check counts as
// checked even when it also has a question for an agent (the check is what runs); a rule with neither is manual.

import type { RuleSummary, Snapshot } from './contract';
import { byText } from './filter';

/** The three ways a rule is covered, and their sum. */
export interface Coverage {
  total: number;
  /** A check that runs by itself. */
  check: number;
  /** No check, but a question for an agent. */
  agent: number;
  /** Neither: a person works it. */
  manual: number;
}

export interface CoverageRow extends Coverage {
  /** The family, or the name of the gate. */
  name: string;
}

export interface GateRow extends CoverageRow {
  /** The families of the gate that match no loaded rule. */
  unknown: string[];
}

type Counted = Pick<RuleSummary, 'has_check' | 'has_agent'>;

export function coverageOf(rules: readonly Counted[]): Coverage {
  const out: Coverage = { total: rules.length, check: 0, agent: 0, manual: 0 };
  for (const rule of rules) {
    if (rule.has_check) out.check += 1;
    else if (rule.has_agent) out.agent += 1;
    else out.manual += 1;
  }
  return out;
}

/** One row for each family that has a rule, by name. */
export function byFamily(rules: readonly RuleSummary[]): CoverageRow[] {
  const groups = new Map<string, RuleSummary[]>();
  for (const rule of rules) {
    const list = groups.get(rule.family);
    if (list) list.push(rule);
    else groups.set(rule.family, [rule]);
  }
  return [...groups.entries()]
    .map(([name, list]) => ({ name, ...coverageOf(list) }))
    .sort((a, b) => byText(a.name, b.name));
}

/** One row for each gate: the rules of its families (a gate is a list of families, not a property of a rule). */
export function byGate(gates: Snapshot['gates'], rules: readonly RuleSummary[]): GateRow[] {
  const rows: GateRow[] = [];
  for (const gate of gates) {
    if (typeof gate.name !== 'string' || gate.name === '') continue;
    const families = new Set(Array.isArray(gate.families) ? gate.families : []);
    const unknown = Array.isArray(gate.unknown) ? gate.unknown.filter((u) => typeof u === 'string') : [];
    rows.push({ name: gate.name, ...coverageOf(rules.filter((r) => families.has(r.family))), unknown });
  }
  return rows.sort((a, b) => byText(a.name, b.name));
}

/** The share of `part` in `whole` as a whole percent; 0 for an empty whole. */
export function percent(part: number, whole: number): number {
  return whole === 0 ? 0 : Math.round((part / whole) * 100);
}
