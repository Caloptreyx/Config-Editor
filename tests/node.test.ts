import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import {
  appendEntry,
  isDocumentValid,
  isValidFloat,
  moveAt,
  type Node,
  nodesEqual,
  removeAt,
  updateAt,
} from '../frontend/src/lib/node.ts';

const doc: Node = {
  kind: 'object',
  entries: [
    {
      key: 'server',
      comment: 'network settings',
      value: {
        kind: 'object',
        entries: [{ key: 'port', comment: null, value: { kind: 'integer', value: '25565' } }],
      },
    },
    {
      key: 'motd',
      comment: null,
      value: { kind: 'array', items: [{ kind: 'string', value: 'a' }, { kind: 'string', value: 'b' }] },
    },
  ],
};

describe('updateAt', () => {
  test('replaces the target and shares every untouched branch', () => {
    const next = updateAt(doc, [0, 0], () => ({ kind: 'integer', value: '25566' }));

    assert.ok(next.kind === 'object' && doc.kind === 'object');
    assert.notEqual(next, doc);
    assert.equal(next.entries[1], doc.entries[1]);
    assert.deepEqual(next.entries[0].value, {
      kind: 'object',
      entries: [{ key: 'port', comment: null, value: { kind: 'integer', value: '25566' } }],
    });
    assert.ok(doc.entries[0].value.kind === 'object');
    assert.deepEqual(doc.entries[0].value.entries[0].value, { kind: 'integer', value: '25565' }, 'input not mutated');
  });

  test('returns the same root for a missing path or an identity update', () => {
    assert.equal(
      updateAt(doc, [5, 0], () => ({ kind: 'null' })),
      doc,
    );
    assert.equal(
      updateAt(doc, [0, 0], (node) => node),
      doc,
    );
  });
});

describe('structural edits', () => {
  test('moveAt swaps neighbours and ignores moves past either end', () => {
    const moved = moveAt(doc, [1, 0], 1);
    assert.deepEqual(moved.kind === 'object' && moved.entries[1].value, {
      kind: 'array',
      items: [
        { kind: 'string', value: 'b' },
        { kind: 'string', value: 'a' },
      ],
    });
    assert.equal(moveAt(doc, [1, 0], -1), doc);
    assert.equal(moveAt(doc, [1, 1], 1), doc);
  });

  test('removeAt drops only the addressed item', () => {
    const removed = removeAt(doc, [1, 0]);
    assert.deepEqual(removed.kind === 'object' && removed.entries[1].value, {
      kind: 'array',
      items: [{ kind: 'string', value: 'b' }],
    });
  });

  test('appendEntry adds to objects only', () => {
    const added = appendEntry(doc, [0], 'host', { kind: 'string', value: '0.0.0.0' });
    assert.ok(added.kind === 'object' && added.entries[0].value.kind === 'object');
    assert.equal(added.entries[0].value.entries.at(-1)?.key, 'host');
    assert.equal(appendEntry(doc, [1], 'x', { kind: 'null' }), doc);
  });
});

describe('nodesEqual', () => {
  test('ignores comments but not kinds, keys or order', () => {
    const recommented = updateAt(doc, [], (node) =>
      node.kind === 'object'
        ? { ...node, entries: node.entries.map((entry) => ({ ...entry, comment: 'changed' })) }
        : node,
    );
    assert.ok(nodesEqual(doc, recommented));

    assert.ok(!nodesEqual({ kind: 'integer', value: '1' }, { kind: 'string', value: '1' }));
    assert.ok(!nodesEqual({ kind: 'integer', value: '1' }, { kind: 'float', value: '1' }));
    assert.ok(!nodesEqual(doc, moveAt(doc, [0], 1)));
    assert.ok(!nodesEqual({ kind: 'array', items: [] }, { kind: 'object', entries: [] }));
  });

  test('editing a value back to the original is clean again', () => {
    const edited = updateAt(doc, [0, 0], () => ({ kind: 'integer', value: '1' }));
    const restored = updateAt(edited, [0, 0], () => ({ kind: 'integer', value: '25565' }));
    assert.ok(!nodesEqual(doc, edited));
    assert.ok(nodesEqual(doc, restored));
  });
});

describe('validation', () => {
  test('integers must be exact decimal text', () => {
    const integer = (value: string): Node => ({ kind: 'integer', value });
    assert.ok(isDocumentValid(integer('-12345678901234567890')));
    assert.ok(!isDocumentValid(integer('007')));
    assert.ok(!isDocumentValid(integer('1.5')));
    assert.ok(!isDocumentValid(integer('')));
  });

  test('floats are validated loosely, including special values', () => {
    for (const value of ['0.5', '.5', '1e-3', '1_000.5', '-inf', '.inf', 'nan', '+Infinity', '3']) {
      assert.ok(isValidFloat(value), value);
    }
    for (const value of ['', '1.2.3', 'abc', 'e5', '-']) {
      assert.ok(!isValidFloat(value), value);
    }
  });

  test('one invalid value deep in the tree invalidates the document', () => {
    assert.ok(!isDocumentValid(updateAt(doc, [0, 0], () => ({ kind: 'integer', value: 'x' }))));
  });
});
