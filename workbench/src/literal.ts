// Kept free of VS Code imports so the node tests can load it.

/**
 * The longest run of characters every match of a regex must contain, or '' when none can be
 * proven. Only top-level text outside groups and classes counts, and a quantifier drops the
 * character it applies to, so the result is always a substring of any match.
 */
export function longestLiteral(source: string): string {
  let best = '';
  let current = '';
  let depth = 0;
  const flush = () => {
    if (current.length > best.length) best = current;
    current = '';
  };

  for (let index = 0; index < source.length; index++) {
    const char = source[index] as string;
    if (char === '\\') {
      const next = source[++index];
      if (next === undefined) break;
      if (depth > 0) continue;
      if (/[A-Za-z0-9]/.test(next)) flush();
      else current += next;
    } else if (char === '[') {
      // skip the class, honoring escapes and a leading ]
      flush();
      index++;
      if (source[index] === '^') index++;
      if (source[index] === ']') index++;
      while (index < source.length && source[index] !== ']') index += source[index] === '\\' ? 2 : 1;
    } else if (char === '(') {
      flush();
      depth++;
    } else if (char === ')') {
      depth = Math.max(0, depth - 1);
    } else if (char === '|') {
      // alternation at the top level means no literal is required
      if (depth === 0) return '';
    } else if (depth > 0) {
      // inside a group
    } else if ('?*{'.includes(char)) {
      // the previous character becomes optional or repeated an unknown number of times
      current = current.slice(0, -1);
      flush();
      if (char === '{') index = Math.max(index, source.indexOf('}', index));
    } else if (char === '+') {
      // the previous character is still required once, but nothing can follow it directly
      flush();
    } else if ('.^$'.includes(char)) {
      flush();
    } else {
      current += char;
    }
  }
  flush();
  return best;
}
