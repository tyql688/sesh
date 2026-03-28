# Rust Backend Module Split — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split oversized Rust files into focused sub-modules for maintainability, with zero functional changes.

**Architecture:** Each large file (provider, db, exporter) becomes a directory with `mod.rs` + sub-files. Public interfaces are preserved via `pub use` re-exports. Three minimal deduplication changes: extract `row_to_session_meta()` helper, define `FTS_CONTENT_LIMIT` constant, separate HTML templates.

**Tech Stack:** Rust, Tauri 2.0, rusqlite, serde_json, rayon

**Spec:** `docs/superpowers/specs/2026-03-29-rust-backend-split-design.md`

---

## Task 1: Add FTS_CONTENT_LIMIT constant to provider_utils.rs

**Files:**
- Modify: `src-tauri/src/provider_utils.rs:5` (add constant after `NO_PROJECT`)
- Modify: `src-tauri/src/providers/claude.rs:406`
- Modify: `src-tauri/src/providers/codex.rs:356`
- Modify: `src-tauri/src/providers/gemini.rs:190,430`
- Modify: `src-tauri/src/providers/cursor.rs:152`
- Modify: `src-tauri/src/providers/opencode.rs:180`

- [ ] **Step 1: Add constant to provider_utils.rs**

In `src-tauri/src/provider_utils.rs`, after line 5 (`pub const NO_PROJECT: &str = "(No Project)";`), add:

```rust
pub const FTS_CONTENT_LIMIT: usize = 2000;
```

- [ ] **Step 2: Update claude.rs**

In `src-tauri/src/providers/claude.rs`, update the `use` statement (line 12) to include `FTS_CONTENT_LIMIT`:

```rust
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, FTS_CONTENT_LIMIT,
};
```

Then change line 406 from:
```rust
let content_text = truncate_to_bytes(&full_content, 2000);
```
to:
```rust
let content_text = truncate_to_bytes(&full_content, FTS_CONTENT_LIMIT);
```

- [ ] **Step 3: Update codex.rs**

In `src-tauri/src/providers/codex.rs`, update the `use` statement (lines 13-14) to include `FTS_CONTENT_LIMIT`:

```rust
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, NO_PROJECT, FTS_CONTENT_LIMIT,
};
```

Then change line 356 from:
```rust
let content_text = truncate_to_bytes(&full_content, 2000);
```
to:
```rust
let content_text = truncate_to_bytes(&full_content, FTS_CONTENT_LIMIT);
```

- [ ] **Step 4: Update gemini.rs**

In `src-tauri/src/providers/gemini.rs`, update the `use` statement (lines 10-11) to include `FTS_CONTENT_LIMIT`:

```rust
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, NO_PROJECT, FTS_CONTENT_LIMIT,
};
```

Then change line 190 from:
```rust
let content_text = truncate_to_bytes(&full_content, 2000);
```
to:
```rust
let content_text = truncate_to_bytes(&full_content, FTS_CONTENT_LIMIT);
```

And change line 430 from:
```rust
let content_text = truncate_to_bytes(&content_parts.join("\n"), 2000);
```
to:
```rust
let content_text = truncate_to_bytes(&content_parts.join("\n"), FTS_CONTENT_LIMIT);
```

- [ ] **Step 5: Update cursor.rs**

In `src-tauri/src/providers/cursor.rs`, update the `use` statement (line 10) to include `FTS_CONTENT_LIMIT`:

```rust
use crate::provider_utils::{is_system_content, session_title, truncate_to_bytes, FTS_CONTENT_LIMIT};
```

Then change line 152 from:
```rust
content_text: truncate_to_bytes(&content_parts.join("\n"), 2000),
```
to:
```rust
content_text: truncate_to_bytes(&content_parts.join("\n"), FTS_CONTENT_LIMIT),
```

- [ ] **Step 6: Update opencode.rs**

In `src-tauri/src/providers/opencode.rs`, update the `use` statement (line 7) to include `FTS_CONTENT_LIMIT`:

```rust
use crate::provider_utils::{session_title, truncate_to_bytes, FTS_CONTENT_LIMIT};
```

Then change line 180 from:
```rust
content_text: truncate_to_bytes(&content_text, 2000),
```
to:
```rust
content_text: truncate_to_bytes(&content_text, FTS_CONTENT_LIMIT),
```

- [ ] **Step 7: Verify**

