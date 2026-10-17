// The class a rule is shown with, as the hub said it (`GET /api/rules/sources`, one entry per file). The page works
// out no class of its own: two implementations of the same rule drift apart. A rule is matched to its file by its
// `source`, which is the path of that file; a path the hub does not name, or a class this version does not know,
// is `unclassified` (fail closed).

import type { DataClass, HubSources } from './api';
import type { Key } from './i18n';

const CLASSES: readonly DataClass[] = ['public', 'internal', 'private', 'unclassified'];

/** The class of each file the hub named. */
export type SourceIndex = ReadonlyMap<string, DataClass>;

export function sourceIndex(doc: HubSources | null): SourceIndex {
  const index = new Map<string, DataClass>();
  if (!doc || !Array.isArray(doc.files)) return index;
  for (const file of doc.files) {
    if (typeof file?.path !== 'string') continue;
    const shown = (CLASSES as readonly unknown[]).includes(file.data_class) ? file.data_class : 'unclassified';
    // A path named twice keeps the stricter word: unclassified is not more released than private.
    const before = index.get(file.path);
    index.set(file.path, before === undefined ? shown : stricter(before, shown));
  }
  return index;
}

/** `unclassified` and `private` are both "not to be released"; of two words the one that releases less wins. */
function stricter(a: DataClass, b: DataClass): DataClass {
  const rank = (c: DataClass): number => (c === 'unclassified' ? 3 : c === 'private' ? 2 : c === 'internal' ? 1 : 0);
  return rank(a) >= rank(b) ? a : b;
}

/** The class of a rule by its `source`. */
export function classOf(index: SourceIndex, source: string): DataClass {
  return index.get(source) ?? 'unclassified';
}

/** The key of the label of a class. */
export function classLabel(shown: DataClass): Key {
  return `class.${shown}`;
}

/** How many files of the snapshot the hub did not release, for the overview. */
export function unclassifiedCount(doc: HubSources | null): number {
  return doc && Array.isArray(doc.unclassified) ? doc.unclassified.length : 0;
}
