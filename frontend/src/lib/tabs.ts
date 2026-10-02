import { type Format, isDocumentValid, type Node, nodesEqual } from './node.ts';

export interface ParseError {
  message: string;
  line: number | null;
  column: number | null;
}

/** A file as returned by `GET /file` and the save endpoints. */
export interface ConfigFile {
  path: string;
  format: Format;
  hash: string;
  content: string;
  document: Node | null;
  error: ParseError | null;
}

export interface EditorTab {
  path: string;
  /** Last state read from or written to the server. */
  file: ConfigFile;
  /** Visual edits; null when the file does not parse. */
  document: Node | null;
  /** Raw text edits. */
  content: string;
  raw: boolean;
}

export function tabFromFile(file: ConfigFile, raw = false): EditorTab {
  return { path: file.path, file, document: file.document, content: file.content, raw: raw || !file.document };
}

export function isDocumentDirty(tab: EditorTab): boolean {
  return !!tab.document && !!tab.file.document && !nodesEqual(tab.document, tab.file.document);
}

export const isTabDirty = (tab: EditorTab) => tab.content !== tab.file.content || isDocumentDirty(tab);

/** Dirty and, in the visual editor, free of invalid numbers/dates. */
export function isTabSavable(tab: EditorTab): boolean {
  if (tab.raw) return tab.content !== tab.file.content;
  return !!tab.document && isDocumentDirty(tab) && isDocumentValid(tab.document);
}

export function upsertTab(tabs: EditorTab[], tab: EditorTab): EditorTab[] {
  return tabs.some((current) => current.path === tab.path)
    ? tabs.map((current) => (current.path === tab.path ? tab : current))
    : [...tabs, tab];
}

/** The tab to activate after closing `path`: the right neighbour, else the left one. */
export function nextActivePath(tabs: EditorTab[], closedPath: string, activePath: string | null): string | null {
  if (activePath !== closedPath) return activePath;

  const index = tabs.findIndex((tab) => tab.path === closedPath);
  const remaining = tabs.filter((tab) => tab.path !== closedPath);
  return remaining[Math.min(index, remaining.length - 1)]?.path ?? null;
}
