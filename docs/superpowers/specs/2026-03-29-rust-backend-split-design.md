# Rust Backend Module Split Design

> Date: 2026-03-29
> Scope: src-tauri/src/ — Rust backend only
> Strategy: File splitting + minimal deduplication (Plan B)

## Goal

Split oversized Rust files into smaller, focused modules for long-term maintainability. No functional changes, no interface changes, no behavior changes. Pure structural refactoring.

## Constraints

- **Zero functional change** — `cargo build --release` output behaves identically before and after
- **Zero interface change** — all `pub fn` signatures unchanged; external `use` paths preserved via `pub use` re-exports
- **Zero trait change** — `SessionProvider`, `ParsedSession`, `SessionMeta`, `Message` untouched
- **Untouched files** — `models.rs`, `provider.rs`, `indexer.rs`, `watcher.rs`, `terminal.rs`, `trash_state.rs`, `main.rs`, `commands/` directory
- **No logic merging across providers** — each provider keeps its own independent implementation

## Target Directory Layout

```
src-tauri/src/
├── providers/
│   ├── mod.rs                    # pub use all providers (interface unchanged)
│   ├── claude/
│   │   ├── mod.rs                # ClaudeProvider struct + SessionProvider impl + new()
│   │   ├── parser.rs             # parse_session() + message extraction helpers
│   │   └── images.rs             # image placeholder merging, source extraction
│   ├── codex/
│   │   ├── mod.rs                # CodexProvider struct + SessionProvider impl + new()
│   │   ├── parser.rs             # parse_session_file() + event processing
│   │   └── tools.rs              # map_codex_tool_name, extract_tool_output, extract_codex_content
│   ├── gemini/
│   │   ├── mod.rs                # GeminiProvider struct + SessionProvider impl + new() + scan_impl + build_project_map
│   │   ├── logs_parser.rs        # parse_logs_json()
│   │   ├── chat_parser.rs        # parse_chat_file()
│   │   ├── orphan.rs             # merge_orphan_sessions()
│   │   ├── images.rs             # resolve_gemini_image_path, normalize_path
│   │   └── tools.rs              # map_gemini_tool_name
│   ├── cursor/
│   │   ├── mod.rs                # CursorProvider struct + SessionProvider impl + new() + open_db + read_blobs
│   │   ├── parser.rs             # parse_session_db() + load_messages detail parsing
│   │   └── tools.rs              # map_cursor_tool_name, remap_tool_args, think extraction
│   └── opencode/
│       ├── mod.rs                # OpenCodeProvider struct + SessionProvider impl + new()
│       └── parser.rs             # message/part parsing, extract_tokens, ms_to_rfc3339, capitalize_tool
│
├── db/
│   ├── mod.rs                    # Database struct, open() with schema/migrations/pragmas, with_transaction, pub use
│   ├── queries.rs                # get_session, list_*, search_*, favorites, meta, stats
│   ├── sync.rs                   # sync_provider_snapshot, sync_source_snapshot, upsert, delete_missing, rename, delete, clear
│   └── row_mapper.rs             # row_to_session_meta() — single helper replacing 4 duplicated column mappings
│
├── exporter/
│   ├── mod.rs                    # export() router (unchanged)
│   ├── json.rs                   # unchanged
│   ├── markdown.rs               # unchanged
│   ├── html.rs                   # render() logic (calls into templates)
│   └── templates.rs              # CSS_TEMPLATE, JS_TEMPLATE, HTML constants
│
├── provider_utils.rs             # existing + new FTS_CONTENT_LIMIT constant
├── lib.rs                        # updated mod declarations
└── (all other files unchanged)
```

## Provider Split Rules

Each provider follows the same pattern:

**mod.rs**: Provider struct + `SessionProvider` trait impl (`scan_all`, `scan_source`, `load_messages`, `watch_paths`, `provider`) + `new()` constructor. ~50-100 lines.

**parser.rs**: Core parsing function and its direct helpers (message extraction, token extraction, content assembly, timestamp handling). Largest code block per provider.

**Additional files**: Only when a group of functions forms a self-contained logical unit. If a helper is only called once from parser.rs and is under 30 lines, it stays in parser.rs.

### Per-Provider Breakdown

