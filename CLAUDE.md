# CC Session

Desktop app for browsing AI coding sessions. Tauri 2.0 + Solid.js + Rust + SQLite FTS5.

## Commands

```bash
npm run tauri dev             # Dev with hot reload
npm run tauri build           # Production build
npx tauri build --bundles dmg # DMG only
cd src-tauri && cargo clippy  # Rust lint
cd src-tauri && cargo test    # Rust tests (parser + trash lifecycle)
npx tsc --noEmit              # TS type check
npm run lint                  # ESLint
npm run format:check          # Prettier check
./scripts/release.sh 0.2.0   # Bump, commit, tag, push → triggers CI release
```

## Project Layout

```
src/                       # Solid.js frontend
  App/                     # Root component, KeyboardShortcuts, SyncManager
  components/              # MessageBubble/, SessionView/, Explorer/, TreeNode, TabBar, etc.
  stores/                  # Reactive state (toast, search, theme, settings, favorites)
  i18n/                    # en.json, zh.json
  lib/                     # types, tauri, provider-registry, icons, formatters, tree-utils
  styles/                  # variables.css, layout.css, messages.css, components.css
src-tauri/src/
  providers/               # claude/, codex/, gemini/, kimi/, cursor/, opencode/, cc_mirror.rs
  commands/                # sessions.rs, settings.rs, trash.rs, terminal.rs
  exporter/                # json.rs, markdown.rs, html.rs, templates.rs
  db/                      # mod.rs, queries.rs, sync.rs, row_mapper.rs
  indexer.rs               # Parallel scan, batch upsert, tree building
  watcher.rs               # notify crate, FS events
  models.rs                # Provider, SessionMeta, Message, TokenUsage, TreeNode, TrashMeta
  provider.rs              # SessionProvider trait, DeletionPlan, execute_trash/restore/purge
  provider_utils.rs        # Shared helpers
  trash_state.rs           # Trash metadata I/O, shared_deletions tracking
```

## Provider Architecture

All providers implement `SessionProvider` trait:
- `scan_all()` / `load_messages()` / `watch_paths()` — indexing & display
- `deletion_plan()` / `restore_action()` / `cleanup_on_permanent_delete()` — trash lifecycle
- `purge_from_source()` — remove from shared DB/file (OpenCode)

Provider metadata uses the Bridge pattern: `Provider` enum → `ProviderDescriptor` trait
(zero-sized structs) for static metadata; `SessionProvider` via `make_provider()` for instance ops.

File-based (Claude, Codex, Gemini, Kimi, Cursor, CC-Mirror, Qwen): FS event watching via `notify` crate.
SQLite-based (OpenCode): 2s polling in frontend. Opened with `SQLITE_OPEN_READ_WRITE` to read WAL data.

Tool names mapped to canonical names per provider (e.g. Codex `exec_command` → `Bash`, Cursor `StrReplace` → `Edit`, Kimi `WriteFile` → `Write`, Qwen `run_shell_command` → `Bash`).

## Data Sources

| Provider    | Path                                   | Format |
|-------------|----------------------------------------|--------|
| Claude Code | `~/.claude/projects/**/*.jsonl`        | JSONL  |
| Codex       | `~/.codex/sessions/**/*.jsonl`         | JSONL  |
| Gemini      | `~/.gemini/tmp/*/chats/*.json`         | JSON   |
| Kimi CLI    | `~/.kimi/sessions/**/wire.jsonl`       | JSONL  |
| Cursor CLI  | `~/.cursor/projects/*/agent-transcripts/**/*.jsonl` | JSONL |
| OpenCode    | `~/.local/share/opencode/opencode.db`  | SQLite |
| CC-Mirror   | `~/.cc-mirror/{variant}/config/projects/**/*.jsonl` | JSONL |
| Qwen Code   | `~/.qwen/projects/*/chats/*.jsonl`     | JSONL  |

## Testing

- **Rust**: `cd src-tauri && cargo test`
  - 58 parser golden tests in `tests/parser_tests.rs`
  - Trash lifecycle test in `tests/trash_lifecycle_test.rs` (real local data, `--ignored`)
  - Provider unit tests in `src/provider.rs`
- **Frontend**: `npm test` (vitest)

## Key Patterns