Run:
```bash
cd src-tauri && cargo clippy --quiet 2>&1
```
Expected: zero warnings, zero errors.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/provider_utils.rs src-tauri/src/providers/claude.rs src-tauri/src/providers/codex.rs src-tauri/src/providers/gemini.rs src-tauri/src/providers/cursor.rs src-tauri/src/providers/opencode.rs
git commit -m "refactor: add FTS_CONTENT_LIMIT constant to provider_utils"
```

---

## Task 2: Split db.rs into db/ module

**Files:**
- Delete: `src-tauri/src/db.rs`
- Create: `src-tauri/src/db/mod.rs`
- Create: `src-tauri/src/db/row_mapper.rs`
- Create: `src-tauri/src/db/queries.rs`
- Create: `src-tauri/src/db/sync.rs`

**No changes to lib.rs needed** — `mod db;` resolves to both `db.rs` and `db/mod.rs`.

- [ ] **Step 1: Create db/ directory**

```bash
mkdir -p src-tauri/src/db
```

- [ ] **Step 2: Create db/row_mapper.rs**

Create `src-tauri/src/db/row_mapper.rs` with the extracted row mapping helper:

```rust
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
        is_sidechain: row.get::<_, i64>(10).unwrap_or(0) != 0,
    })
}

fn str_to_provider(s: &str) -> Provider {
    Provider::from_str(s).unwrap_or(Provider::Claude)
}
```

- [ ] **Step 3: Create db/sync.rs**

Create `src-tauri/src/db/sync.rs` containing these functions moved from db.rs:

- `upsert_session_on()` (lines 446-485)
- `delete_missing_sessions_for_source()` (lines 487-509)
- `delete_missing_sources_for_provider()` (lines 511-535)
- `repeat_vars()` (lines 723-727)
- `provider_to_str()` (lines 434-436) — private, used by upsert

And the `Database` impl methods that use them:

- `sync_provider_snapshot()` (lines 148-186)
- `sync_source_snapshot()` (lines 188-212)
- `rename_session()` (lines 243-250)
- `delete_session()` (lines 262-267)
- `clear_all()` (lines 252-260)

The file should start with:

```rust
use std::collections::HashSet;
use rusqlite::{params, Connection};
use crate::models::{Provider, SessionMeta};
use crate::provider::ParsedSession;
use super::Database;

impl Database {
    pub fn sync_provider_snapshot(
        &self,
        provider: &Provider,
        sessions: &[ParsedSession],
    ) -> Result<(), rusqlite::Error> {
        // ... exact existing code from lines 149-186
    }

    pub fn sync_source_snapshot(
        &self,
        provider: &Provider,
        source_path: &str,
        sessions: &[ParsedSession],
    ) -> Result<(), rusqlite::Error> {
        // ... exact existing code from lines 189-212
    }

    pub fn rename_session(&self, id: &str, new_title: &str) -> Result<(), rusqlite::Error> {
        // ... exact existing code from lines 243-250
    }

    pub fn clear_all(&self) -> Result<(), rusqlite::Error> {
        // ... exact existing code from lines 252-260
    }

    pub fn delete_session(&self, id: &str) -> Result<(), rusqlite::Error> {
        // ... exact existing code from lines 262-267
    }
}

fn provider_to_str(provider: &Provider) -> &'static str {
    provider.key()
}

fn upsert_session_on(
    conn: &Connection,
    meta: &SessionMeta,
    content_text: &str,
) -> Result<(), rusqlite::Error> {
    // ... exact existing code from lines 450-485
}

fn delete_missing_sessions_for_source(
    conn: &Connection,
    provider_key: &str,
    source_path: &str,
    ids: &HashSet<String>,
) -> Result<(), rusqlite::Error> {
    // ... exact existing code from lines 492-509
}

fn delete_missing_sources_for_provider(
    conn: &Connection,
    provider_key: &str,
    source_paths: &[String],
) -> Result<(), rusqlite::Error> {
    // ... exact existing code from lines 515-535
}

