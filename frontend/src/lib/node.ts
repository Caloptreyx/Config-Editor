export const FORMATS = ['yaml', 'json', 'toml', 'properties', 'env', 'ini'] as const;
export type Format = (typeof FORMATS)[number];

export type Node =
  | { kind: 'string'; value: string }
  | { kind: 'integer'; value: string }
  | { kind: 'float'; value: string }
  | { kind: 'boolean'; value: boolean }
  | { kind: 'null' }
  | { kind: 'datetime'; value: string }
  | { kind: 'array'; items: Node[] }
  | { kind: 'object'; entries: Entry[] };

export interface Entry {
  key: string;
  value: Node;
  comment: string | null;
}

export type NodeKind = Node['kind'];
export type NodeOf<K extends NodeKind> = Extract<Node, { kind: K }>;
export type ContainerNode = NodeOf<'array'> | NodeOf<'object'>;
export type TextNode = NodeOf<'string'> | NodeOf<'integer'> | NodeOf<'float'> | NodeOf<'datetime'>;

/** Child indices from the root: an entry index for objects, an item index for arrays. */
export type NodePath = readonly number[];

export const INTEGER_PATTERN = /^-?(0|[1-9][0-9]*)$/;
// loose on purpose: the backend decides with the target format's own float grammar
const FLOAT_PATTERN = /^[+-]?([0-9][0-9_]*(\.[0-9_]*)?|\.[0-9][0-9_]*)([eE][+-]?[0-9_]+)?$/;
const FLOAT_SPECIAL_PATTERN = /^[+-]?\.?(inf|infinity|nan)$/i;

export const isValidFloat = (text: string) => FLOAT_PATTERN.test(text) || FLOAT_SPECIAL_PATTERN.test(text);

export const isContainer = (node: Node): node is ContainerNode => node.kind === 'array' || node.kind === 'object';

export function isValidScalar(node: Node): boolean {
  if (node.kind === 'integer') return INTEGER_PATTERN.test(node.value);
  if (node.kind === 'float') return isValidFloat(node.value);
  if (node.kind === 'datetime') return node.value.trim().length > 0;
  return true;
}

/** True when every integer/float/datetime in the tree holds a value the backend can accept. */
export function isDocumentValid(node: Node): boolean {
  if (node.kind === 'array') return node.items.every(isDocumentValid);
  if (node.kind === 'object') return node.entries.every((entry) => isDocumentValid(entry.value));
  return isValidScalar(node);
}

export function defaultNode(kind: NodeKind): Node {
  switch (kind) {
    case 'string':
      return { kind, value: '' };
    case 'integer':
      return { kind, value: '0' };
    case 'float':
      return { kind, value: '0.0' };
    case 'boolean':
      return { kind, value: false };
    case 'null':
      return { kind };
    case 'datetime':
      return { kind, value: '1970-01-01T00:00:00Z' };
    case 'array':
      return { kind, items: [] };
    case 'object':
      return { kind, entries: [] };
  }
}

function childAt(node: Node, index: number): Node | undefined {
  if (node.kind === 'array') return node.items[index];
  if (node.kind === 'object') return node.entries[index]?.value;
  return undefined;
}

function replaceChild(node: Node, index: number, child: Node): Node {
  if (node.kind === 'array') return { ...node, items: node.items.map((item, i) => (i === index ? child : item)) };
  if (node.kind === 'object') {
    return { ...node, entries: node.entries.map((entry, i) => (i === index ? { ...entry, value: child } : entry)) };
  }
  return node;
}

/** Immutably replaces the node at `path`; untouched branches keep their references. */
export function updateAt(root: Node, path: NodePath, update: (node: Node) => Node): Node {
  if (path.length === 0) return update(root);

  const [index, ...rest] = path;
  const child = childAt(root, index);
  if (!child) return root;

  const next = updateAt(child, rest, update);
  return next === child ? root : replaceChild(root, index, next);
}

const updateContainer = (root: Node, path: NodePath, update: (node: ContainerNode) => ContainerNode): Node =>
  updateAt(root, path, (node) => (isContainer(node) ? update(node) : node));

function withoutIndex<T>(items: T[], index: number): T[] {
  return items.filter((_, i) => i !== index);
}

function swapped<T>(items: T[], index: number, target: number): T[] {
  if (index < 0 || target < 0 || index >= items.length || target >= items.length) return items;

  const next = [...items];
  [next[index], next[target]] = [next[target], next[index]];
  return next;
}

/** Removes the entry or item at `path` (which must point below the root). */
export function removeAt(root: Node, path: NodePath): Node {
  if (path.length === 0) return root;

  const index = path[path.length - 1];
  return updateContainer(root, path.slice(0, -1), (parent) =>
    parent.kind === 'array'
      ? { ...parent, items: withoutIndex(parent.items, index) }
      : { ...parent, entries: withoutIndex(parent.entries, index) },
  );
}

/** Swaps the entry or item at `path` with its neighbour; out-of-range moves leave the tree unchanged. */
export function moveAt(root: Node, path: NodePath, offset: -1 | 1): Node {
  if (path.length === 0) return root;

  const index = path[path.length - 1];
  return updateContainer(root, path.slice(0, -1), (parent) => {
    if (parent.kind === 'array') {
      const items = swapped(parent.items, index, index + offset);
      return items === parent.items ? parent : { ...parent, items };
    }
    const entries = swapped(parent.entries, index, index + offset);
    return entries === parent.entries ? parent : { ...parent, entries };
  });
}

export function appendItem(root: Node, arrayPath: NodePath, item: Node): Node {
  return updateContainer(root, arrayPath, (node) =>
    node.kind === 'array' ? { ...node, items: [...node.items, item] } : node,
  );
}

export function appendEntry(root: Node, objectPath: NodePath, key: string, value: Node): Node {
  return updateContainer(root, objectPath, (node) =>
    node.kind === 'object' ? { ...node, entries: [...node.entries, { key, value, comment: null }] } : node,
  );
}

/** Structural equality of documents, ignoring entry comments (they are response-only). */
export function nodesEqual(a: Node, b: Node): boolean {
  if (a === b) return true;

  switch (a.kind) {
    case 'null':
      return b.kind === 'null';
    case 'array':
      return (
        b.kind === 'array' &&
        a.items.length === b.items.length &&
        a.items.every((item, i) => nodesEqual(item, b.items[i]))
      );
    case 'object':
      return (
        b.kind === 'object' &&
        a.entries.length === b.entries.length &&
        a.entries.every((entry, i) => entry.key === b.entries[i].key && nodesEqual(entry.value, b.entries[i].value))
      );
    default:
      return b.kind === a.kind && 'value' in b && b.value === a.value;
  }
}
