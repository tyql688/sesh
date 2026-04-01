# Provider Architecture Refactor

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate scattered provider-specific logic from commands/indexer/frontend by moving it into providers, so adding a new provider only touches provider files.

**Architecture:** Extend `SessionProvider` trait with lifecycle methods (`is_shared_source`, `delete_from_source`, `owns_source_path`, `resume_command`, `display_key`, `sort_order`). Each provider implements what it needs; trait provides defaults. Frontend creates a record-based registry replacing switch statements. Commands/indexer become pure dispatch — no path heuristics, no provider string checks.

**Tech Stack:** Rust (Tauri 2.0, rusqlite), TypeScript (Solid.js)

---

## Phase 1: Rust — Extend SessionProvider Trait

### Task 1: Add new trait methods with defaults

**Files:**
- Modify: `src-tauri/src/provider.rs`

- [ ] **Step 1: Add trait methods**

Add these methods to the `SessionProvider` trait in `provider.rs`, after `load_messages`:

```rust
    /// Whether this provider's source files are shared across multiple sessions.
    /// Shared sources (e.g. OpenCode's opencode.db, Gemini's logs.json) cannot be
    /// physically moved to trash — only soft-deleted.
    fn is_shared_source(&self) -> bool {
        false
    }

    /// Delete a session's data from its shared source file.
    /// Only called for providers where `is_shared_source()` returns true.
    /// Called when trash is emptied or a trashed session is permanently deleted.
    fn delete_from_source(
        &self,
        _source_path: &str,
        _session_id: &str,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    /// Check if a source file path belongs to this provider.
    fn owns_source_path(&self, source_path: &str) -> bool;

    /// Build the CLI resume command for a session.
    /// `variant_name` is only used by CC-Mirror.
    fn resume_command(&self, session_id: &str, variant_name: Option<&str>) -> Option<String>;

    /// Key used to group sessions in the tree. Defaults to provider key.
    /// CC-Mirror overrides to include variant name (e.g. "cc-mirror:cczai").
    fn display_key(&self, variant_name: Option<&str>) -> String {
        let _ = variant_name;
        self.provider().key().to_string()
    }

    /// Sort order for provider groups in the tree.
    fn sort_order(&self) -> u32;
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check 2>&1`
Expected: FAIL — all 7 providers need `owns_source_path`, `resume_command`, `sort_order` (no defaults).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/provider.rs
git commit -m "refactor: extend SessionProvider trait with lifecycle methods"
```

---

### Task 2: Implement trait methods for Claude provider

**Files:**
- Modify: `src-tauri/src/providers/claude/mod.rs`

- [ ] **Step 1: Add implementations**

Add to the `impl SessionProvider for ClaudeProvider` block:

```rust
    fn owns_source_path(&self, source_path: &str) -> bool {
        let normalized = source_path.replace('\\', "/");
        normalized.contains("/.claude/projects/") && !normalized.contains("/.cc-mirror/")
    }

    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("claude --resume {session_id}"))
    }

    fn sort_order(&self) -> u32 {
        0
    }
```

- [ ] **Step 2: Verify compilation**

Run: `cd src-tauri && cargo check 2>&1`
Expected: FAIL (other providers still missing).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/providers/claude/mod.rs
git commit -m "refactor: implement trait methods for Claude provider"
```

---

### Task 3: Implement trait methods for Codex provider

**Files:**
- Modify: `src-tauri/src/providers/codex/mod.rs`

- [ ] **Step 1: Add implementations**

Add to the `impl SessionProvider for CodexProvider` block:

```rust
    fn owns_source_path(&self, source_path: &str) -> bool {
        source_path.replace('\\', "/").contains("/.codex/sessions/")
    }

    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("codex resume {session_id}"))
    }

    fn sort_order(&self) -> u32 {
        2
    }
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/providers/codex/mod.rs
git commit -m "refactor: implement trait methods for Codex provider"
```

---

### Task 4: Implement trait methods for Gemini provider

**Files:**
- Modify: `src-tauri/src/providers/gemini/mod.rs`