- **Message struct:** `{ role, content, timestamp, tool_name, tool_input, token_usage }` — universal across all providers
- **Thinking:** `MessageRole::System` with `[thinking]\n` prefix. Collapsible in frontend.
- **Images:** `[Image: source: /path]` or `[Image: source: data:...]` in content.
- **Tool call + result merge:** Providers use `call_id` maps to merge tool outputs into tool call messages.
- **Subagents:** Parent-child sessions linked via `parent_id`. Frontend "Open" button navigates by `agentId` (Kimi), `description` match (Cursor/Codex), or `nickname` (Codex).
- **Trash lifecycle:** `TrashMeta.parent_id` links children to parents. Restore recovers all children. Permanent delete uses `cleanup_session_dir()` with `is_session_dir()` safety check + `cleanup_on_permanent_delete()` hook.

## Lessons Learned (Pitfalls to Avoid)

### OpenCode (SQLite)

- **Must use `SQLITE_OPEN_READ_WRITE`** even for read-only access — `READ_ONLY` can't see WAL data.
- **Uses XDG path `~/.local/share/opencode/`**, not macOS `~/Library/Application Support/`.
- **"Global" project has worktree="/".** Use session's `directory` field instead.

### Codex

- **`function_call` and `function_call_output` paired by `call_id`**, not sequential. Must use `call_id → index` map.
- **`function_call_output.output` can be nested JSON.** MCP: `[{"type":"text","text":"..."}]`. Custom: `{"output":"..."}`.

### Kimi CLI

- **Session path uses MD5 of project path.** Read `~/.kimi/kimi.json` for MD5 → path mapping.
- **wire.jsonl is an event stream.** Key types: `TurnBegin`, `ContentPart`, `ToolCall`/`ToolResult`, `SubagentEvent`, `StatusUpdate`.
- **Subagent data embedded in parent wire.jsonl** as `SubagentEvent` entries. Titles from `subagents/<id>/meta.json`.
- **Parallel Agent ToolCall args truncated.** Only first gets complete JSON. Frontend regex fallback.
- **Timestamps are float seconds** (not milliseconds).

### Cursor CLI

- **JSONL transcripts** at `~/.cursor/projects/<key>/agent-transcripts/<id>/<id>.jsonl`. Subagents under `<id>/subagents/`.
- **store.db is a CLI session marker**, not content. At `~/.cursor/chats/<hash>/<id>/store.db`. Don't delete during trash, only permanent delete.
- **`[REDACTED]`** = redacted thinking. Strip from visible text.
- **Subagent matching uses full prompt text** to avoid prefix collisions.

### CC-Mirror

- **Multi-variant aggregator** under `~/.cc-mirror/`. Each variant: `variant.json` + `config/projects/` with JSONL.
- **Variant names sanitized** (alphanumeric, hyphens, underscores) for safe shell usage.
- **Resume command:** `{variant-name} --resume {session-id}`.

### Qwen Code

- **Path uses `sanitizeCwd()`** — project dir is `~/.qwen/projects/{cwd-with-hyphens}/chats/`, not SHA256.
- **`thought` field is boolean `true`**, not a string. Thinking text is in the `text` field of the same part.
- **`functionCall.id` matches `toolCallResult.callId`** for tool result merging.
- **Subagents embedded in parent session** as `agent` tool calls — no separate files.
- **System records** (`ui_telemetry`, `slash_command`, `at_command`, `chat_compression`) are skipped during parsing.
- **Legacy tool names:** `search_file_content` → `grep_search`, `replace` → `edit`, `task` → `agent`.

### Trash / Restore

- **`TrashMeta.parent_id`** links children to parents for reliable restore across all filesystem layouts.
- **`is_session_dir()` safety check** prevents `remove_dir_all` on shared directories (e.g. Gemini `chats/`).
- **Kimi `subagents/` preserved during trash** for title restoration, deleted on permanent delete.

### General

- **Provider isolation.** Each provider maps tool names to canonical set {Bash, Edit, Read, Write, Glob, Grep, Agent, Plan}.
- **Resume commands:** Claude `--resume ID`, Codex `resume ID`, Gemini `--resume ID`, Kimi `--session ID`, Cursor `--resume=ID`, OpenCode `-s ID`, Qwen `--resume ID`.
- **FTS content truncated to 2000 bytes** for index size. Display content never truncated.

## Conventions

- Rust: `cargo fmt` + `cargo clippy` before commit
- TypeScript: `npx eslint src/` + `npx prettier --check "src/**/*.{ts,tsx,css}"`
- TypeScript: strict mode, no `any`
- Commits: conventional commits (`feat:`, `fix:`, `refactor:`)
- i18n: all user-facing strings via `t()`, never hardcoded
- Provider colors: Claude `#d97757`, Codex `#10b981`, Gemini `#f59e0b`, Cursor `#3b82f6`, OpenCode `#06b6d4`, Kimi `#1783ff`, CC-Mirror `#f472b6`, Qwen `#6c3cf5`
