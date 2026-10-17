import { describe, expect, it } from 'vitest';
import { errorDoc, goldenText, hubSourcesDoc, ruleDoc, snapshotDoc } from './fixtures';

describe('the golden documents', () => {
  it('are the kinds of the contract', () => {
    expect(snapshotDoc().kind).toBe('rules.snapshot');
    expect(ruleDoc().kind).toBe('rules.rule');
    expect(errorDoc().kind).toBe('rules.error');
    expect(errorDoc().code).toBe('not_found');
  });

  it('hold the four rules and the rule DEP-01 with its text', () => {
    expect(snapshotDoc().rules.map((r) => r.id)).toEqual(['DEP-01', 'DEP-09', 'ERR-100', 'LUA-01']);
    expect(ruleDoc().rule.id).toBe('DEP-01');
    expect(ruleDoc().rule.text).toBe('Text of one.');
  });

  it('are a fresh copy each time', () => {
    const one = snapshotDoc();
    one.rules.length = 0;
    expect(snapshotDoc().rules).toHaveLength(4);
  });

  it('are the text the hub passes on', () => {
    expect(JSON.parse(goldenText('rule'))).toEqual(ruleDoc());
  });

  it('give the classes for exactly the files of the snapshot', () => {
    const files = snapshotDoc().sources.map((s) => s.path);
    expect(hubSourcesDoc().files.map((f) => f.path)).toEqual(files);
  });
});
