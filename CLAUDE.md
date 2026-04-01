# CC Session

Desktop app for browsing AI coding sessions. Tauri 2.0 + Solid.js + Rust + SQLite FTS5.

## Commands

```bash
npm run tauri dev             # Dev with hot reload
npm run tauri build           # Production build
npx tauri build --bundles dmg # DMG only
cd src-tauri && cargo clippy  # Rust lint
cd src-tauri && cargo test    # Rust tests (21 parser golden tests)
npx tsc --noEmit              # TS type check
npm run lint                  # ESLint
npm run format:check          # Prettier check
./scripts/release.sh 0.2.0   # Bump, commit, tag, push → triggers CI release
```

## Project Layout

```
src/                       # Solid.js frontend
  App/                     # Root component (index.tsx, KeyboardShortcuts.ts, SyncManager.ts)
  components/
    MessageBubble/         # Message rendering (index, MarkdownRenderer, ToolMessage, etc.)
    SessionView/           # Session detail (index, SessionToolbar, SessionSearch, hooks)
    Explorer/              # Tree navigation (index, hooks)
    ...                    # Flat components: TreeNode, TabBar, CodeBlock, etc.
  stores/                  # Reactive state (toast, search, theme, settings, favorites)
  i18n/                    # en.json, zh.json
  lib/                     # types.ts, tauri.ts, provider-registry.ts, icons.tsx, formatters.ts,
                           # platform.ts, tree-utils.ts, tree-builders.ts
  styles/                  # variables.css, layout.css, messages.css, components.css
src-tauri/src/
  providers/
    claude/                # mod.rs, parser.rs, images.rs
    codex/                 # mod.rs, parser.rs, tools.rs
    gemini/                # mod.rs, logs_parser.rs, chat_parser.rs, orphan.rs, images.rs, tools.rs
    kimi/                  # mod.rs, parser.rs, tools.rs
    cursor/                # mod.rs, parser.rs, tools.rs
    opencode/              # mod.rs, parser.rs
    cc_mirror.rs           # CC-Mirror provider (multi-variant Claude Code aggregator)
  commands/                # sessions.rs, settings.rs, trash.rs, terminal.rs
  exporter/                # json.rs, markdown.rs, html.rs, templates.rs
  db/                      # mod.rs (schema, read/write conn separation), queries.rs, sync.rs, row_mapper.rs
  indexer.rs               # Parallel scan, batch upsert, tree building
  watcher.rs               # notify crate, FS events for JSONL/JSON providers
  models.rs                # Provider, SessionMeta, Message, TokenUsage, TreeNode
  provider.rs              # SessionProvider trait, make_provider()
  provider_utils.rs        # Shared helpers, FTS_CONTENT_LIMIT constant
```

## Provider Architecture

All providers implement `SessionProvider` trait:
- `scan_all()` → `Vec<ParsedSession>` (meta + content for FTS)
- `load_messages(session_id, source_path)` → `Vec<Message>` (on-demand)
- `watch_paths()` → directories for file system watcher

Provider constructors return `Option<Self>` (graceful skip if HOME unavailable).
`Provider::parse(s)` maps string keys to enum variants (renamed from `from_str`).

File-based (Claude, Codex, Gemini, Kimi CLI, CC-Mirror): FS event watching via `notify` crate.
SQLite-based (Cursor CLI, OpenCode): 2s polling in frontend when live watch enabled. Opened with `SQLITE_OPEN_READ_WRITE` to read WAL data.

CC-Mirror is a special provider that aggregates multiple Claude Code-like variants under `~/.cc-mirror/`. Each variant subdirectory contains a `variant.json` metadata file and `config/projects/` directory with JSONL sessions (same format as Claude Code). Variant names are sanitized (alphanumeric, hyphens, underscores) for safe shell usage. Sessions are grouped by variant in the UI. Resume commands use variant names as command prefixes (e.g., `mycczai --resume session-id`).

Heavy commands (`reindex`, `sync_sources`, `delete_sessions_batch`, `export_sessions_batch`) are `async` with `tokio::task::spawn_blocking` to avoid blocking the main thread.

Tool names are mapped to canonical names per provider (e.g. Codex `exec_command` → `Bash`, Cursor CLI `StrReplace` → `Edit`, Kimi CLI `WriteFile` → `Write`) so the frontend has one consistent display path.

## Data Sources

