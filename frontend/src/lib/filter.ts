import type { Entry, Node } from './node.ts';

/** Whether a scalar value or any key/value below `node` contains the query (trimmed, lower case). */
export function nodeMatches(node: Node, query: string): boolean {
  if (!query) return true;

  switch (node.kind) {
    case 'null':
      return 'null'.includes(query);
    case 'boolean':
      return String(node.value).includes(query);
    case 'array':
      return node.items.some((item) => nodeMatches(item, query));
    case 'object':
      return node.entries.some((entry) => entryMatches(entry, query));
    default:
      return node.value.toLowerCase().includes(query);
  }
}

export const keyMatches = (entry: Entry, query: string) => !query || entry.key.toLowerCase().includes(query);

export const entryMatches = (entry: Entry, query: string) =>
  keyMatches(entry, query) || nodeMatches(entry.value, query);