- [ ] **Step 1: Add implementations**

Gemini uses shared `logs.json` files. Add to the `impl SessionProvider for GeminiProvider` block:

```rust
    fn is_shared_source(&self) -> bool {
        true
    }

    fn delete_from_source(
        &self,
        _source_path: &str,
        _session_id: &str,
    ) -> Result<(), ProviderError> {
        // Gemini logs.json is a flat JSON file — session data is removed by
        // not including it during re-parse. The shared_deletions mechanism
        // prevents it from reappearing on next scan.
        Ok(())
    }

    fn owns_source_path(&self, source_path: &str) -> bool {
        source_path.replace('\\', "/").contains("/.gemini/tmp/")
    }

    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("gemini --resume {session_id}"))
    }

    fn sort_order(&self) -> u32 {
        3
    }
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/providers/gemini/mod.rs
git commit -m "refactor: implement trait methods for Gemini provider"
```

---

### Task 5: Implement trait methods for Cursor provider

**Files:**
- Modify: `src-tauri/src/providers/cursor/mod.rs`

- [ ] **Step 1: Add implementations**

Cursor uses one `store.db` per session — NOT shared:

```rust
    fn owns_source_path(&self, source_path: &str) -> bool {
        source_path.replace('\\', "/").contains("/.cursor/chats/")
    }

    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("agent --resume={session_id}"))
    }

    fn sort_order(&self) -> u32 {
        4
    }
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/providers/cursor/mod.rs
git commit -m "refactor: implement trait methods for Cursor provider"
```

---

### Task 6: Implement trait methods for OpenCode provider

**Files:**
- Modify: `src-tauri/src/providers/opencode/mod.rs`

- [ ] **Step 1: Add implementations**

OpenCode uses a shared `opencode.db`. Move the deletion SQL from `trash.rs` here:

```rust
    fn is_shared_source(&self) -> bool {
        true
    }

    fn delete_from_source(
        &self,
        source_path: &str,
        session_id: &str,
    ) -> Result<(), ProviderError> {
        let conn = rusqlite::Connection::open(source_path)?;
        let _ = conn.execute(
            "DELETE FROM part WHERE session_id = ?1",
            rusqlite::params![session_id],
        );
        let _ = conn.execute(
            "DELETE FROM message WHERE session_id = ?1",
            rusqlite::params![session_id],
        );
        let _ = conn.execute(
            "DELETE FROM todo WHERE session_id = ?1",
            rusqlite::params![session_id],
        );
        let _ = conn.execute(
            "DELETE FROM session_share WHERE session_id = ?1",
            rusqlite::params![session_id],
        );
        // Delete child sessions (subagents)
        let child_ids: Vec<String> = conn
            .prepare("SELECT id FROM session WHERE parent_id = ?1")
            .and_then(|mut stmt| {
                let rows = stmt.query_map(rusqlite::params![session_id], |row| row.get(0))?;
                Ok(rows.filter_map(|r| r.ok()).collect())
            })
            .unwrap_or_default();
        for cid in &child_ids {
            let _ = conn.execute("DELETE FROM part WHERE session_id = ?1", rusqlite::params![cid]);
            let _ = conn.execute(
                "DELETE FROM message WHERE session_id = ?1",
                rusqlite::params![cid],
            );
            let _ = conn.execute("DELETE FROM todo WHERE session_id = ?1", rusqlite::params![cid]);
            let _ = conn.execute(
                "DELETE FROM session_share WHERE session_id = ?1",
                rusqlite::params![cid],
            );
            let _ = conn.execute("DELETE FROM session WHERE id = ?1", rusqlite::params![cid]);
        }
        let _ = conn.execute(
            "DELETE FROM session WHERE id = ?1",
            rusqlite::params![session_id],
        );
        Ok(())
    }

    fn owns_source_path(&self, source_path: &str) -> bool {
        source_path.replace('\\', "/").contains("/opencode/opencode.db")
    }

    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("opencode -s {session_id}"))
    }

    fn sort_order(&self) -> u32 {
        5
    }
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/providers/opencode/mod.rs
git commit -m "refactor: implement trait methods for OpenCode provider"
```