| Provider    | Path                                   | Format |
|-------------|----------------------------------------|--------|
| Claude Code | `~/.claude/projects/**/*.jsonl`        | JSONL  |
| Codex       | `~/.codex/sessions/**/*.jsonl`         | JSONL  |
| Gemini      | `~/.gemini/tmp/*/chats/*.json`         | JSON   |
| Kimi CLI    | `~/.kimi/sessions/**/*.jsonl`          | JSONL  |
| Cursor CLI  | `~/.cursor/chats/**/store.db`          | SQLite |
| OpenCode    | `~/.local/share/opencode/opencode.db`  | SQLite |
| CC-Mirror   | `~/.cc-mirror/{variant}/config/projects/**/*.jsonl` | JSONL |

## Database

`Database` uses separate read/write connections (`read_conn` + `write_conn`, both `Mutex<Connection>`).
Read queries go through `lock_read()`, writes through `lock_write()` / `with_transaction()`.
WAL mode enables concurrent reads during writes.

## Testing

- **Rust**: 21 golden tests in `src-tauri/tests/parser_tests.rs` covering Claude, Codex, Kimi parsers
  - Test fixtures in `src-tauri/tests/fixtures/`
  - Run: `cd src-tauri && cargo test`
- **Frontend**: vitest (`src/stores/search.test.ts`)
  - Run: `npm test`

## CI

Three-platform matrix (macOS, Windows, Linux) with:
- Frontend: tsc, eslint, prettier, vitest, vite build
- Rust: fmt, check, clippy (with `Swatinem/rust-cache`)
- Linux: installs webkit2gtk/gtk system deps

## Key Patterns

- **Message struct:** `{ role, content, timestamp, tool_name, tool_input, token_usage }` — universal across all providers
- **Thinking:** Emitted as `MessageRole::System` with `[thinking]\n` prefix. Frontend renders as collapsible block. HTML export renders as `<details>`.
- **Images:** `[Image: source: /path]` or `[Image: source: data:...]` in content. Frontend detects and renders as `<img>`.
- **Token usage:** Attached to last assistant/tool message of each turn. Frontend aggregates session totals.
- **Tool call + result merge:** Providers use `call_id` maps to merge tool outputs into tool call messages (single Message with name + input + output).

## Lessons Learned (Pitfalls to Avoid)

### SQLite Providers (Cursor CLI, OpenCode)

- **SQLITE_OPEN_READ_ONLY cannot read WAL data.** Cursor and OpenCode use WAL mode. During active sessions, data lives in the WAL file. `READ_ONLY` connections only see checkpointed data. Must use `SQLITE_OPEN_READ_WRITE` even though we only read.
- **Cursor CLI store.db uses BLOB column type**, not TEXT. `row.get::<_, String>(0)` silently fails. Must use `row.get::<_, Vec<u8>>(0)` then `String::from_utf8_lossy`.
- **Cursor CLI content is JSON-array-as-string.** The `content` field in blobs is stored as a JSON string `"[{\"type\":\"text\",...}]"`, not a native JSON array. Need double-parse: first serde gives `Value::String`, then parse the string as `Vec<Value>`.
- **FSEvents unreliable for SQLite WAL changes.** macOS file system events don't reliably fire for WAL writes. Solution: use 2-second polling in frontend for DB-based providers, keep FS events only for JSONL/JSON providers.
- **OpenCode uses XDG path, not macOS standard.** `dirs::data_local_dir()` returns `~/Library/Application Support` on macOS, but OpenCode stores data in `~/.local/share/opencode/`. Must manually construct `$HOME/.local/share/opencode`.
- **OpenCode "global" project has worktree="/".** Prefer session's `directory` field over project's `worktree` for path resolution.

### Codex

- **`agent_message` events duplicate `response_item/message/assistant`.** Same content, 1:1 correspondence. Only parse `response_item`, skip `agent_message`.
- **`function_call` and `function_call_output` are paired by `call_id`**, not sequential. Codex emits multiple calls in batch, then results in batch. Must use a `call_id → index` map to merge results into the right call message.
- **`function_call_output.output` can be nested JSON.** MCP tool results come as `[{"type":"text","text":"..."}]` array. Custom tool results as `{"output":"...","metadata":{...}}`. Need `extract_tool_output()` to handle both.
- **`exec_command` args use `cmd` not `command`.** Must remap to `{"command": cmd}` for frontend Bash display.
- **Empty `write_stdin` calls are polling noise.** Filter out `write_stdin` where `chars` is empty.
- **`apply_patch` input is raw patch text, not JSON.** Frontend `formatToolInput` can't JSON.parse it. Handle in catch branch.

### Gemini

