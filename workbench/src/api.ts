// Client for the panel's server file API. The workbench page is served from the panel's own
// origin, so requests carry the user's session cookie like the panel's own frontend does.

export interface DirectoryEntry {
  name: string;
  size: number;
  directory: boolean;
  file: boolean;
  symlink: boolean;
  modified: string;
  created: string;
}

export interface MatchSpan {
  start_byte: number;
  end_byte: number;
}

export interface MatchBlock {
  start_line: number;
  end_line: number;
  content: string;
  matches: MatchSpan[];
}

export interface ContentMatches {
  file: string;
  truncated: boolean;
  blocks: MatchBlock[];
}

export interface SearchResponse {
  entries: DirectoryEntry[];
  content_matches?: ContentMatches[];
}

export interface SearchRequest {
  path_filter: { include: string[]; exclude: string[]; case_insensitive: boolean } | null;
  content_filter: {
    query: string;
    max_search_size: number;
    include_unmatched: boolean;
    case_insensitive: boolean;
  } | null;
  match_context?: { before: number; after: number; max_matches: number };
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

// the panel keeps the impersonated user of a browser tab in sessionStorage, which same-origin frames share
function headers(extra?: Record<string, string>): Record<string, string> {
  const impersonated = sessionStorage.getItem('impersonated_user');
  return { ...(impersonated ? { 'Calagopus-User': impersonated } : {}), ...extra };
}

async function errorFrom(response: Response): Promise<ApiError> {
  let message = `${response.status} ${response.statusText}`;
  try {
    const body = await response.json();
    if (Array.isArray(body?.errors) && typeof body.errors[0] === 'string') message = body.errors[0];
    else if (typeof body?.error === 'string') message = body.error;
  } catch {
    // not a JSON error body
  }
  return new ApiError(message, response.status);
}

export class PanelFiles {
  private readonly base: string;

  constructor(serverUuid: string) {
    this.base = `/api/client/servers/${encodeURIComponent(serverUuid)}/files`;
  }

  private async request(path: string, init: RequestInit = {}): Promise<Response> {
    const response = await fetch(`${this.base}${path}`, {
      credentials: 'same-origin',
      ...init,
      headers: headers(init.headers as Record<string, string> | undefined),
    });
    if (!response.ok) throw await errorFrom(response);
    return response;
  }

  private async json<T>(path: string, method: string, body?: unknown): Promise<T> {
    const response = await this.request(path, {
      method,
      headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    return (await response.json()) as T;
  }

  /** Every entry of a directory; the list endpoint pages at 100 entries at most. */
  async list(directory: string): Promise<DirectoryEntry[]> {
    const entries: DirectoryEntry[] = [];
    for (let page = 1; ; page++) {
      const query = new URLSearchParams({ directory, page: String(page), per_page: '100', sort: 'name_asc' });
      const data = await this.json<{ entries: { total: number; data: DirectoryEntry[] } }>(`/list?${query}`, 'GET');
      entries.push(...data.entries.data);
      if (data.entries.data.length === 0 || entries.length >= data.entries.total) return entries;
    }
  }

  /** The entry for one path, or null when it does not exist. */
  async stat(root: string, name: string): Promise<DirectoryEntry | null> {
    const data = await this.json<{ entries: DirectoryEntry[] }>('/stat', 'POST', { root, files: [name] });
    return data.entries[0] ?? null;
  }

  async read(file: string): Promise<Uint8Array> {
    const response = await this.request(`/contents?${new URLSearchParams({ file })}`);
    return new Uint8Array(await response.arrayBuffer());
  }

  async write(file: string, content: Uint8Array): Promise<void> {
    await this.request(`/write?${new URLSearchParams({ file })}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/octet-stream' },
      body: new Blob([content as Uint8Array<ArrayBuffer>]),
    });
  }

  async createDirectory(root: string, name: string): Promise<void> {
    await this.json('/create-directory', 'POST', { root, name });
  }

  async delete(root: string, name: string): Promise<void> {
    await this.json('/delete', 'POST', { root, files: [name] });
  }

  /** Renames or moves, with both paths relative to the server root. */
  async rename(from: string, to: string): Promise<void> {
    await this.json('/rename', 'PUT', { root: '/', files: [{ from, to }] });
  }

  /** Search below the server root; returned names and files are relative to it. */
  async search(request: SearchRequest): Promise<SearchResponse> {
    return await this.json<SearchResponse>('/search', 'POST', { root: '/', size_filter: null, ...request });
  }
}