---

### Task 7: Implement trait methods for Kimi provider

**Files:**
- Modify: `src-tauri/src/providers/kimi/mod.rs`

- [ ] **Step 1: Add implementations**

```rust
    fn owns_source_path(&self, source_path: &str) -> bool {
        source_path.replace('\\', "/").contains("/.kimi/sessions/")
    }

    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("kimi --session {session_id}"))
    }

    fn sort_order(&self) -> u32 {
        6
    }
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/providers/kimi/mod.rs
git commit -m "refactor: implement trait methods for Kimi provider"
```

---

### Task 8: Implement trait methods for CC-Mirror provider

**Files:**
- Modify: `src-tauri/src/providers/cc_mirror.rs`

- [ ] **Step 1: Add implementations**

CC-Mirror overrides `display_key` and `resume_command` to use variant names:

```rust
    fn owns_source_path(&self, source_path: &str) -> bool {
        let normalized = source_path.replace('\\', "/");
        normalized.contains("/.cc-mirror/") && normalized.contains("/config/projects/")
    }

    fn resume_command(&self, session_id: &str, variant_name: Option<&str>) -> Option<String> {
        variant_name.map(|name| format!("{name} --resume {session_id}"))
    }

    fn display_key(&self, variant_name: Option<&str>) -> String {
        match variant_name {
            Some(vn) => format!("cc-mirror:{vn}"),
            None => "cc-mirror".to_string(),
        }
    }

    fn sort_order(&self) -> u32 {
        1
    }
```

- [ ] **Step 2: Verify full compilation**