fn repeat_vars(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}
```

Note: `sync_provider_snapshot` and `sync_source_snapshot` call `self.lock_conn()` and `self.with_transaction()` which are defined in `mod.rs` — this works because they are in the same `impl Database` (split across files via `super::Database`).

- [ ] **Step 4: Create db/queries.rs**

Create `src-tauri/src/db/queries.rs` containing all query/read functions:

- `get_session()` (lines 214-241) — **replace inline row mapping with `row_mapper::row_to_session_meta(&row)?`**
- `list_sessions()` (lines 269-278)
- `search_filtered()` (lines 280-301)
- `session_count()` (lines 303-306)
- `get_meta()` / `set_meta()` (lines 308-327)
- `vacuum()` (lines 329-332)
- `db_size_bytes()` (lines 334-338)
- `provider_session_counts()` (lines 340-353)
- `list_recent_sessions()` (lines 355-366)
- `add_favorite()` / `remove_favorite()` / `is_favorite()` / `list_favorites()` (lines 368-431)
- `provider_to_str_pub()` (lines 438-440)
- `list_sessions_from_query()` (lines 537-567) — **replace inline row mapping**
- `search_with_fts()` (lines 569-586)
- `search_with_like()` (lines 588-620)
- `append_search_filters()` (lines 622-643)
- `append_search_filters_numbered()` (lines 645-672)
- `query_search_results()` (lines 674-706) — **replace inline row mapping**
- `build_fts_query()` (lines 708-721)

The file should start with:

```rust
use std::collections::HashMap;
use rusqlite::params;
use crate::models::{Provider, SearchFilters, SearchResult, SessionMeta};
use super::Database;
use super::row_mapper::row_to_session_meta;