| Provider | Lines | mod.rs | parser.rs | Extra Files |
|----------|-------|--------|-----------|-------------|
| claude | 713 | struct + trait (~80) | parse_session + append_user_message + extract_token_usage + extract_message_content (~350) | images.rs (~150): 5 image functions |
| gemini | 957 | struct + trait + build_project_map + scan_impl (~200) | — | logs_parser.rs (~130), chat_parser.rs (~230), orphan.rs (~80), images.rs (~60), tools.rs (~15) |
| codex | 539 | struct + trait (~80) | parse_session_file + events (~300) | tools.rs (~100): tool mapping + output extraction |
| cursor | 504 | struct + trait + open_db + read_blobs (~80) | parse_session_db + detail parsing (~250) | tools.rs (~120): tool mapping + think extraction |
| opencode | 443 | struct + trait (~100) | all parsing + helpers (~300) | none (small enough for one parser file) |

## db/ Split Details

| File | Content | ~Lines |
|------|---------|--------|
| mod.rs | `Database` struct, `open()` (schema DDL + migrations + pragmas), `with_transaction()`, `pub use` | 200 |
| queries.rs | `get_session`, `list_sessions`, `list_recent_sessions`, `search_filtered`, `search_with_fts`, `search_with_like`, `build_fts_query`, `session_count`, `provider_session_counts`, `get_meta`/`set_meta`, `vacuum`, `db_size_bytes`, all favorites functions | 350 |
| sync.rs | `sync_provider_snapshot`, `sync_source_snapshot`, `upsert_session_on`, `delete_missing_sessions_for_source`, `delete_missing_sources_for_provider`, `rename_session`, `delete_session`, `clear_all` | 150 |
| row_mapper.rs | `pub fn row_to_session_meta(row: &Row) -> Result<SessionMeta>` | 30 |

### row_mapper.rs Deduplication

The `SessionMeta` column-index mapping is currently duplicated at 4 locations in db.rs. Extract to a single function:

```rust
// db/row_mapper.rs
use crate::models::{Provider, SessionMeta};

pub fn row_to_session_meta(row: &rusqlite::Row) -> rusqlite::Result<SessionMeta> {
    Ok(SessionMeta {
        id: row.get(0)?,
        provider: str_to_provider(&row.get::<_, String>(1)?),
        title: row.get(2)?,
        project_path: row.get(3)?,
        project_name: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        message_count: row.get(7)?,
        file_size_bytes: row.get(8)?,
        source_path: row.get(9)?,
        is_sidechain: row.get::<_, i32>(10)? != 0,
    })
}

fn str_to_provider(s: &str) -> Provider {
    Provider::from_str(s).unwrap_or(Provider::Claude)
}
```

The 4 call sites in queries.rs change from inline column mapping to `row_to_session_meta(&row)?`.

## exporter/ Split Details

| File | Content | ~Lines |
|------|---------|--------|
| html.rs | `render()` function — message iteration, HTML tag generation, calls template constants | 300 |
| templates.rs | `pub const CSS_TEMPLATE: &str`, `pub const JS_TEMPLATE: &str`, other HTML boilerplate constants | 350 |

## provider_utils.rs Change

Add one constant:

```rust
pub const FTS_CONTENT_LIMIT: usize = 2000;
```

Replace all 5 occurrences of `truncate_to_bytes(&full_content, 2000)` with `truncate_to_bytes(&full_content, FTS_CONTENT_LIMIT)` across provider files.

## Execution Order

Ordered by dependency (foundations first) and risk (smallest first for providers):

| Step | Module | Rationale |
|------|--------|-----------|
| 1 | provider_utils.rs constant | No dependencies, trivial change |
| 2 | db/ split | Widely depended on — stabilize foundation first |
| 3 | exporter/ split | Independent subsystem |
| 4 | opencode/ | Smallest provider — validate split pattern |
| 5 | cursor/ | Medium size |
| 6 | codex/ | Medium size |
| 7 | claude/ | Large, contains image subsystem |
| 8 | gemini/ | Largest and most complex — last |

## Verification Per Step

After each module split:

```bash
cargo clippy --quiet    # zero warnings
cargo build --release   # compiles
```

Fix any clippy warnings within the current module before proceeding to the next.

## Git Strategy

One atomic commit per module:

```
refactor: add FTS_CONTENT_LIMIT constant to provider_utils
refactor: split db.rs into db/ module
refactor: split exporter/html.rs templates
refactor: split opencode into opencode/ module
refactor: split cursor into cursor/ module
refactor: split codex into codex/ module
refactor: split claude into claude/ module
refactor: split gemini into gemini/ module
```

Each commit is independently revertible via `git revert`.

## What This Does NOT Do

- No provider logic merging or unification
- No trait changes or new abstractions
- No error handling improvements
- No transaction safety improvements
- No performance optimizations
- No new tests (separate effort)
- No frontend changes

These are all valid improvements but belong to future iterations, not this structural split.