Run: `cd src-tauri && cargo check 2>&1`
Expected: PASS — all providers now implement required methods.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/providers/cc_mirror.rs
git commit -m "refactor: implement trait methods for CC-Mirror provider"
```

---

## Phase 2: Rust — Clean Up Commands & Indexer

### Task 9: Add `provider_from_source_path` helper using trait dispatch

**Files:**
- Modify: `src-tauri/src/provider.rs`

- [ ] **Step 1: Add helper function**

Add after `all_providers()`:

```rust
/// Identify which provider owns a source path by asking each provider.
pub fn provider_from_source_path(source_path: &str) -> Option<Provider> {
    for p in all_providers() {
        if p.owns_source_path(source_path) {
            return Some(p.provider());
        }
    }
    None
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/provider.rs
git commit -m "refactor: add provider_from_source_path using trait dispatch"
```

---

### Task 10: Refactor trash.rs — remove path heuristics

**Files:**
- Modify: `src-tauri/src/commands/trash.rs`

- [ ] **Step 1: Replace `is_shared_file` with trait dispatch**

Remove the `is_shared_file` function entirely. Replace the usage in `trash_session` (around line 74-87):

```rust
    // Determine if this is a shared source using provider trait
    let provider_enum = crate::models::Provider::parse(&resolved_provider);
    let provider_impl = provider_enum.as_ref().and_then(crate::provider::make_provider);
    let shared = provider_impl.as_ref().is_some_and(|p| p.is_shared_source());
```

- [ ] **Step 2: Replace `delete_from_source_db` with trait dispatch**

Remove the entire `delete_from_source_db` function. In `empty_trash` and `permanent_delete_trash`, replace calls to `delete_from_source_db` with:

```rust
    if let Some(provider_enum) = crate::models::Provider::parse(&entry.provider) {
        if let Some(provider_impl) = crate::provider::make_provider(&provider_enum) {
            let _ = provider_impl.delete_from_source(&entry.original_path, &entry.id);
        }
    }
```

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check 2>&1`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/trash.rs
git commit -m "refactor: trash.rs uses trait dispatch instead of path heuristics"
```

---

### Task 11: Refactor sessions.rs — remove `.db` checks and `PROVIDER_PATH_PATTERNS`

**Files:**
- Modify: `src-tauri/src/commands/sessions.rs`
- Modify: `src-tauri/src/provider_utils.rs`

- [ ] **Step 1: Replace `provider_from_source_path` in sessions.rs**

The local `provider_from_source_path` function in `sessions.rs` (line ~300) currently uses `PROVIDER_PATH_PATTERNS`. Replace it:

```rust
fn provider_from_source_path(source_path: &str) -> Option<Provider> {
    crate::provider::provider_from_source_path(source_path)
}
```

- [ ] **Step 2: Replace `.db` checks in `delete_session`**

In `delete_session`, replace the `/opencode.db` check with trait dispatch:

```rust
    if path.exists() {
        if provider_from_source_path(&source_path).is_none() {
            return Err(format!(
                "refused to delete '{}': not inside a known provider directory",
                source_path
            ));
        }
        // Skip physical deletion for shared sources (contain multiple sessions)
        let is_shared = provider_from_source_path(&source_path)
            .and_then(|p| crate::provider::make_provider(&p))
            .is_some_and(|p| p.is_shared_source());
        if !is_shared {
            std::fs::remove_file(path)
                .map_err(|e| format!("failed to delete file '{source_path}': {e}"))?;
        }
    }
```

- [ ] **Step 3: Same for `delete_sessions_batch`**

Apply the same pattern in `delete_sessions_batch`:

```rust
                let is_shared = provider_from_source_path(source_path)
                    .and_then(|p| crate::provider::make_provider(&p))
                    .is_some_and(|p| p.is_shared_source());
                if !is_shared {
                    std::fs::remove_file(path)
                        .map_err(|e| format!("failed to delete file {source_path}: {e}"))?;
                }
```

- [ ] **Step 4: Remove `PROVIDER_PATH_PATTERNS` from provider_utils.rs**

Delete the `PROVIDER_PATH_PATTERNS` constant (lines 100-115) from `provider_utils.rs`. Update tests in `sessions.rs` that reference it to use `crate::provider::provider_from_source_path` instead.

- [ ] **Step 5: Verify compilation and tests**

Run: `cd src-tauri && cargo check 2>&1 && cargo test 2>&1`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/sessions.rs src-tauri/src/provider_utils.rs
git commit -m "refactor: sessions.rs uses trait dispatch, remove PROVIDER_PATH_PATTERNS"
```

---

### Task 12: Refactor terminal.rs — remove cc-mirror special cases

**Files:**
- Modify: `src-tauri/src/commands/terminal.rs`

- [ ] **Step 1: Rewrite `get_resume_command`**

Replace the entire function body with trait dispatch:

```rust
#[tauri::command]
pub fn get_resume_command(
    session_id: String,
    provider: String,
    state: State<AppState>,
) -> Result<String, String> {
    let safe_id = sanitize_session_id(&session_id);
    let p = Provider::parse(&provider).ok_or_else(|| format!("unknown provider '{provider}'"))?;
    let provider_impl =
        crate::provider::make_provider(&p).ok_or("provider unavailable".to_string())?;

    let variant_name = state
        .db
        .get_session(&session_id)
        .ok()
        .flatten()
        .and_then(|s| s.variant_name);

    provider_impl
        .resume_command(&safe_id, variant_name.as_deref())
        .ok_or_else(|| format!("{} session missing variant name", provider))
}
```

- [ ] **Step 2: Rewrite `resume_session`**

Same pattern:

```rust
#[tauri::command]
pub fn resume_session(
    session_id: String,
    provider: String,
    terminal_app: String,
    state: State<AppState>,
) -> Result<(), String> {
    let safe_id = sanitize_session_id(&session_id);
    let p = Provider::parse(&provider).ok_or_else(|| format!("unknown provider '{provider}'"))?;
    let provider_impl =
        crate::provider::make_provider(&p).ok_or("provider unavailable".to_string())?;

    let session = state.db.get_session(&session_id).ok().flatten();
    let variant_name = session.as_ref().and_then(|s| s.variant_name.clone());

    let cmd = provider_impl
        .resume_command(&safe_id, variant_name.as_deref())
        .ok_or_else(|| format!("{} session missing variant name, cannot resume", provider))?;

    let cwd = session.and_then(|s| {
        if s.project_path.is_empty() {
            None
        } else {
            Some(s.project_path)
        }
    });

    terminal::launch_terminal(&terminal_app, &cmd, cwd.as_deref())
}
```

- [ ] **Step 3: Update `ALLOWED_PROVIDERS` in `open_in_terminal`**

Replace the hardcoded list with trait-based validation:

```rust
    let cmd_name = parts[0];
    let is_allowed = crate::provider::all_providers()
        .iter()
        .any(|p| {
            p.resume_command(cmd_name, None)
                .is_some_and(|c| c.starts_with(cmd_name))
        })
        || is_known_cc_mirror_variant(cmd_name);
```

Actually, simpler — keep the `ALLOWED_PROVIDERS` list but derive from providers. For now, a static list is fine as a security boundary. Just add a comment:

```rust
    // Security: only allow known CLI commands. If adding a provider, update here.
    const ALLOWED_PROVIDERS: &[&str] = &[
        "claude", "codex", "gemini", "cursor", "agent", "opencode", "kimi",
    ];
```

- [ ] **Step 4: Verify compilation**

Run: `cd src-tauri && cargo check 2>&1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/terminal.rs
git commit -m "refactor: terminal.rs uses trait dispatch, no cc-mirror special cases"
```

---

### Task 13: Refactor indexer.rs — remove cc-mirror special cases

**Files:**
- Modify: `src-tauri/src/indexer.rs`

- [ ] **Step 1: Rewrite `build_tree` to use trait methods**

Replace the `display_key` logic (lines 69-78) with:

```rust
        for session in sessions {
            let provider_impl = crate::provider::make_provider(&session.provider);
            let display_key = provider_impl
                .as_ref()
                .map(|p| p.display_key(session.variant_name.as_deref()))
                .unwrap_or_else(|| session.provider.key().to_string());
```

Replace the label construction (lines 96-108) with:

```rust
            let (provider_enum, label) =
                if let Some(variant_name) = display_key.strip_prefix("cc-mirror:") {
                    (Provider::CcMirror, variant_name.to_string())
                } else {
                    match Provider::parse(&display_key) {
                        Some(p) => {
                            let l = p.label().to_string();
                            (p, l)
                        }
                        None => continue,
                    }
                };
```

Note: The label construction for cc-mirror variants still needs the prefix check since `display_key` is a string. This is acceptable.

- [ ] **Step 2: Replace sort logic**

Replace the hardcoded sort (lines 229-241) with:

```rust
        tree.sort_by(|a, b| {
            let order_a = a
                .provider
                .as_ref()
                .and_then(crate::provider::make_provider)
                .map(|p| p.sort_order())
                .unwrap_or(99);
            let order_b = b
                .provider
                .as_ref()
                .and_then(crate::provider::make_provider)
                .map(|p| p.sort_order())
                .unwrap_or(99);
            order_a.cmp(&order_b).then(a.id.cmp(&b.id))
        });
```

- [ ] **Step 3: Verify compilation**

Run: `cd src-tauri && cargo check 2>&1`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/indexer.rs
git commit -m "refactor: indexer uses trait dispatch for display_key and sort_order"
```

---

### Task 14: Clean up Provider enum — remove methods moved to trait

**Files:**
- Modify: `src-tauri/src/models.rs`

- [ ] **Step 1: Remove `resume_command` and `color` from Provider enum**

Delete the `resume_command` method (lines 67-78) and `color` method (lines 55-65) from `impl Provider`. These are now on the trait or unused.

- [ ] **Step 2: Verify compilation and fix callers**

Run: `cd src-tauri && cargo check 2>&1`

If any callers remain for `resume_command`, they should already be updated in Tasks 12. If any callers remain for `color`, check if they're in the HTML exporter — the exporter can use `Provider::color()` or inline the map. Keep `color()` in the exporter file if needed (it's a display concern for HTML export, which doesn't have provider instances).

If the exporter needs it, move the color map to `src-tauri/src/exporter/html.rs` as a local function:

```rust
fn provider_color(provider: &Provider) -> &'static str {
    match provider {
        Provider::Claude => "#8b5cf6",
        Provider::Codex => "#10b981",
        Provider::Gemini => "#f59e0b",
        Provider::Cursor => "#3b82f6",
        Provider::OpenCode => "#06b6d4",
        Provider::Kimi => "#6366f1",
        Provider::CcMirror => "#f472b6",
    }
}
```

- [ ] **Step 3: Run full test suite**

Run: `cd src-tauri && cargo test 2>&1`
Expected: All 21+ tests PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/models.rs src-tauri/src/exporter/html.rs
git commit -m "refactor: remove resume_command/color from Provider enum"
```