impl Database {
    pub fn get_session(&self, id: &str) -> Result<Option<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, provider, title, project_path, project_name,
                    created_at, updated_at, message_count, file_size_bytes, source_path, is_sidechain
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            row_to_session_meta(row)
        })?;
        match rows.next() {
            Some(Ok(meta)) => Ok(Some(meta)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    // ... rest of query functions, all using row_to_session_meta() where applicable
}
```

**Key change**: The 4 inline `SessionMeta { id: row.get(0)?, ... }` blocks become `row_to_session_meta(row)` calls:
1. `get_session()` — `query_map` closure
2. `list_favorites()` — `query_map` closure
3. `list_sessions_from_query()` — `query_map` closure
4. `query_search_results()` — `query_map` closure wraps in `SearchResult { session: row_to_session_meta(row)?, snippet: row.get(11)? }`

- [ ] **Step 5: Create db/mod.rs**

Create `src-tauri/src/db/mod.rs` with the Database struct, open(), lock_conn(), with_transaction(), and module declarations:

```rust
mod queries;
mod row_mapper;
mod sync;

use std::path::Path;
use std::sync::Mutex;
use rusqlite::{params, Connection};

pub use queries::provider_to_str_pub;

pub struct Database {
    conn: Mutex<Connection>,
    db_path: std::path::PathBuf,
}

impl Database {
    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, rusqlite::Error> {
        // ... exact existing code from lines 17-24
    }
}

impl Database {
    pub fn with_transaction<T, F>(&self, f: F) -> Result<T, rusqlite::Error>
    where
        F: FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    {
        // ... exact existing code from lines 28-44
    }

    pub fn open(data_dir: &Path) -> Result<Self, rusqlite::Error> {
        // ... exact existing code from lines 46-146
    }
}
```

- [ ] **Step 6: Delete old db.rs**

```bash
rm src-tauri/src/db.rs
```

- [ ] **Step 7: Verify**

```bash
cd src-tauri && cargo clippy --quiet 2>&1
```
Expected: zero warnings, zero errors.

- [ ] **Step 8: Commit**

```bash
git add -A src-tauri/src/db/ && git add src-tauri/src/db.rs
git commit -m "refactor: split db.rs into db/ module"
```

---

## Task 3: Split exporter/html.rs templates

**Files:**
- Modify: `src-tauri/src/exporter/html.rs`
- Create: `src-tauri/src/exporter/templates.rs`
- Modify: `src-tauri/src/exporter/mod.rs`

- [ ] **Step 1: Identify the template boundary**

In `html.rs`, the `render()` function (lines 360-646) contains a large `format!(r#"<!DOCTYPE html>..."#)` string from line 523 to line 645. This template includes inline CSS (~85 lines) and HTML structure.

The split approach: extract the `format!()` template string into `templates.rs` as a public function that accepts the computed values as parameters.

- [ ] **Step 2: Create exporter/templates.rs**

Create `src-tauri/src/exporter/templates.rs`:

```rust
/// Assemble the final HTML document from pre-rendered parts.
pub fn assemble_html(
    title: &str,
    provider_label: &str,
    provider_clr: &str,
    project: &str,
    count: u32,
    date: &str,
    messages_html: &str,
    token_summary_html: &str,
) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
... (exact content from html.rs lines 524-645, using the same format parameters)
"#,
        title = title,
        provider_label = provider_label,
        provider_clr = provider_clr,
        project = project,
        count = count,
        date = date,
        messages_html = messages_html,
        token_summary_html = token_summary_html,
    )
}
```

The entire `format!()` call from lines 523-645 of `html.rs` moves here verbatim. The function parameters match the format placeholders exactly.

- [ ] **Step 3: Update html.rs render()**

In `html.rs`, replace lines 523-645 (the `format!(...)` call) with:

```rust
    super::templates::assemble_html(
        &title,
        &provider_label,
        provider_clr,
        &project,
        count,
        &html_escape(&date),
        &messages_html,
        &token_summary_html,
    )
```

- [ ] **Step 4: Update exporter/mod.rs**

Add the module declaration. Current content of `src-tauri/src/exporter/mod.rs`:

```rust
mod html;
mod json;
mod markdown;
mod templates;  // ADD THIS LINE
```

- [ ] **Step 5: Verify**

```bash
cd src-tauri && cargo clippy --quiet 2>&1
```
Expected: zero warnings, zero errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/exporter/
git commit -m "refactor: split exporter/html.rs templates"
```

---

## Task 4: Split opencode into opencode/ module

**Files:**
- Delete: `src-tauri/src/providers/opencode.rs`
- Create: `src-tauri/src/providers/opencode/mod.rs`
- Create: `src-tauri/src/providers/opencode/parser.rs`
- Modify: `src-tauri/src/providers/mod.rs`

- [ ] **Step 1: Create directory**

```bash
mkdir -p src-tauri/src/providers/opencode
```

- [ ] **Step 2: Create opencode/parser.rs**

Move these functions (they are free functions, not impl methods):

- `extract_tokens()` (lines 47-66)
- `ms_to_rfc3339()` (lines 69-72)
- `capitalize_tool()` (lines 437-442)

And the message loading logic from `load_messages()` will stay in mod.rs since it's a trait impl.

```rust
use crate::models::TokenUsage;
use serde_json::Value;

pub fn extract_tokens(msg_json: &Value) -> Option<TokenUsage> {
    // ... exact existing code from lines 47-66
}

pub fn ms_to_rfc3339(ms: i64) -> Option<String> {
    // ... exact existing code from lines 69-72
}

pub fn capitalize_tool(name: &str) -> String {
    // ... exact existing code from lines 437-442
}
```

- [ ] **Step 3: Create opencode/mod.rs**

Contains `OpenCodeProvider` struct, `new()`, `open_db()`, and all `SessionProvider` trait impl methods. References parser functions via `parser::extract_tokens()` etc.

```rust
mod parser;

use std::path::PathBuf;
use rusqlite::{params, Connection};
use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::provider_utils::{session_title, truncate_to_bytes, FTS_CONTENT_LIMIT};

use parser::{extract_tokens, ms_to_rfc3339, capitalize_tool};

pub struct OpenCodeProvider {
    // ... exact existing struct fields
}

impl OpenCodeProvider {
    pub fn new() -> Self {
        // ... exact existing code from lines 14-27
    }

    fn open_db(&self) -> Option<Connection> {
        // ... exact existing code from lines 29-43
    }
}

impl SessionProvider for OpenCodeProvider {
    fn provider(&self) -> Provider {
        // ... exact existing code
    }
    fn watch_paths(&self) -> Vec<PathBuf> {
        // ... exact existing code
    }
    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        // ... exact existing code from lines 87-186
        // calls to extract_tokens, ms_to_rfc3339 now go through parser:: prefix
    }
    fn load_messages(&self, session_id: &str, _source_path: &str) -> Result<Vec<Message>, ProviderError> {
        // ... exact existing code from lines 189-433
        // calls to extract_tokens, ms_to_rfc3339, capitalize_tool now go through parser:: prefix
    }
}
```

- [ ] **Step 4: Update providers/mod.rs**

The current `providers/mod.rs` has `pub mod opencode;`. This continues to work — Rust resolves `opencode` to either `opencode.rs` or `opencode/mod.rs`. No change needed.

- [ ] **Step 5: Delete old file**

```bash
rm src-tauri/src/providers/opencode.rs
```

- [ ] **Step 6: Verify**

```bash
cd src-tauri && cargo clippy --quiet 2>&1
```
Expected: zero warnings, zero errors.

- [ ] **Step 7: Commit**

```bash
git add -A src-tauri/src/providers/opencode/ && git add src-tauri/src/providers/opencode.rs
git commit -m "refactor: split opencode into opencode/ module"
```

---

## Task 5: Split cursor into cursor/ module

**Files:**
- Delete: `src-tauri/src/providers/cursor.rs`
- Create: `src-tauri/src/providers/cursor/mod.rs`
- Create: `src-tauri/src/providers/cursor/parser.rs`
- Create: `src-tauri/src/providers/cursor/tools.rs`

- [ ] **Step 1: Create directory**

```bash
mkdir -p src-tauri/src/providers/cursor
```

- [ ] **Step 2: Create cursor/tools.rs**

Move these free functions:

- `extract_user_text()` (lines 158-176)
- `extract_tag_content()` (lines 179-186)
- `extract_workspace_path()` (lines 189-200)
- `extract_text_content()` (lines 203-205)
- `extract_text_from_content()` (lines 207-221)
- `extract_text_from_parts()` (lines 223-234)
- `parse_content_array()` (lines 237-249)
- `strip_think_tags()` (lines 252-266)
- `extract_think_content()` (lines 269-279)
- `map_cursor_tool_name()` (lines 282-294)
- `remap_tool_args()` (lines 297-333)

```rust
use serde_json::Value;

pub fn extract_user_text(parts: &[Value]) -> String {
    // ... exact existing code from lines 158-176
}

// ... all other functions with exact existing code
```

- [ ] **Step 3: Create cursor/parser.rs**

Move `parse_session_db()` (lines 57-154). It calls functions from tools.rs:

```rust
use std::path::Path;
use rusqlite::Connection;
use serde_json::Value;
use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::ParsedSession;
use crate::provider_utils::{is_system_content, session_title, truncate_to_bytes, FTS_CONTENT_LIMIT};

use super::tools::*;
use super::CursorProvider;

impl CursorProvider {
    pub(super) fn parse_session_db(db_path: &Path) -> Option<ParsedSession> {
        // ... exact existing code from lines 57-154
        // Uses open_db() via Self::open_db(), read_blobs() via Self::read_blobs()
        // Uses extract_user_text, extract_workspace_path, etc. from tools
    }
}
```

Note: `parse_session_db` calls `Self::open_db()` and `Self::read_blobs()`, which stay in mod.rs. This works because `parse_session_db` is an `impl CursorProvider` method defined in a sub-module.

- [ ] **Step 4: Create cursor/mod.rs**

```rust
mod parser;
pub(crate) mod tools;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use rusqlite::Connection;
use serde_json::Value;
use walkdir::WalkDir;
use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::provider_utils::{is_system_content, session_title, truncate_to_bytes, FTS_CONTENT_LIMIT};

use tools::*;

pub struct CursorProvider {
    // ... exact existing struct
}

impl CursorProvider {
    pub fn new() -> Self {
        // ... lines 17-21
    }
    fn chats_dir(&self) -> PathBuf {
        // ... lines 23-25
    }
    pub(super) fn open_db(db_path: &Path) -> Option<Connection> {
        // ... lines 27-39
    }
    pub(super) fn read_blobs(conn: &Connection) -> Vec<String> {
        // ... lines 42-55
    }
}

impl SessionProvider for CursorProvider {
    fn provider(&self) -> Provider { ... }
    fn watch_paths(&self) -> Vec<PathBuf> { ... }
    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        // ... lines 349-369, calls Self::parse_session_db()
    }
    fn load_messages(&self, _session_id: &str, source_path: &str) -> Result<Vec<Message>, ProviderError> {
        // ... lines 371-503, uses tools functions directly
    }
}
```

- [ ] **Step 5: Delete old file and verify**

```bash
rm src-tauri/src/providers/cursor.rs
cd src-tauri && cargo clippy --quiet 2>&1
```
Expected: zero warnings, zero errors.

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/src/providers/cursor/ && git add src-tauri/src/providers/cursor.rs
git commit -m "refactor: split cursor into cursor/ module"
```

