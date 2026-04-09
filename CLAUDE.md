# CC Session

Desktop app for browsing AI coding sessions. Tauri 2.0 + Solid.js + Rust + SQLite FTS5.

## Commands

```bash
npm run tauri dev             # Dev with hot reload
npm run tauri build           # Production build
cd src-tauri && cargo clippy  # Rust lint
cd src-tauri && cargo test    # Rust tests
npx tsc --noEmit              # TS type check
npm run lint                  # ESLint
npm run format:check          # Prettier check
./scripts/release.sh <version> # Bump, commit, tag, push → triggers CI release
```

## Project Layout

```
src/                       # Solid.js frontend (components, stores, i18n, lib, styles)
src-tauri/src/
  providers/               # claude/, codex/, gemini/, kimi/, cursor/, opencode/, qwen/, cc_mirror.rs
  commands/                # sessions.rs, settings.rs, trash.rs, terminal.rs
  services/                # provider_snapshots.rs, session_lifecycle.rs, session_resolution.rs, source_sync.rs
  exporter/                # json.rs, markdown.rs, html.rs, templates.rs
  db/                      # mod.rs, queries.rs, sync.rs, row_mapper.rs
  indexer.rs  watcher.rs  models.rs  provider.rs  provider_utils.rs  trash_state.rs
src/stores/               # settings, search, selection, providerSnapshots, updater, favorites
src/lib/                  # tauri.ts, provider-watch.ts, formatters, tree-builders, icons
```

## Provider Architecture

All providers implement `SessionProvider` trait (`watch_paths` / `scan_all` / `scan_source` / `load_messages` / `deletion_plan` / `restore_action` / `cleanup_on_permanent_delete`).
Metadata via Bridge pattern: `Provider` enum → `ProviderDescriptor` (zero-sized structs).

| Provider    | Path                                   | Format | Watch |
|-------------|----------------------------------------|--------|-------|
| Claude Code | `~/.claude/projects/**/*.jsonl`        | JSONL  | FS    |
| Codex       | `~/.codex/sessions/**/*.jsonl`         | JSONL  | FS    |
| Gemini      | `~/.gemini/tmp/*/chats/*.json`         | JSON   | Poll  |
| Kimi CLI    | `~/.kimi/sessions/**/wire.jsonl`       | JSONL  | FS    |
| Cursor CLI  | `~/.cursor/projects/*/agent-transcripts/**/*.jsonl` | JSONL | FS |
| OpenCode    | `~/.local/share/opencode/opencode.db`  | SQLite | Poll  |
| Qwen Code   | `~/.qwen/projects/*/chats/*.jsonl`     | JSONL  | FS    |
| Copilot     | `~/.copilot/session-state/*/events.jsonl` | JSONL | FS   |
| CC-Mirror   | `~/.cc-mirror/{variant}/config/projects/**/*.jsonl` | JSONL | FS |

Tool names mapped to canonical set per provider: {Bash, Edit, Read, Write, Glob, Grep, Agent, Plan}.
Resume: Claude `--resume`, Codex `resume`, Gemini `--resume`, Kimi `--session`, Cursor `--resume=`, OpenCode `-s`, Qwen `--resume`, Copilot `--resume=`.

## Testing

- **Rust**: `cd src-tauri && cargo test` — parser golden tests + provider/unit tests + fixture command interface coverage
- **Frontend**: `npm test` (vitest)
- **Manual smoke**: `provider_lifecycle_real_interface.rs` is ignored by default and intended for local/manual real-provider verification

## Key Patterns

- **Message**: `{ role, content, timestamp, tool_name, tool_input, token_usage }` — universal
- **Thinking**: `MessageRole::System` with `[thinking]\n` prefix
- **Images**: `[Image: source: ...]` in content
- **Tool merge**: `call_id` maps pair tool calls with results
- **Subagents**: `parent_id` links children; "Open" button for providers with separate files (Claude, Codex, Kimi, Cursor, CC-Mirror)
- **Provider snapshots**: backend derives provider label/color/order/watch strategy/path info; frontend consumes snapshot data
- **Trash**: `TrashMeta.parent_id` cascades restore/delete; `is_session_dir()` prevents shared dir deletion

## Pitfalls

- **OpenCode**: Must use `SQLITE_OPEN_READ_WRITE` (not READ_ONLY) for WAL. Uses XDG path, not macOS `~/Library/`.
- **macOS watchers**: File-backed providers use `notify` with `macos_kqueue` for more reliable file-level follow behavior; do not assume `FSEvents`.
- **Codex**: `call_id` pairing, output can be nested JSON.
- **Kimi**: MD5 project path, event stream format, float-second timestamps, truncated parallel agent args.
- **Cursor**: JSONL transcripts + store.db marker. `[REDACTED]` = redacted thinking. Full-text subagent matching.
- **CC-Mirror**: Multi-variant under `~/.cc-mirror/`, sanitized variant names.
- **Qwen**: `sanitizeCwd()` path (hyphens, not SHA256). `thought: true` boolean + `text` field. Subagents embedded in parent (no separate files). Skip `ui_telemetry`/`slash_command`/`at_command`/`chat_compression`.
- **Copilot**: Session ID from directory name (UUID). `reasoningText` for thinking. `toolRequests[]` in `assistant.message` + `tool.execution_start/complete` for tool calls. Subagents embedded via `task` tool (no separate files). Skip `hook.*`/`session.info`/`system.notification`/`session.mode_changed`.

## Conventions

- Rust: `cargo fmt` + `cargo clippy` before commit
- TypeScript: strict mode, no `any`, ESLint + Prettier
- Commits: conventional commits (`feat:`, `fix:`, `refactor:`)
- i18n: all user-facing strings via `t()`
- Colors: Claude `#d97757`, Codex `#10b981`, Gemini `#f59e0b`, Cursor `#3b82f6`, OpenCode `#06b6d4`, Kimi `#1783ff`, CC-Mirror `#f472b6`, Qwen `#6c3cf5`, Copilot `#171717`
