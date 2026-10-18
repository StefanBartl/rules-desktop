import { describe, expect, it } from 'vitest';
import { draftOf, patchOf, unchanged, type Body } from './edit';

function r(extra: Record<string, unknown> = {}): Body {
  return {
    id: 'DEP-01',
    title: '`DEP-01` — one',
    severity: 'recommended',
    text: 'The text.\n',
    editable: true,
    etag: 'sha256:1',
    etag_scope: 'rule',
    check: { type: 'grep', pattern: 'x' },
    ...extra,
  } as unknown as Body;
}

describe('the draft', () => {
  it('starts as the rule is, with the title without its id and the data as JSON', () => {
    const d = draftOf(r());
    expect(d.title).toBe('one');
    expect(d.severity).toBe('recommended');
    expect(d.text).toBe('The text.\n');
    expect(JSON.parse(d.check)).toEqual({ type: 'grep', pattern: 'x' });
    expect(d.agent).toBe('');
    expect(unchanged(r(), d)).toBe(true);
  });
});

describe('the patch', () => {
  it('holds only what changed', () => {
    const rl = r();
    const d = draftOf(rl);
    expect(patchOf(rl, d)).toEqual({ ok: true, patch: {} });
    expect(patchOf(rl, { ...d, severity: 'critical' })).toEqual({ ok: true, patch: { severity: 'critical' } });
    expect(patchOf(rl, { ...d, title: '  other  ' })).toEqual({ ok: true, patch: { title: 'other' } });
    expect(patchOf(rl, { ...d, text: 'New.\n' })).toEqual({ ok: true, patch: { text: 'New.\n' } });
    expect(unchanged(rl, { ...d, text: 'New.\n' })).toBe(false);
  });

  it('refuses a title that is empty or on several lines before the engine is asked', () => {
    const rl = r();
    const d = draftOf(rl);
    expect(patchOf(rl, { ...d, title: '  ' })).toEqual({ ok: false, field: 'title', reason: 'empty' });
    expect(patchOf(rl, { ...d, title: 'a\nb' })).toEqual({ ok: false, field: 'title', reason: 'multiline' });
  });

  it('reads check and agent as JSON objects, takes them away when empty and refuses anything else', () => {
    const rl = r();
    const d = draftOf(rl);
    expect(patchOf(rl, { ...d, check: '' })).toEqual({ ok: true, patch: { check: null } });
    expect(patchOf(rl, { ...d, agent: '{"question":"q"}' })).toEqual({ ok: true, patch: { agent: { question: 'q' } } });
    for (const bad of ['{', '[]', '7', '"x"', 'null']) {
      expect(patchOf(rl, { ...d, check: bad }), bad).toEqual({ ok: false, field: 'check', reason: 'json' });
    }
    // The same text with other white space is not a change.
    expect(patchOf(rl, { ...d, check: `  ${d.check}\n` })).toEqual({ ok: true, patch: {} });
  });

  it('leaves check and agent alone for a rule whose check is a Lua function', () => {
    const rl = r({ editable: false, check: undefined });
    const d = draftOf(rl);
    expect(patchOf(rl, { ...d, check: '{"type":"grep"}', agent: '{"q":1}', severity: 'critical' })).toEqual({
      ok: true,
      patch: { severity: 'critical' },
    });
  });
});