---

## Task 6: Split codex into codex/ module

**Files:**
- Delete: `src-tauri/src/providers/codex.rs`
- Create: `src-tauri/src/providers/codex/mod.rs`
- Create: `src-tauri/src/providers/codex/parser.rs`
- Create: `src-tauri/src/providers/codex/tools.rs`

- [ ] **Step 1: Create directory**

```bash
mkdir -p src-tauri/src/providers/codex
```

- [ ] **Step 2: Create codex/tools.rs**

Move these free functions:

- `extract_codex_content()` (lines 380-394)
- `extract_codex_array_content()` (lines 396-423)
- `extract_codex_text()` (lines 425-430)
- `is_codex_image_wrapper()` (lines 432-435)
- `extract_tool_output()` (lines 439-462)
- `map_codex_tool_name()` (lines 465-478)
- `strip_inline_image_sources()` (lines 480-496)

```rust
use serde_json::Value;

pub fn extract_codex_content(content: &Value) -> String {
    // ... exact existing code
}
// ... all other functions
```

- [ ] **Step 3: Create codex/parser.rs**

Move `parse_session_file()` (lines 59-377):

```rust
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use serde_json::{json, Value};
use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, NO_PROJECT, FTS_CONTENT_LIMIT,
};

use super::tools::*;
use super::CodexProvider;

impl CodexProvider {
    pub(super) fn parse_session_file(path: &PathBuf) -> Option<ParsedSession> {
        // ... exact existing code from lines 59-377
    }
}
```

