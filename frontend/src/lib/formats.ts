import { type Format, INTEGER_PATTERN, type Node, type NodeKind, type NodePath } from './node.ts';

const FLAT_SCALAR_KINDS: NodeKind[] = ['string', 'integer', 'float', 'boolean'];
const FLAT_FLOAT_PATTERN = /^-?[0-9]+\.[0-9]+$/;

/** properties, env and ini store every value as text; the backend infers the kind from it. */
export const isFlatFormat = (format: Format) => format === 'properties' || format === 'env' || format === 'ini';

/** Kinds a new value may have inside the container at `containerPath` (empty when nothing may be added there). */
export function allowedChildKinds(format: Format, containerPath: NodePath): NodeKind[] {
  const depth = containerPath.length;

  switch (format) {
    case 'yaml':
    case 'json':
      return ['string', 'integer', 'float', 'boolean', 'null', 'array', 'object'];
    case 'toml':
      return ['string', 'integer', 'float', 'boolean', 'datetime', 'array', 'object'];
    case 'properties':
    case 'env':
      return depth === 0 ? FLAT_SCALAR_KINDS : [];
    case 'ini':
      return depth === 0 ? [...FLAT_SCALAR_KINDS, 'object'] : depth === 1 ? FLAT_SCALAR_KINDS : [];
  }
}

/** The node a flat-format value reads back as after being written as `text`. */
export function inferFlatScalar(text: string): Node {
  if (text === 'true' || text === 'false') return { kind: 'boolean', value: text === 'true' };
  if (INTEGER_PATTERN.test(text)) return { kind: 'integer', value: text };
  if (FLAT_FLOAT_PATTERN.test(text)) return { kind: 'float', value: text };
  return { kind: 'string', value: text };
}

/** Monaco language id used for the raw editor. */
export function monacoLanguage(format: Format): string {
  switch (format) {
    case 'yaml':
      return 'yaml';
    case 'json':
      return 'json';
    case 'toml':
      return 'toml';
    case 'properties':
    case 'env':
    case 'ini':
      return 'ini';
  }
}
