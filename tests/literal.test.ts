import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import { longestLiteral } from '../workbench/src/literal.ts';

// Wings searches for the extracted literal, so a literal that some match lacks would hide that
// match from the Search view.
function assertSound(pattern: string, samples: string[]) {
  const literal = longestLiteral(pattern);
  const regex = new RegExp(pattern);
  for (const sample of samples) {
    const match = sample.match(regex)?.[0];
    assert.ok(match !== undefined, `${pattern} should match ${sample}`);
    assert.ok(match.includes(literal), `${pattern}: match ${JSON.stringify(match)} lacks ${JSON.stringify(literal)}`);
  }
}

describe('longestLiteral', () => {
  test('plain text is its own literal', () => {
    assert.equal(longestLiteral('max-players'), 'max-players');
  });

  test('escaped metacharacters are literal, escape classes break the run', () => {
    assert.equal(longestLiteral('server\\.properties'), 'server.properties');
    assert.equal(longestLiteral('port\\d+ = on'), ' = on');
  });

  test('quantifiers drop the character they apply to', () => {
    assertSound('colou?r: red', ['color: red', 'colour: red']);
    assertSound('motd=x*!', ['motd=!', 'motd=xxx!']);
    assertSound('ab{0,2}cdef', ['acdef', 'abbcdef']);
    assertSound('view-distance: 1+0', ['view-distance: 10', 'view-distance: 1110']);
  });

  test('groups and classes contribute nothing', () => {
    assertSound('level-(name|seed)=world', ['level-name=world', 'level-seed=world']);
    assertSound('[a-z]+_enabled: true', ['pvp_enabled: true']);
    assertSound('[\\]x]config', [']config', 'xconfig']);
  });

  test('top-level alternation has no required literal', () => {
    assert.equal(longestLiteral('online-mode|white-list'), '');
  });

  test('patterns without any literal yield nothing', () => {
    assert.equal(longestLiteral('\\d+'), '');
    assert.equal(longestLiteral('.*'), '');
  });
});