---

## Phase 3: TypeScript — Provider Registry

### Task 15: Create provider registry with record-based config

**Files:**
- Create: `src/lib/provider-registry.ts`

- [ ] **Step 1: Write the registry**

```typescript
import type { Provider } from "./types";
import type { JSX } from "solid-js";

/** Per-provider configuration. Adding a provider = adding one entry here. */
export interface ProviderDef {
  key: Provider;
  label: string;
  /** CSS variable name (without --), e.g. "claude" → var(--claude) */
  colorVar: string;
  /** Build the CLI resume command. Returns empty for providers that need variant_name. */
  resumeCommand: (id: string) => string;
  /** How the frontend watches for live changes */
  watchStrategy: "fs" | "poll";
  /** Debounce delay in ms for FS-event-based watching */
  watchDebounceMs: number;
  /** Whether FS change matching should use directory prefix (not exact path) */
  watchMatchPrefix: boolean;
  /** Sort order in the sidebar tree */
  sortOrder: number;
}

const REGISTRY: Record<Provider, ProviderDef> = {
  claude: {
    key: "claude",
    label: "Claude Code",
    colorVar: "claude",
    resumeCommand: (id) => `claude --resume ${id}`,
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 0,
  },
  codex: {
    key: "codex",
    label: "Codex",
    colorVar: "codex",
    resumeCommand: (id) => `codex resume ${id}`,
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 2,
  },
  gemini: {
    key: "gemini",
    label: "Gemini",
    colorVar: "gemini",
    resumeCommand: (id) => `gemini --resume ${id}`,
    watchStrategy: "fs",
    watchDebounceMs: 800,
    watchMatchPrefix: true,
    sortOrder: 3,
  },
  cursor: {
    key: "cursor",
    label: "Cursor",
    colorVar: "cursor",
    resumeCommand: (id) => `agent --resume=${id}`,
    watchStrategy: "poll",
    watchDebounceMs: 2000,
    watchMatchPrefix: false,
    sortOrder: 4,
  },
  opencode: {
    key: "opencode",
    label: "OpenCode",
    colorVar: "opencode",
    resumeCommand: (id) => `opencode -s ${id}`,
    watchStrategy: "poll",
    watchDebounceMs: 2000,
    watchMatchPrefix: false,
    sortOrder: 5,
  },
  kimi: {
    key: "kimi",
    label: "Kimi CLI",
    colorVar: "kimi",
    resumeCommand: (id) => `kimi --session ${id}`,
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 6,
  },
  "cc-mirror": {
    key: "cc-mirror",
    label: "CC-Mirror",
    colorVar: "cc-mirror",
    resumeCommand: () => "",
    watchStrategy: "fs",
    watchDebounceMs: 300,
    watchMatchPrefix: false,
    sortOrder: 1,
  },
};

export function getProvider(provider: Provider): ProviderDef {
  return REGISTRY[provider];
}

export function getProviderLabel(provider: Provider): string {
  return REGISTRY[provider]?.label ?? provider;
}

export function getProviderColor(provider: Provider): string {
  return `var(--${REGISTRY[provider]?.colorVar ?? provider})`;
}

export function allProviders(): ProviderDef[] {
  return Object.values(REGISTRY);
}

/** Build resume command, handling cc-mirror variant names. */
export function buildResumeCommand(
  provider: Provider,
  sessionId: string,
  variantName?: string,
): string {
  if (provider === "cc-mirror" && variantName) {
    return `${variantName} --resume ${sessionId}`;
  }
  return REGISTRY[provider].resumeCommand(sessionId);
}

/** Get the display label, using variant name for cc-mirror. */
export function getDisplayLabel(provider: Provider, variantName?: string): string {
  if (provider === "cc-mirror" && variantName) {
    return variantName;
  }
  return REGISTRY[provider].label;
}
```