- [ ] **Step 4: Create codex/mod.rs**

```rust
mod parser;
mod tools;

use std::fs;
use std::path::PathBuf;
use rayon::prelude::*;
use serde::Deserialize;
use walkdir::WalkDir;
use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};

// Deserialized structs if any (e.g. CodexManifest)
#[derive(Deserialize)]
struct CodexManifest { /* ... if used in collect_jsonl_files */ }

pub struct CodexProvider {
    // ... exact existing struct
}

impl CodexProvider {
    pub fn new() -> Self { ... }
    fn sessions_dir(&self) -> PathBuf { ... }
    fn archived_sessions_dir(&self) -> PathBuf { ... }
    fn collect_jsonl_files(&self) -> Vec<PathBuf> { ... }
}

impl SessionProvider for CodexProvider {
    fn provider(&self) -> Provider { ... }
    fn watch_paths(&self) -> Vec<PathBuf> { ... }
    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        // ... calls Self::parse_session_file() via par_iter
    }
    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> { ... }
    fn load_messages(&self, _session_id: &str, source_path: &str) -> Result<Vec<Message>, ProviderError> { ... }
}
```

- [ ] **Step 5: Delete old file and verify**

```bash
rm src-tauri/src/providers/codex.rs
cd src-tauri && cargo clippy --quiet 2>&1
```

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/src/providers/codex/ && git add src-tauri/src/providers/codex.rs
git commit -m "refactor: split codex into codex/ module"
```

---

## Task 7: Split claude into claude/ module

**Files:**
- Delete: `src-tauri/src/providers/claude.rs`
- Create: `src-tauri/src/providers/claude/mod.rs`
- Create: `src-tauri/src/providers/claude/parser.rs`
- Create: `src-tauri/src/providers/claude/images.rs`

- [ ] **Step 1: Create directory**

```bash
mkdir -p src-tauri/src/providers/claude
```

- [ ] **Step 2: Create claude/images.rs**

Move these free functions:

- `contains_image_source()` (line 600-602)
- `contains_image_placeholder_without_source()` (lines 604-606)
- `merge_image_placeholders_with_sources()` (lines 608-648)
- `extract_image_source_segments()` (lines 650-669)
- `is_image_placeholder()` (lines 671-673)

```rust
use crate::models::Message;

pub fn contains_image_source(content: &str) -> bool {
    // ... exact existing code
}

pub fn contains_image_placeholder_without_source(content: &str) -> bool {
    // ... exact existing code
}

pub fn merge_image_placeholders_with_sources(messages: &mut Vec<Message>) {
    // ... exact existing code from lines 608-648
}

pub fn extract_image_source_segments(content: &str) -> Vec<String> {
    // ... exact existing code from lines 650-669
}

