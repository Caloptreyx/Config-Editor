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

## Permissions

The extension uses the panel's file permissions; it adds none of its own.

| Action | Permission |
|---|---|
| Open the page, browse folders, manage favorites | `files.read` |
| Open a file | `files.read-content` |
| Save, review changes, raw saves | `files.create` (the panel's file write permission) |

Without `files.create` the editor is read-only.

## Installation

Download `dev_caloptreyx_configeditor.c7s.zip` from the latest release and either upload it under
**Admin → Extensions** or put it in your heavy image's `build/extensions/` directory and run
`docker compose restart web`. Extensions need the `:heavy` panel image or a development environment.
The extension adds one table (`dev_caloptreyx_configeditor_favorites`) through its migration.

Users find **Config Editor** in the server sidebar.

To build the zip from a checkout, run `python3 scripts/package.py` (writes `dist/`).

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
`tests/` and `cargo test` against the newest panel release.

## License

MIT