- [ ] **Step 2: Commit**

```bash
git add src/lib/provider-registry.ts
git commit -m "refactor: create provider registry as single source of truth"
```

---

### Task 16: Refactor icons — eliminate switch statement

**Files:**
- Modify: `src/lib/icons.tsx`

- [ ] **Step 1: Replace switch with record lookup**

Replace the `ProviderIcon` switch-case function with a record:

```typescript
import type { Provider } from "./types";

const PROVIDER_ICONS: Record<Provider, () => JSX.Element> = {
  claude: () => (
    <svg width="14" height="14" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg">
      <path d="M4.709 15.955l4.72-2.647..." fill="#D97757" fill-rule="nonzero" />
    </svg>
  ),
  // ... each provider's SVG as before ...
};

export function ProviderIcon(props: { provider: Provider }) {
  const Icon = PROVIDER_ICONS[props.provider];
  return Icon ? <Icon /> : <span>?</span>;
}
```

- [ ] **Step 2: Replace `ProviderDot` switch with registry**

```typescript
import { getProviderColor } from "./provider-registry";

export function ProviderDot(props: { provider: Provider }) {
  return (
    <span class="provider-dot provider-logo" style={{ color: getProviderColor(props.provider) }}>
      <ProviderIcon provider={props.provider} />
    </span>
  );
}
```

