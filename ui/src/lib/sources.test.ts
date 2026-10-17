import { describe, expect, it } from 'vitest';
import type { HubSources } from './api';
import { hubSourcesDoc, snapshotDoc } from './fixtures';
import { classLabel, classOf, sourceIndex, unclassifiedCount } from './sources';

describe('the class of a rule', () => {
  it('is the class the hub gave the file of its source', () => {
    const index = sourceIndex(hubSourcesDoc());
    expect(snapshotDoc().rules.map((r) => classOf(index, r.source))).toEqual([
      'internal',
      'internal',
      'private',
      'internal',
    ]);
  });

  it('is unclassified for a file the hub does not name, even under a classified folder', () => {
    // The page works out nothing from the name: the hub said nothing of this path.
    const index = sourceIndex(hubSourcesDoc());
    expect(classOf(index, 'Checklists/NEW.md')).toBe('unclassified');
    expect(classOf(index, 'checklists/DEP.md')).toBe('unclassified');
    expect(classOf(sourceIndex(null), 'Checklists/DEP.md')).toBe('unclassified');
  });

  it('is unclassified for a word this version does not know', () => {
    const doc = hubSourcesDoc();
    (doc.files[0] as { data_class: string }).data_class = 'open-to-all';
    expect(classOf(sourceIndex(doc), 'Checklists/DEP.md')).toBe('unclassified');
  });

  it('of a path named twice is the one that releases less', () => {
    const doc: HubSources = {
      ...hubSourcesDoc(),
      files: [
        { path: 'x/A.md', data_class: 'private', rules: 1, key: 'x' },
        { path: 'x/A.md', data_class: 'public', rules: 1, key: 'x' },
      ],
    };
    expect(classOf(sourceIndex(doc), 'x/A.md')).toBe('private');
  });

  it('survives an answer that is not the hub shape', () => {
    expect(sourceIndex({ files: 'nope' } as unknown as HubSources).size).toBe(0);
    expect(unclassifiedCount({ unclassified: null } as unknown as HubSources)).toBe(0);
  });

  it('has a label and a count for the overview', () => {
    expect(classLabel('private')).toBe('class.private');
    expect(unclassifiedCount({ ...hubSourcesDoc(), unclassified: ['a/B.md'] })).toBe(1);
    expect(unclassifiedCount(null)).toBe(0);
  });
});
