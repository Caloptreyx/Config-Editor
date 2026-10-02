import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import type { Node } from '../frontend/src/lib/node.ts';
import { type ConfigFile, isTabDirty, isTabSavable, nextActivePath, tabFromFile } from '../frontend/src/lib/tabs.ts';

const file = (document: Node | null, content = 'a=1\n'): ConfigFile => ({
  path: '/server.properties',
  format: 'properties',
  hash: 'h',
  content,
  document,
  error: document ? null : { message: 'bad', line: 1, column: 2 },
});

const parsed: Node = {
  kind: 'object',
  entries: [{ key: 'a', comment: 'first', value: { kind: 'integer', value: '1' } }],
};

describe('tabs', () => {
  test('unparsable files open in raw mode', () => {
    assert.equal(tabFromFile(file(null)).raw, true);
    assert.equal(tabFromFile(file(parsed)).raw, false);
  });

  test('dirty tracks both editors and clears when values return to the original', () => {
    const tab = tabFromFile(file(parsed));
    assert.ok(!isTabDirty(tab));
    assert.ok(isTabDirty({ ...tab, content: 'a=2\n' }));

    const edited = {
      ...tab,
      document: { kind: 'object', entries: [{ key: 'a', comment: null, value: { kind: 'integer', value: '2' } }] },
    } satisfies typeof tab;
    assert.ok(isTabDirty(edited));
    assert.ok(
      !isTabDirty({
        ...tab,
        document: { kind: 'object', entries: [{ key: 'a', comment: null, value: { kind: 'integer', value: '1' } }] },
      }),
    );
  });

  test('invalid numbers block saving in the visual editor only', () => {
    const tab = tabFromFile(file(parsed));
    const invalid = {
      ...tab,
      document: { kind: 'object', entries: [{ key: 'a', comment: null, value: { kind: 'integer', value: '1.5' } }] },
    } satisfies typeof tab;
    assert.ok(isTabDirty(invalid));
    assert.ok(!isTabSavable(invalid));
    assert.ok(isTabSavable({ ...tab, raw: true, content: 'a=1.5\n' }));
    assert.ok(!isTabSavable({ ...tab, raw: true }));
  });

  test('closing the active tab activates its right neighbour, else the left one', () => {
    const tabs = ['/a.yml', '/b.yml', '/c.yml'].map((path) => tabFromFile({ ...file(parsed), path }));
    assert.equal(nextActivePath(tabs, '/b.yml', '/b.yml'), '/c.yml');
    assert.equal(nextActivePath(tabs, '/c.yml', '/c.yml'), '/b.yml');
    assert.equal(nextActivePath(tabs, '/a.yml', '/c.yml'), '/c.yml');
    assert.equal(nextActivePath(tabs.slice(0, 1), '/a.yml', '/a.yml'), null);
  });
});