- [ ] **Step 3: Commit**

```bash
git add src/lib/icons.tsx
git commit -m "refactor: icons use record lookup instead of switch"
```

---

### Task 17: Delete old providers.ts — update all imports

**Files:**
- Delete: `src/lib/providers.ts`
- Modify: all files that import from `./providers` or `../lib/providers`

- [ ] **Step 1: Find all imports**

Run: `grep -rn "from.*providers" src/ --include="*.ts" --include="*.tsx" | grep -v node_modules | grep -v provider-registry`

- [ ] **Step 2: Update each import**

Change all imports from `"./providers"` / `"../lib/providers"` to `"./provider-registry"` / `"../lib/provider-registry"`. The function names are the same (`getProviderLabel`, `getProviderColor`, `getProviderConfig` → `getProvider`, `allProviders`).

Replace `getProviderConfig(p).resumeCommand(id)` with `buildResumeCommand(p, id)`.

- [ ] **Step 3: Delete providers.ts**

```bash
rm src/lib/providers.ts
```

- [ ] **Step 4: Type check**

Run: `npx tsc --noEmit 2>&1`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "refactor: replace providers.ts with provider-registry.ts"
```

---

## Phase 4: TypeScript — Clean Up Components

### Task 18: Refactor SessionView watch logic — use registry

**Files:**
- Modify: `src/components/SessionView/index.tsx`

- [ ] **Step 1: Replace watch heuristics with registry lookup**

Replace lines 262-289 (the watch strategy block) with:

```typescript
      if (isWatching) {
        const activeSourcePath = meta().source_path || props.session.source_path;
        const providerDef = getProvider(meta().provider);

        if (providerDef.watchStrategy === "poll") {
          pollTimer = setInterval(reloadSession, providerDef.watchDebounceMs);
        } else {
          unwatchFn = await listen<string[]>("sessions-changed", (event) => {
            const changedPaths = event.payload ?? [];
            if (!activeSourcePath) return;

            let matched: boolean;
            if (providerDef.watchMatchPrefix) {
              const dir = activeSourcePath.replace(/\/[^/]+\/[^/]+$/, "");
              matched = changedPaths.some((p) => p.startsWith(dir));
            } else {
              matched = changedPaths.includes(activeSourcePath);
            }
            if (!matched) return;

            clearTimeout(watchDebounce);
            watchDebounce = setTimeout(reloadSession, providerDef.watchDebounceMs);
          });
        }
      }
