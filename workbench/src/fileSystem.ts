import { Emitter, Event } from '@codingame/monaco-vscode-api/vscode/vs/base/common/event';
import { Disposable, type IDisposable } from '@codingame/monaco-vscode-api/vscode/vs/base/common/lifecycle';
import type { URI } from '@codingame/monaco-vscode-api/vscode/vs/base/common/uri';
import {
  createFileSystemProviderError,
  FileChangeType,
  FilePermission,
  FileSystemProviderCapabilities,
  FileSystemProviderErrorCode,
  FileType,
  type IFileChange,
  type IFileDeleteOptions,
  type IFileOverwriteOptions,
  type IFileSystemProviderWithFileReadWriteCapability,
  type IFileWriteOptions,
  type IStat,
} from '@codingame/monaco-vscode-api/vscode/vs/platform/files/common/files';
import { ApiError, type DirectoryEntry, type PanelFiles } from './api.ts';

export interface Permissions {
  write: boolean;
  delete: boolean;
  rename: boolean;
}

function parentOf(path: string): string {
  const index = path.lastIndexOf('/');
  return index <= 0 ? '/' : path.slice(0, index);
}

function nameOf(path: string): string {
  return path.slice(path.lastIndexOf('/') + 1);
}

function typeOf(entry: DirectoryEntry): FileType {
  const base = entry.directory ? FileType.Directory : FileType.File;
  return entry.symlink ? base | FileType.SymbolicLink : base;
}

function toError(error: unknown, path: string): Error {
  if (error instanceof ApiError) {
    const code =
      error.status === 404
        ? FileSystemProviderErrorCode.FileNotFound
        : error.status === 401 || error.status === 403
          ? FileSystemProviderErrorCode.NoPermissions
          : error.status === 413
            ? FileSystemProviderErrorCode.FileTooLarge
            : FileSystemProviderErrorCode.Unknown;
    return createFileSystemProviderError(`${path}: ${error.message}`, code);
  }
  return error instanceof Error ? error : new Error(String(error));
}

/**
 * The server's files as VS Code's `file://` scheme, backed by the panel API so that reads and
 * writes go through Wings with the user's permissions, ignored files, revisions and activity log.
 * Directory listings are cached briefly because the workbench stats the same paths repeatedly.
 */
export class ServerFileSystemProvider extends Disposable implements IFileSystemProviderWithFileReadWriteCapability {
  readonly capabilities: FileSystemProviderCapabilities;
  readonly onDidChangeCapabilities = Event.None;

  private readonly changes = this._register(new Emitter<readonly IFileChange[]>());
  readonly onDidChangeFile = this.changes.event;

  private readonly listings = new Map<string, { at: number; entries: Promise<Map<string, DirectoryEntry>> }>();

  constructor(
    private readonly files: PanelFiles,
    private readonly permissions: Permissions,
  ) {
    super();
    this.capabilities =
      FileSystemProviderCapabilities.FileReadWrite |
      FileSystemProviderCapabilities.PathCaseSensitive |
      (permissions.write ? 0 : FileSystemProviderCapabilities.Readonly);
  }

  private listing(directory: string): Promise<Map<string, DirectoryEntry>> {
    const cached = this.listings.get(directory);
    if (cached && Date.now() - cached.at < 2000) return cached.entries;

    const entries = this.files
      .list(directory)
      .then((list) => new Map(list.map((entry) => [entry.name, entry])))
      .catch((error) => {
        this.listings.delete(directory);
        throw toError(error, directory);
      });
    this.listings.set(directory, { at: Date.now(), entries });
    return entries;
  }

  private invalidate(...paths: string[]): void {
    for (const path of paths) {
      this.listings.delete(path);
      this.listings.delete(parentOf(path));
    }
  }

  watch(): IDisposable {
    return Disposable.None;
  }

  async stat(resource: URI): Promise<IStat> {
    const path = resource.path;
    if (path === '/' || path === '') {
      return { type: FileType.Directory, ctime: 0, mtime: 0, size: 0 };
    }

    const entry = (await this.listing(parentOf(path))).get(nameOf(path));
    if (!entry) throw createFileSystemProviderError(`${path} not found`, FileSystemProviderErrorCode.FileNotFound);

    return {
      type: typeOf(entry),
      ctime: Date.parse(entry.created) || 0,
      mtime: Date.parse(entry.modified) || 0,
      size: entry.size,
      permissions: this.permissions.write ? undefined : FilePermission.Readonly,
    };
  }

  async readdir(resource: URI): Promise<[string, FileType][]> {
    const entries = await this.listing(resource.path || '/');
    return [...entries.values()].map((entry) => [entry.name, typeOf(entry)]);
  }

  async readFile(resource: URI): Promise<Uint8Array> {
    try {
      return await this.files.read(resource.path);
    } catch (error) {
      throw toError(error, resource.path);
    }
  }

  async writeFile(resource: URI, content: Uint8Array, opts: IFileWriteOptions): Promise<void> {
    const path = resource.path;
    const existing = (await this.listing(parentOf(path))).get(nameOf(path));
    if (existing && !opts.overwrite) {
      throw createFileSystemProviderError(`${path} already exists`, FileSystemProviderErrorCode.FileExists);
    }
    if (!existing && !opts.create) {
      throw createFileSystemProviderError(`${path} not found`, FileSystemProviderErrorCode.FileNotFound);
    }
    if (existing?.directory) {
      throw createFileSystemProviderError(`${path} is a directory`, FileSystemProviderErrorCode.FileIsADirectory);
    }

    try {
      await this.files.write(path, content);
    } catch (error) {
      throw toError(error, path);
    } finally {
      this.invalidate(path);
    }
    this.changes.fire([{ type: existing ? FileChangeType.UPDATED : FileChangeType.ADDED, resource }]);
  }

  async mkdir(resource: URI): Promise<void> {
    const path = resource.path;
    try {
      await this.files.createDirectory(parentOf(path), nameOf(path));
    } catch (error) {
      throw toError(error, path);
    } finally {
      this.invalidate(path);
    }
    this.changes.fire([{ type: FileChangeType.ADDED, resource }]);
  }

  async delete(resource: URI, _opts: IFileDeleteOptions): Promise<void> {
    const path = resource.path;
    if (!this.permissions.delete) {
      throw createFileSystemProviderError(`${path}: missing permission`, FileSystemProviderErrorCode.NoPermissions);
    }
    try {
      await this.files.delete(parentOf(path), nameOf(path));
    } catch (error) {
      throw toError(error, path);
    } finally {
      this.invalidate(path);
      this.listings.delete(path);
    }
    this.changes.fire([{ type: FileChangeType.DELETED, resource }]);
  }

  async rename(from: URI, to: URI, opts: IFileOverwriteOptions): Promise<void> {
    if (!this.permissions.rename) {
      throw createFileSystemProviderError(`${from.path}: missing permission`, FileSystemProviderErrorCode.NoPermissions);
    }
    if (!opts.overwrite && (await this.listing(parentOf(to.path))).has(nameOf(to.path))) {
      throw createFileSystemProviderError(`${to.path} already exists`, FileSystemProviderErrorCode.FileExists);
    }
    try {
      await this.files.rename(from.path.replace(/^\//, ''), to.path.replace(/^\//, ''));
    } catch (error) {
      throw toError(error, from.path);
    } finally {
      this.invalidate(from.path, to.path);
      this.listings.delete(from.path);
    }
    this.changes.fire([
      { type: FileChangeType.DELETED, resource: from },
      { type: FileChangeType.ADDED, resource: to },
    ]);
  }
}
