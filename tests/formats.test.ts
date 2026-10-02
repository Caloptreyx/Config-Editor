import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import { allowedChildKinds, inferFlatScalar } from '../frontend/src/lib/formats.ts';
import type { Node } from '../frontend/src/lib/node.ts';

describe('allowedChildKinds', () => {
  test('json and yaml allow everything but datetime at any depth', () => {
    for (const format of ['json', 'yaml'] as const) {
      const kinds = allowedChildKinds(format, [0, 3, 1]);
      assert.ok(kinds.includes('null') && kinds.includes('array') && kinds.includes('object'));
      assert.ok(!kinds.includes('datetime'));
    }
  });

  test('toml allows datetime but never null', () => {
    const kinds = allowedChildKinds('toml', [0]);
    assert.ok(kinds.includes('datetime'));
    assert.ok(!kinds.includes('null'));
  });

  test('properties and env are flat scalar maps', () => {
    for (const format of ['properties', 'env'] as const) {
      assert.deepEqual(allowedChildKinds(format, []), ['string', 'integer', 'float', 'boolean']);
      assert.deepEqual(allowedChildKinds(format, [0]), []);
    }
  });

  test('ini allows sections at the root and only scalars inside them', () => {
    assert.ok(allowedChildKinds('ini', []).includes('object'));
    assert.deepEqual(allowedChildKinds('ini', [2]), ['string', 'integer', 'float', 'boolean']);
    assert.deepEqual(allowedChildKinds('ini', [2, 0]), []);
  });
});

describe('inferFlatScalar', () => {
  test('follows the backend inference exactly', () => {
    const cases: [string, Node][] = [
      ['true', { kind: 'boolean', value: true }],
      ['false', { kind: 'boolean', value: false }],
      ['True', { kind: 'string', value: 'True' }],
      ['25565', { kind: 'integer', value: '25565' }],
      ['-1', { kind: 'integer', value: '-1' }],
      ['007', { kind: 'string', value: '007' }],
      ['0.5', { kind: 'float', value: '0.5' }],
      ['1.', { kind: 'string', value: '1.' }],
      ['1e3', { kind: 'string', value: '1e3' }],
      ['', { kind: 'string', value: '' }],
      [' 1', { kind: 'string', value: ' 1' }],
    ];
    for (const [text, expected] of cases) {
      assert.deepEqual(inferFlatScalar(text), expected, JSON.stringify(text));
    }
  });
});