```

Add import at top:
```typescript
import { getProvider } from "../../lib/provider-registry";
```

- [ ] **Step 2: Type check**

Run: `npx tsc --noEmit 2>&1`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/components/SessionView/index.tsx
git commit -m "refactor: SessionView watch uses provider registry"
```

---

### Task 19: Refactor ContextMenus — use registry for resume

**Files:**
- Modify: `src/components/Explorer/ContextMenus.tsx`

- [ ] **Step 1: Replace cc-mirror special case**

Replace lines 40-47 with:

```typescript
        const provider = node.provider ?? "claude";
        const cmd = buildResumeCommand(provider, node.id, providerLabel);
```

Add import:
```typescript
import { buildResumeCommand } from "../../lib/provider-registry";
```

Remove unused `getProviderConfig` import.

- [ ] **Step 2: Commit**

```bash
git add src/components/Explorer/ContextMenus.tsx
git commit -m "refactor: ContextMenus uses buildResumeCommand"
```

---

### Task 20: Refactor SessionToolbar — use registry for label

**Files:**
- Modify: `src/components/SessionView/SessionToolbar.tsx`

- [ ] **Step 1: Replace cc-mirror special case**

Replace lines 24-30 with:

```typescript
  const providerLabel = () => {
    const meta = props.meta();
    return getDisplayLabel(meta.provider, meta.variant_name);
  };
```

Update import:
```typescript
import { getDisplayLabel } from "../../lib/provider-registry";
```

Remove unused `getProviderLabel` import from old providers.

- [ ] **Step 2: Commit**

```bash
git add src/components/SessionView/SessionToolbar.tsx
git commit -m "refactor: SessionToolbar uses getDisplayLabel"
```

---

## Phase 5: Verification & Cleanup

### Task 21: Full verification

**Files:** None (verification only)

- [ ] **Step 1: Rust checks**

```bash
cd src-tauri && cargo fmt -- --check 2>&1
cd src-tauri && cargo clippy 2>&1
cd src-tauri && cargo test 2>&1
```

Expected: All PASS

- [ ] **Step 2: TypeScript checks**

```bash
npx tsc --noEmit 2>&1
npm run lint 2>&1
npm run format:check 2>&1
npm test 2>&1
```

Expected: All PASS

- [ ] **Step 3: Dev smoke test**

```bash
npm run tauri dev
```

Verify:
- Tree loads with all providers
- Sessions open correctly
- Delete a session → appears in trash
- Restore from trash → session reappears
- Resume command copies correctly (including cc-mirror variant)
- Live watch works for file-based providers
- Live watch (poll) works for Cursor/OpenCode

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "refactor: final cleanup after provider architecture refactor"
```

---

## Summary: Before vs After

### Adding a new provider — files to touch

| Before (current) | After (refactored) |
|---|---|
| `models.rs` — Provider enum + 6 methods | `models.rs` — Provider enum + parse/key/label only |
| `provider.rs` — make_provider match | `provider.rs` — make_provider match |
| `provider_utils.rs` — PROVIDER_PATH_PATTERNS | *(deleted)* |
| `commands/trash.rs` — is_shared_file, delete_from_source_db | *(uses trait)* |
| `commands/sessions.rs` — .db checks (4 places) | *(uses trait)* |
| `commands/terminal.rs` — if "cc-mirror" (2 places) | *(uses trait)* |
| `indexer.rs` — cc-mirror display_key + sort | *(uses trait)* |
| `src/lib/providers.ts` — PROVIDERS object | *(deleted)* |
| `src/lib/icons.tsx` — switch (7 cases) | `src/lib/icons.tsx` — record (no switch) |
| `src/lib/types.ts` — Provider union | `src/lib/types.ts` — Provider union |
| `src/lib/provider-registry.ts` | `src/lib/provider-registry.ts` — single entry |
| `src/styles/variables.css` — colors | `src/styles/variables.css` — colors |
| **~12 files, ~8 with logic changes** | **~5 files, only 3 with logic (provider impl, provider.rs, registry.ts)** |
