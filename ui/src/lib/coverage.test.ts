import { describe, expect, it } from 'vitest';
import type { RuleSummary, Snapshot } from './contract';
import { byFamily, byGate, coverageOf, percent } from './coverage';

const rule = (id: string, has_check: boolean, has_agent: boolean): RuleSummary =>
  ({ id, family: id.split('-')[0], has_check, has_agent }) as unknown as RuleSummary;

const rules = [
  rule('DEP-01', true, false),
  rule('DEP-02', true, true),
  rule('DEP-03', false, true),
  rule('DEP-04', false, false),
  rule('ERR-01', false, false),
  rule('LUA-01', true, false),
];

describe('coverage', () => {
  it('counts a rule with a check as checked even when it also has a question, and a rule with neither as manual', () => {
    expect(coverageOf(rules.slice(0, 4))).toEqual({ total: 4, check: 2, agent: 1, manual: 1 });
    expect(coverageOf([])).toEqual({ total: 0, check: 0, agent: 0, manual: 0 });
  });

  it('is one row for each family, by name', () => {
    expect(byFamily(rules)).toEqual([
      { name: 'DEP', total: 4, check: 2, agent: 1, manual: 1 },
      { name: 'ERR', total: 1, check: 0, agent: 0, manual: 1 },
      { name: 'LUA', total: 1, check: 1, agent: 0, manual: 0 },
    ]);
  });

  it('is one row for each gate over the rules of its families, with the families that match nothing', () => {
    const gates = [
      { name: 'review', families: ['DEP', 'NOPE'], unknown: ['NOPE'] },
      { name: 'all', families: ['DEP', 'ERR', 'LUA'] },
      { families: ['DEP'] },
    ] as unknown as Snapshot['gates'];
    expect(byGate(gates, rules)).toEqual([
      { name: 'all', total: 6, check: 3, agent: 1, manual: 2, unknown: [] },
      { name: 'review', total: 4, check: 2, agent: 1, manual: 1, unknown: ['NOPE'] },
    ]);
  });

  it('is a gate without a family that has no rule', () => {
    const gates = [{ name: 'empty', families: [] }] as unknown as Snapshot['gates'];
    expect(byGate(gates, rules)[0]).toMatchObject({ total: 0, manual: 0 });
  });

  it('rounds a share to a whole percent and has none for nothing', () => {
    expect(percent(1, 3)).toBe(33);
    expect(percent(2, 3)).toBe(67);
    expect(percent(0, 0)).toBe(0);
  });
});
