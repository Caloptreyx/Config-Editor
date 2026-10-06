# Config Editor

A [Calagopus Panel](https://calagopus.com) extension that turns a server's configuration files into
forms. Open a file from the server's **Config Editor** page, change values in fields built for their
type, and save. Only the values you changed are rewritten on disk; comments, blank lines, quoting and
indentation everywhere else stay exactly as they were.

Package name: `dev.caloptreyx.configeditor` · Requires panel `>=1.2.3`

## Features

- **Six formats**: YAML, JSON (including JSONC-style `//` and `/* */` comments and trailing commas),
  TOML, Java `.properties` (`server.properties`, `eula.txt`), `.env` files and INI (`.ini`, `.cfg`).
- **Fields that fit the value**: switches for booleans, validated number fields that keep large
  integers such as seeds and Discord IDs exact, multi-line text areas, null values you can fill in,
  and TOML dates. Objects become collapsible groups, lists get add, remove and reorder controls, and
  keys can be added or removed where the format allows it.
- **Minimal-diff saving**: the backend re-reads the file and edits only the text of changed entries.
  Untouched parts keep their exact bytes, including comments, quote style and CRLF line endings.
- **Comments as help text**: the comment written above a key appears as the description of its field.
- **Config-only file browser**: folders plus the files the editor understands, nothing else.
- **Favorites**: star the files you edit often; each user has their own list per server.
- **Tabs**: keep several files open, see which ones have unsaved changes, save with Ctrl/Cmd+S. Closing
  a changed tab or leaving the page asks first.
- **Safe writes**: review a diff of the exact file contents before saving. If the file changed on disk
  since you opened it, you choose between reloading and overwriting.
- **Raw mode**: a code editor for the file's text, used automatically when a file does not parse, with
  the error's line and column.
- **Native integration**: parsing runs in the panel backend (Rust). Writes go through Wings like the
  panel's own file editor, so they create file revisions, appear in the activity log as file writes and
  respect subuser ignored files. Built from the panel's own components.
- **VS Code**: **Open in VS Code** on the Config Editor page opens the full VS Code workbench for all of
  the server's files: Explorer, tabs, Quick Open (Ctrl/Cmd+P), Search (Ctrl/Cmd+Shift+F), command
  palette, breadcrumbs, minimap and outline. See [VS Code](#vs-code).

## How saving works

Saving sends the whole edited document. The backend parses the current file, compares it with the
document and turns the difference into text edits:

- A value equal to the file's current value is not touched.
- A changed value is replaced in place; the key, separator, quote style and any comment on the same
  line stay.
- A removed key loses its own lines and the comment block directly above it. List items lose their
  own lines.
- A new key or item is added at the end of its object or list, indented like its neighbours.
- When an in-place edit is not possible (for example a value that changes from text to a list, or a
  one-line `[a, b]` list that gains items), only the smallest enclosing value is written again.

The result is parsed again before it is written, and the save is refused if it does not match the
document you submitted.

| Format | Files | Value types | Notes |
|---|---|---|---|
| YAML | `*.yml`, `*.yaml` | all except dates | YAML 1.2 core schema. Anchors, aliases, custom tags and multi-document files open in raw mode. New strings that YAML 1.1 (SnakeYAML) would read as something else, such as `yes` or `on`, are quoted. |
| JSON | `*.json`, `*.jsonc` | all except dates | Numbers keep their exact text. NaN and infinity are refused. |
| TOML | `*.toml` | all except null | Key/value lines of a table are kept before its sub-tables; integers must fit in 64 bits. |
| Properties | `*.properties`, `eula.txt` | text, numbers, booleans | Java escapes and line continuations are understood. |
| .env | `.env`, `.env.*`, `*.env` | text, numbers, booleans | `export` prefixes, quote styles and inline comments are kept. |
| INI | `*.ini`, `*.cfg` | text, numbers, booleans | Global keys come before the first `[section]`. |

Properties, .env and INI files store text, so the editor reads `true`/`false` as booleans and plain
decimal numbers as numbers; anything else is text. Duplicate keys in any format open the file in raw
mode, so no value is silently dropped.

## VS Code

The VS Code view is the real VS Code workbench running in the browser
([monaco-vscode-api](https://github.com/CodinGame/monaco-vscode-api), VS Code 1.138). It loads in a frame
from `/config-editor-vscode/` on the panel's own origin and talks to the panel's file API directly, so:

- Reads, saves, new files and folders, renames and deletes are the panel's own file operations. They go
  through Wings with the user's permissions and subuser ignored files, and saves create file revisions
  and activity log entries like the file manager's.
- Quick Open and Search use the panel's file search (run by Wings), so nothing walks the server's files
  from the browser. Wings matches plain text: VS Code checks regular expressions and whole-word matches
  in the browser against what Wings found for the longest literal part of the pattern. A pattern with
  no literal part (such as `\d+` or `a|b`) cannot be searched. Results are limited by the panel's
  **Max File Manager Search Results** and **Max File Manager Content Search Size** settings, and at
  most 100 matches per file are listed.
- Files larger than **Max File Manager View Size** cannot be opened or saved.
- Extensions are limited to the bundled ones: language grammars (YAML, JSON, INI and properties, XML,
  Lua, JavaScript/TypeScript, Python, Java, shell, PowerShell, batch, SQL, CSS, HTML, Markdown,
  Dockerfile, .env, logs), the default themes and Seti file icons. There is no marketplace: the
  workbench runs with the user's panel session, so a third-party extension could act as the user.
- There is no terminal, debugger or Git: no process runs next to the server's files.
- The theme follows the panel's light or dark scheme. VS Code settings and layout are kept in the
  browser's storage.

The workbench adds about 16 MB of static files to the panel build; browsers only load them when the
view is opened.

## Permissions

The extension uses the panel's file permissions; it adds none of its own.

| Action | Permission |
|---|---|
| Open the page, browse folders, manage favorites | `files.read` |
| Open a file | `files.read-content` |
| Save, review changes, raw saves, create files and folders in VS Code | `files.create` (the panel's file write permission) |
| Rename or move in VS Code | `files.update` |
| Delete in VS Code | `files.delete` |

Without `files.create` both editors are read-only.

## Installation

Download `dev_caloptreyx_configeditor.c7s.zip` from the latest release and either upload it under
**Admin → Extensions** or put it in your heavy image's `build/extensions/` directory and run
`docker compose restart web`. Extensions need the `:heavy` panel image or a development environment.
The extension adds one table (`dev_caloptreyx_configeditor_favorites`) through its migration.

Users find **Config Editor** in the server sidebar; **Open in VS Code** is at its top right.

To build the zip from a checkout, run `python3 scripts/package.py` (writes `dist/`). It builds the VS
Code workbench first (`npm ci && npm run build` in `workbench/`, Node 20 or newer).

## API

All routes live under `/api/client/servers/{server}/config-editor`:

| Route | Purpose |
|---|---|
| `GET /files?directory=` | Folders and supported files of a directory |
| `GET /file?path=` | Content, hash, parsed document (or the parse error) |
| `PUT /file` | Save a document (`expected_hash`, `dry_run`, `force`) |
| `PUT /file/raw` | Save raw text |
| `GET`, `POST`, `DELETE /favorites` | The user's favorites on the server |

## Development

The crate lives at the repository root, the frontend in `frontend/`. Every push runs the shared
extension check (`.github/workflows/check.yml`): typecheck, Biome, frontend build, the node tests in
`tests/` and `cargo test` against the newest panel release, plus a typecheck and build of the
workbench.

The VS Code workbench is a separate Vite app in `workbench/` because it bundles its own Monaco build,
which cannot share a page with the panel's `monaco-editor`. `npm run build` there writes
`frontend/public/config-editor-vscode/` (git-ignored), which the panel copies into its own build. Its
`@codingame/monaco-vscode-*` packages must all have the same version.

## License

MIT
