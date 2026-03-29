# CC Session

Desktop app for browsing AI coding sessions. Tauri 2.0 + Solid.js + Rust + SQLite FTS5.

## Commands

```bash
npm run tauri dev             # Dev with hot reload
npm run tauri build           # Production build
npx tauri build --bundles dmg # DMG only
cd src-tauri && cargo clippy  # Rust lint
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
  lib/                     # types.ts, tauri.ts, providers.ts, icons.tsx, formatters.ts,
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
  commands/                # sessions.rs, settings.rs, trash.rs, terminal.rs
  exporter/                # json.rs, markdown.rs, html.rs, templates.rs
  db/                      # mod.rs (schema), queries.rs, sync.rs, row_mapper.rs
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

File-based (Claude, Codex, Gemini, Kimi CLI): FS event watching via `notify` crate.
SQLite-based (Cursor CLI, OpenCode): 2s polling in frontend when live watch enabled. Opened with `SQLITE_OPEN_READ_WRITE` to read WAL data.

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

### HTML Export

- **Never use `&s[..N]` for truncation.** Multi-byte UTF-8 characters (Chinese, emoji) can be split, causing panic. Always use `truncate_char_boundary()` or don't truncate at all.
- **No content truncation in export.** Previously truncated tool input/output to 500 chars. Removed — export should be complete.

### Indexer

- **`build_tree` must use `Provider::from_str()`.** Previously hardcoded `match` for claude/codex/gemini — adding cursor/opencode silently failed. Using `from_str()` ensures new providers are automatically included.

### General

- **Provider isolation.** Changes to one provider's parser must not affect others. The boundary is the canonical tool name mapping — each provider maps its tool names to {Bash, Edit, Read, Write, Glob, Grep, Agent, Plan} and the frontend only handles these.
- **Resume commands vary per provider.** Claude: `claude --resume ID`, Codex: `codex resume ID`, Gemini: `gemini --resume ID`, Kimi CLI: `kimi --session ID`, Cursor CLI: `agent --resume=ID`, OpenCode: `opencode -s ID`.
- **FTS content is intentionally truncated** to 2000 bytes via `truncate_to_bytes`. This is for index size, not display. Display content is never truncated.
- **Timestamps.** Claude/Codex/Gemini have per-message timestamps. Cursor CLI has none (use file metadata). OpenCode uses epoch milliseconds (convert with `ms / 1000`).
- **Cursor CLI has no token usage data.** store.db blobs only contain `role`, `content`, `id` — no usage/token fields. Token billing is tracked server-side only.

## Conventions

- Rust: `cargo fmt` + `cargo clippy` before commit
- TypeScript: `npx eslint src/` + `npx prettier --check "src/**/*.{ts,tsx,css}"` before commit
- TypeScript: strict mode, no `any`
- Commits: conventional commits (`feat:`, `fix:`, `refactor:`)
- i18n: all user-facing strings via `t()`, never hardcoded
- CSS: variables in `variables.css`. Provider colors: Claude `#8b5cf6`, Codex `#10b981`, Gemini `#f59e0b`, Kimi CLI `#6366f1`, Cursor CLI `#3b82f6`, OpenCode `#06b6d4`