pub fn is_image_placeholder(content: &str) -> bool {
    // ... exact existing code
}
```

- [ ] **Step 3: Create claude/parser.rs**

Move `parse_session()` (lines 69-429) and its direct helpers:

- `append_user_message()` (lines 432-461)
- `flush_pending_user_message()` (lines 463-472)
- `extract_token_usage()` (lines 475-496)
- `extract_message_content()` (lines 501-546)
- `is_tool_result_message()` (lines 551-558)
- `extract_tool_result_content()` (lines 562-598)

```rust
use std::path::PathBuf;
use std::fs::File;
use std::io::{BufRead, BufReader};
use serde_json::Value;
use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, FTS_CONTENT_LIMIT,
};

use super::images::merge_image_placeholders_with_sources;
use super::ClaudeProvider;

impl ClaudeProvider {
    pub(super) fn parse_session(path: &PathBuf) -> Option<ParsedSession> {
        // ... exact existing code from lines 69-429
        // At the end, calls merge_image_placeholders_with_sources()
    }
}

fn append_user_message(...) { ... }
fn flush_pending_user_message(...) { ... }
fn extract_token_usage(...) { ... }
fn extract_message_content(...) { ... }
fn is_tool_result_message(...) { ... }
fn extract_tool_result_content(...) { ... }
```

- [ ] **Step 4: Create claude/mod.rs**

```rust
mod images;
mod parser;

use std::fs;
use std::path::PathBuf;
use rayon::prelude::*;
use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};

pub struct ClaudeProvider {
    // ... exact existing struct
}

impl ClaudeProvider {
    pub fn new() -> Self { ... }
    fn projects_dir(&self) -> PathBuf { ... }
    fn collect_jsonl_files(&self) -> Vec<PathBuf> { ... }
}

impl SessionProvider for ClaudeProvider {
    fn provider(&self) -> Provider { ... }
    fn watch_paths(&self) -> Vec<PathBuf> { ... }
    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        // ... calls Self::parse_session() via par_iter
    }
    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> { ... }
    fn load_messages(&self, _session_id: &str, source_path: &str) -> Result<Vec<Message>, ProviderError> { ... }
}
```

- [ ] **Step 5: Delete old file and verify**

```bash
rm src-tauri/src/providers/claude.rs
cd src-tauri && cargo clippy --quiet 2>&1
```

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/src/providers/claude/ && git add src-tauri/src/providers/claude.rs
git commit -m "refactor: split claude into claude/ module"
```

---

## Task 8: Split gemini into gemini/ module

**Files:**
- Delete: `src-tauri/src/providers/gemini.rs`
- Create: `src-tauri/src/providers/gemini/mod.rs`
- Create: `src-tauri/src/providers/gemini/logs_parser.rs`
- Create: `src-tauri/src/providers/gemini/chat_parser.rs`
- Create: `src-tauri/src/providers/gemini/orphan.rs`
- Create: `src-tauri/src/providers/gemini/images.rs`
- Create: `src-tauri/src/providers/gemini/tools.rs`

This is the most complex split — 957 lines with multiple independent subsystems.

- [ ] **Step 1: Create directory**

```bash
mkdir -p src-tauri/src/providers/gemini
```

- [ ] **Step 2: Create gemini/tools.rs**

Move:
- `map_gemini_tool_name()` (lines 838-849)
- `normalize_gemini_message()` (lines 851-893)

```rust
use serde_json::Value;
use crate::models::Message;

pub fn map_gemini_tool_name(name: &str) -> &str {
    // ... exact existing code
}

pub fn normalize_gemini_message(msg: &mut Message) {
    // ... exact existing code
}
```

- [ ] **Step 3: Create gemini/images.rs**

Move:
- `strip_at_image_refs()` (lines 812-835)
- `resolve_gemini_image_path()` (lines 895-932)
- `looks_like_image_path()` (lines 934-939)
- `normalize_path()` (lines 941-957)

```rust
use std::path::{Component, Path, PathBuf};

pub fn strip_at_image_refs(parts: &[serde_json::Value]) -> Vec<String> {
    // ... exact existing code
}

pub fn resolve_gemini_image_path(raw: &str, session_dir: &Path) -> Option<String> {
    // ... exact existing code
}

pub fn looks_like_image_path(s: &str) -> bool {
    // ... exact existing code
}

pub fn normalize_path(path: &Path) -> PathBuf {
    // ... exact existing code
}
```

- [ ] **Step 4: Create gemini/orphan.rs**

Move:
- `chat_session_ids()` (lines 670-701)
- `collect_real_session_prefixes()` (lines 706-732)
- `merge_orphan_sessions()` (lines 735-808)

