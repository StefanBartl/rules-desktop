import { describe, expect, it } from 'vitest';
import { DOCS, entriesOf, titleOf } from './docs';

describe('titleOf', () => {
  it('takes the first level-one heading, else the fallback', () => {
    expect(titleOf('intro\n\n# The Title #\n\n## Sub', 'x')).toBe('The Title');
    expect(titleOf('## only a sub heading', 'x')).toBe('x');
  });
});

describe('entriesOf', () => {
  it('names a document by its file and puts the readme first, the rest by name', () => {
    const list = entriesOf({ '../../../docs/SECURITY.md': '# Sec', '../../../README.md': '# Read', '../../../docs/A.md': 'no heading' });
    expect(list.map((d) => [d.id, d.title])).toEqual([
      ['README', 'Read'],
      ['A', 'A'],
      ['SECURITY', 'Sec'],
    ]);
  });
});

describe('the bundled documents', () => {
  it('hold the readme and the files of docs/, and not the instructions for the agent', () => {
    const ids = DOCS.map((d) => d.id);
    expect(ids[0]).toBe('README');
    expect(ids).toContain('SECURITY');
    expect(ids).toContain('USAGE');
    expect(ids).not.toContain('CLAUDE');
    expect(DOCS.every((d) => d.text.length > 0)).toBe(true);
  });
});