- **Image deduplication.** When `inlineData` exists in content array, `@path/to/image.png` text refs are duplicates of the same image. Skip `@` refs when `inlineData` is present.
- **Context markers.** Filter `--- Content from referenced files ---` and `--- End of content ---` text parts.
- **`displayName` vs `name` in toolCalls.** Use `displayName` for human-readable names (Shell, WriteFile, Edit), fall back to internal `name` (run_shell_command, write_file). Map to canonical names.

### Kimi CLI

- **Session path uses MD5 of project path.** `~/.kimi/sessions/<md5(project_path)>/<session_uuid>/wire.jsonl`. Read `~/.kimi/kimi.json` to build the MD5 → project path mapping.
- **wire.jsonl is an event stream**, not message-per-line. Key types: `TurnBegin` (user input), `ContentPart` (text/think), `ToolCall`/`ToolResult` (paired by `id`), `StatusUpdate` (token usage).
- **Image deduplication.** `TurnBegin.user_input` has both `<image path="...">` text marker and `image_url` with base64 data. Skip text markers (`<image path=...>` and `</image>`) when `image_url` is present.
- **Token usage in StatusUpdate.** Format: `{ input_other, output, input_cache_read, input_cache_creation }`. Map `input_tokens = input_other + input_cache_read + input_cache_creation`.
- **`/` slash commands not in wire.jsonl.** They're handled client-side, only recorded in `user-history/*.jsonl` (no AI response).
- **Timestamps are float seconds** (not milliseconds). Convert with `ts as i64` for epoch seconds.

### HTML Export

- **Never use `&s[..N]` for truncation.** Multi-byte UTF-8 characters (Chinese, emoji) can be split, causing panic. Always use `truncate_char_boundary()` or don't truncate at all.
- **No content truncation in export.** Previously truncated tool input/output to 500 chars. Removed — export should be complete.

### Indexer

- **`build_tree` must use `Provider::from_str()`.** Previously hardcoded `match` for claude/codex/gemini — adding cursor/opencode silently failed. Using `from_str()` ensures new providers are automatically included.

### CC Mirror

- **Multiple variants under one `~/.cc-mirror/` root.** Each subdirectory is a variant (e.g., `cczai`, `qwen`). Each must have `variant.json` at the root.
- **Variant names are sanitized.** Only alphanumeric, hyphens, underscores allowed. Extracted from the directory name for safe shell usage.
- **Session grouping by variant.** Tree structure: `Provider (CC-Mirror) → Project (variant name) → Sessions`. Differs from single-instance providers.
- **Projects dir path.** Each variant's sessions are at `{variant-dir}/config/projects/`. Fixed path structure.
- **Resume command uses variant name.** `{variant-name} --resume {session-id}`. Variant name stored in `SessionMeta.variant_name`. Fallback: `claude --resume {session-id}`.
- **Terminal validation for variants.** `open_in_terminal` validates that a variant name matches a known variant directory (directory exists with valid `variant.json`) to prevent shell injection.

### General

- **Provider isolation.** Changes to one provider's parser must not affect others. The boundary is the canonical tool name mapping — each provider maps its tool names to {Bash, Edit, Read, Write, Glob, Grep, Agent, Plan} and the frontend only handles these.
- **Resume commands vary per provider.** Claude: `claude --resume ID`, Codex: `codex resume ID`, Gemini: `gemini --resume ID`, Kimi CLI: `kimi --session ID`, Cursor CLI: `agent --resume=ID`, OpenCode: `opencode -s ID`, CC-Mirror: `{variant-name} --resume ID`.
- **FTS content is intentionally truncated** to 2000 bytes via `truncate_to_bytes`. This is for index size, not display. Display content is never truncated.
- **Timestamps.** Claude/Codex/Gemini have per-message timestamps. Cursor CLI has none (use file metadata). OpenCode uses epoch milliseconds (convert with `ms / 1000`).
- **Cursor CLI has no token usage data.** store.db blobs only contain `role`, `content`, `id` — no usage/token fields. Token billing is tracked server-side only.

## Conventions

- Rust: `cargo fmt` + `cargo clippy` before commit
- TypeScript: `npx eslint src/` + `npx prettier --check "src/**/*.{ts,tsx,css}"` before commit
- TypeScript: strict mode, no `any`
- Commits: conventional commits (`feat:`, `fix:`, `refactor:`)
- i18n: all user-facing strings via `t()`, never hardcoded
- CSS: variables in `variables.css`. Provider colors: Claude `#8b5cf6`, Codex `#10b981`, Gemini `#f59e0b`, Kimi CLI `#6366f1`, Cursor CLI `#3b82f6`, OpenCode `#06b6d4`, CC-Mirror `#f472b6`
