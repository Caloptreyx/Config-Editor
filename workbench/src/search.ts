import type { CancellationToken } from '@codingame/monaco-vscode-api/vscode/vs/base/common/cancellation';
import * as glob from '@codingame/monaco-vscode-api/vscode/vs/base/common/glob';
import { createRegExp } from '@codingame/monaco-vscode-api/vscode/vs/base/common/strings';
import { URI } from '@codingame/monaco-vscode-api/vscode/vs/base/common/uri';
import {
  FileMatch,
  type IFileQuery,
  type ISearchComplete,
  type ISearchProgressItem,
  type ISearchResultProvider,
  type ITextQuery,
  SearchError,
  SearchErrorCode,
  TextSearchMatch,
} from '@codingame/monaco-vscode-api/vscode/vs/workbench/services/search/common/search';
import { TextSearchCompleteMessageType } from '@codingame/monaco-vscode-api/vscode/vs/workbench/services/search/common/searchExtTypes';
import type { ContentMatches, PanelFiles } from './api.ts';
import { longestLiteral } from './literal.ts';

// Wings stops collecting matches in a file after this many.
const MATCHES_PER_FILE = 100;

/** Glob patterns of a VS Code include/exclude expression that are plain `true` entries. */
function globsOf(expression: glob.IExpression | undefined): string[] {
  return Object.entries(expression ?? {})
    .filter(([, value]) => value === true)
    .map(([pattern]) => pattern);
}

function complete(results: FileMatch[], limitHit: boolean, messages: string[] = []): ISearchComplete {
  return {
    results,
    limitHit,
    messages: messages.map((text) => ({ text, type: TextSearchCompleteMessageType.Warning })),
    stats: { type: 'textSearchProvider' },
  };
}

/**
 * Quick Open and the Search view, both answered by the panel's Wings-side file search so the
 * workbench never walks the server tree itself. Wings matches literal text only; regex and
 * whole-word queries search for their literal-free form server-side where possible and are
 * then verified line by line in the browser.
 */
export class ServerSearchProvider implements ISearchResultProvider {
  constructor(
    private readonly files: PanelFiles,
    private readonly maxContentSearchSize: () => number,
  ) {}

  async getAIName(): Promise<string | undefined> {
    return undefined;
  }

  async clearCache(): Promise<void> {}

  async fileSearch(query: IFileQuery, token?: CancellationToken): Promise<ISearchComplete> {
    const pattern = query.filePattern?.trim().replace(/[\\/]/g, '') ?? '';
    // Quick Open warms a cache with an empty pattern on every open, which would make Wings list
    // the whole server; the picker shows recently opened files until something is typed
    if (!pattern || token?.isCancellationRequested) return complete([], false);

    // fuzzy picker input: every typed character in order, like VS Code's own file search
    const fuzzy = [...pattern].map((char) => (/[*?[\]{}!]/.test(char) ? `[${char}]` : char)).join('*');
    const response = await this.files.search({
      path_filter: {
        include: [...globsOf(query.includePattern), `**/*${fuzzy}*`],
        exclude: globsOf(query.excludePattern),
        case_insensitive: true,
      },
      content_filter: null,
    });
    if (token?.isCancellationRequested) return complete([], false);

    const max = query.maxResults ?? Number.POSITIVE_INFINITY;
    const results = response.entries
      .filter((entry) => !entry.directory)
      .slice(0, max)
      .map((entry) => new FileMatch(URI.file(`/${entry.name}`)));
    return complete(results, response.entries.length > results.length);
  }

  async textSearch(
    query: ITextQuery,
    onProgress?: (item: ISearchProgressItem) => void,
    token?: CancellationToken,
  ): Promise<ISearchComplete> {
    const { pattern, isRegExp, isCaseSensitive, isWordMatch, isMultiline } = query.contentPattern;

    let matcher: RegExp;
    try {
      matcher = createRegExp(pattern, !!isRegExp, {
        matchCase: !!isCaseSensitive,
        wholeWord: !!isWordMatch,
        multiline: !!isMultiline,
        global: true,
        unicode: true,
      });
    } catch (error) {
      throw new SearchError(String(error), SearchErrorCode.regexParseError);
    }

    // the longest literal run Wings can look for; a regex without one cannot be searched server-side
    const needle = isRegExp ? longestLiteral(pattern) : pattern;
    if (!needle) {
      return complete([], false, ['Regular expressions need at least one literal character to search on the server.']);
    }

    const response = await this.files.search({
      path_filter: {
        include: globsOf(query.includePattern),
        exclude: globsOf(query.excludePattern),
        case_insensitive: true,
      },
      content_filter: {
        query: needle,
        max_search_size: Math.min(query.maxFileSize ?? Number.POSITIVE_INFINITY, this.maxContentSearchSize()),
        include_unmatched: false,
        case_insensitive: !isCaseSensitive,
      },
      match_context: { before: 0, after: 0, max_matches: MATCHES_PER_FILE },
    });
    if (token?.isCancellationRequested) return complete([], false);

    const max = query.maxResults ?? Number.POSITIVE_INFINITY;
    const results: FileMatch[] = [];
    let count = 0;
    let truncated = false;

    for (const contentMatches of response.content_matches ?? []) {
      const fileMatch = this.matchesIn(contentMatches, matcher, query);
      truncated ||= contentMatches.truncated;
      if (fileMatch.results.length === 0) continue;

      const room = max - count;
      if (fileMatch.results.length > room) fileMatch.results = fileMatch.results.slice(0, room);
      count += fileMatch.results.length;
      results.push(fileMatch);
      onProgress?.(fileMatch);
      if (count >= max) return complete(results, true);
    }

    return complete(
      results,
      false,
      truncated ? [`Some files had more than ${MATCHES_PER_FILE} matches; only the first ones are listed.`] : [],
    );
  }

  /** The VS Code matches of one Wings result: each line of each block is checked with the real matcher. */
  private matchesIn(contentMatches: ContentMatches, matcher: RegExp, query: ITextQuery): FileMatch {
    const fileMatch = new FileMatch(URI.file(`/${contentMatches.file}`));
    for (const block of contentMatches.blocks) {
      const lines = block.content.replace(/\r?\n$/, '').split(/\r?\n/);
      lines.forEach((line, offset) => {
        const lineNumber = block.start_line - 1 + offset;
        const ranges = [];
        matcher.lastIndex = 0;
        for (let match = matcher.exec(line); match; match = matcher.exec(line)) {
          if (match[0].length === 0) {
            matcher.lastIndex++;
            continue;
          }
          ranges.push({
            startLineNumber: lineNumber,
            startColumn: match.index,
            endLineNumber: lineNumber,
            endColumn: match.index + match[0].length,
          });
        }
        if (ranges.length > 0) fileMatch.results.push(new TextSearchMatch(line, ranges, query.previewOptions));
      });
    }
    return fileMatch;
  }
}