```rust
use std::collections::{HashMap, HashSet};
use std::path::Path;
use crate::provider::ParsedSession;

pub fn chat_session_ids(chat_dir: &Path) -> HashSet<String> {
    // ... exact existing code
}

pub fn collect_real_session_prefixes(chat_ids: &HashSet<String>) -> HashSet<String> {
    // ... exact existing code
}

pub fn merge_orphan_sessions(
    log_sessions: &mut Vec<ParsedSession>,
    real_prefixes: &HashSet<String>,
) {
    // ... exact existing code
}
```

- [ ] **Step 5: Create gemini/logs_parser.rs**

Move `parse_logs_json()` (lines 96-221). This is a `GeminiProvider` impl method:

```rust
use std::collections::HashMap;
use std::fs;
use serde_json::Value;
use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, NO_PROJECT, FTS_CONTENT_LIMIT,
};

use super::images::strip_at_image_refs;
use super::tools::map_gemini_tool_name;
use super::GeminiProvider;

impl GeminiProvider {
    pub(super) fn parse_logs_json(
        &self,
        logs_path: &std::path::Path,
        project_map: &HashMap<String, String>,
    ) -> Vec<ParsedSession> {
        // ... exact existing code from lines 96-221
    }
}
```

- [ ] **Step 6: Create gemini/chat_parser.rs**

Move `parse_chat_file()` (lines 223-451):

```rust
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use serde_json::Value;
use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, NO_PROJECT, FTS_CONTENT_LIMIT,
};

use super::images::{resolve_gemini_image_path, strip_at_image_refs};
use super::tools::{map_gemini_tool_name, normalize_gemini_message};
use super::GeminiProvider;

impl GeminiProvider {
    pub(super) fn parse_chat_file(
        &self,
        path: &Path,
        project_map: &HashMap<String, String>,
    ) -> Option<ParsedSession> {
        // ... exact existing code from lines 223-451
    }
}
```

- [ ] **Step 7: Create gemini/mod.rs**

Contains: struct, `new()`, directory helpers, `build_project_map()`, `scan_impl()`, and `SessionProvider` trait impl.

```rust
mod chat_parser;
mod images;
mod logs_parser;
mod orphan;
mod tools;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use serde::Deserialize;
use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::trash_state::active_shared_deletions_by_source;

use orphan::{chat_session_ids, collect_real_session_prefixes, merge_orphan_sessions};

// Deserialized structs (if any, e.g. GeminiProject)
#[derive(Deserialize)]
struct GeminiProject { /* ... */ }

pub struct GeminiProvider {
    // ... exact existing struct
}

impl GeminiProvider {
    pub fn new() -> Self { ... }
    fn gemini_dir(&self) -> PathBuf { ... }
    fn tmp_dir(&self) -> PathBuf { ... }
    fn projects_json_path(&self) -> PathBuf { ... }
    fn build_project_map(&self) -> HashMap<String, String> { ... }
    fn scan_impl(&self, since_millis: Option<i64>) -> Result<Vec<ParsedSession>, ProviderError> {
        // ... lines 455-547
        // calls self.parse_logs_json(), self.parse_chat_file(), merge_orphan_sessions()
    }
}

impl SessionProvider for GeminiProvider {
    fn provider(&self) -> Provider { ... }
    fn watch_paths(&self) -> Vec<PathBuf> { ... }
    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> { ... }
    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> { ... }
    fn load_messages(&self, session_id: &str, source_path: &str) -> Result<Vec<Message>, ProviderError> { ... }
}
```

- [ ] **Step 8: Delete old file and verify**

```bash
rm src-tauri/src/providers/gemini.rs
cd src-tauri && cargo clippy --quiet 2>&1
```
Expected: zero warnings, zero errors.

- [ ] **Step 9: Commit**

```bash
git add -A src-tauri/src/providers/gemini/ && git add src-tauri/src/providers/gemini.rs
git commit -m "refactor: split gemini into gemini/ module"
```

---

## Final Verification

After all 8 tasks are complete:

- [ ] **Full build check**

```bash
cd src-tauri && cargo build --release 2>&1
```
Expected: clean compile.

- [ ] **Verify no logic changes via quick smoke test**

```bash
npm run tauri dev
```
Open the app, verify sessions load, search works, export works. Close the app.
