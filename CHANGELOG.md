# Changelog

All notable changes to this project will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioned with [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.1] - 2026-03-30

### Added

- **Windows custom titlebar** — hide native decorations, render custom minimize/maximize/close buttons for consistent cross-platform look
- **TreeNode.project_path field** — structured field replaces fragile colon-split ID encoding, fixing CC-Mirror path parsing errors
- **Search request versioning** — stale search results are discarded, preventing slow queries from overwriting newer results
- **Export privacy redaction** — all export formats (HTML/MD/JSON) replace home directory paths with `~`

### Fixed

- **CC-Mirror asset scope** — added `$HOME/.cc-mirror/**` to assetProtocol.scope; images now load correctly
- **Provider disable filter** — `disabledProviders` now supports all 7 providers, not just claude/codex/gemini
- **HTML export image detection** — `[Image: source: ...]` markers with trailing text now correctly render as base64 images
- **HTML export path validation** — `inline_image()` validates paths against HOME/tmp allowlist before reading
- **SQLite delete protection** — `delete_session` skips physical deletion of shared `.db` files (Cursor/OpenCode), only removes from index
- **Provider fallback** — `str_to_provider` returns error instead of silently falling back to Claude
- **Path containment checks** — use `Path::starts_with` for component-aware validation instead of string prefix matching
- **open_in_folder** — added HOME path validation; returns error when `canonicalize` fails
- **Residual println!** — replaced with `log::info!` in indexer and watcher
- **Release script** — `cargo check` and `npm install` failures now abort the release instead of being silently swallowed

### Changed

- **Error visibility** — 6 frontend `console.warn` calls replaced with `toastError` for user-visible error notifications
- **Titlebar drag** — uses `-webkit-app-region: drag` CSS property with `no-drag` exclusions for interactive elements

## [0.2.0] - 2026-03-30

### Added

- **CC-Mirror provider** — multi-variant Claude Code aggregator support under `~/.cc-mirror/`, with per-variant session grouping and resume commands
- **Parser golden tests** — integration tests for Gemini, Cursor CLI, and OpenCode parsers with fixture files; fix `delete_session` validation

### Fixed

- **Windows terminal resume** — CMD terminal now spawns `cmd.exe` directly; Windows Terminal and PowerShell resume paths corrected
- **HTML export** — removed avatar background color for cleaner export output
- **ErrorBoundary** — frontend wrapped in ErrorBoundary to prevent white screen crashes
- **SQLite safety** — external SQLite connections now use `PRAGMA query_only` to prevent accidental writes
- **Provider fallback** — `Provider::parse` logs warning instead of silently defaulting to Claude
- **Release script** — added test gate before version bump
- **Terminal security** — hardened `open_in_terminal` command validation
- **Sync safety** — prevent full index deletion when provider scan returns 0 sessions
- **CI** — added `cargo test` step to CI pipeline

### Changed

- Removed dead `_searchMatches` memo in SessionView

## [0.1.5] - 2026-03-30

### Added

- **Linux platform support** — terminal launchers (GNOME Terminal, Konsole, xterm, Alacritty, Kitty, WezTerm) with auto-fallback detection
- **Linux release builds** — `.deb` and `.AppImage` packages in CI release workflow
- **Rust parser golden tests** — 21 integration tests for Claude, Codex, and Kimi parsers with fixture files
- **DB read/write separation** — separate SQLite connections for concurrent reads during reindex
- **Async heavy commands** — `reindex`, `sync_sources`, `delete_sessions_batch`, `export_sessions_batch` no longer block the main thread (tokio `spawn_blocking`)
- **HTML export image inlining** — images embedded as base64 data URIs instead of `file://` paths (self-contained, no path leakage)
- Linux CI checks (ubuntu-latest) alongside macOS and Windows
- Rust dependency caching (`Swatinem/rust-cache`) in CI
- ESLint and Prettier checks enforced in CI
- `isWindows` / `isLinux` platform detection in frontend

### Fixed

- **Kimi incremental sync** — `provider_from_source_path` now detects `/.kimi/sessions/` paths
- **Windows terminal detection** — frontend valid list includes `windows-terminal`, `powershell`, `cmd`
- **Windows temp path validation** — `read_image_base64` uses `TEMP`/`TMP` env vars instead of hardcoded Unix paths
- **Linux terminal UI** — `SettingsPanel` shows correct terminal options per platform
- **Linux terminal detection** — probes gnome-terminal, konsole, alacritty, kitty, wezterm, xfce4-terminal, xterm via `which`
- Provider constructors return `Option<Self>` instead of panicking with `.expect()` when HOME is unavailable
- `release.sh` sed syntax works on both BSD (macOS) and GNU (Linux)
- `localStorage` JSON.parse wrapped in try/catch to prevent crash on corrupted data
- `Provider::from_str` renamed to `Provider::parse` to satisfy clippy
- eslint-disable directives moved to correct lines for innerHTML usage
- `.prettierrc` with `endOfLine: lf` and `.gitattributes` for consistent line endings across platforms

### Changed

- Project renamed from Sesh to CC Session (full git history rewrite)
- All Tauri commands using `State<AppState>` now work with `Clone`-able `AppState` and `Indexer`

## [0.1.2] - 2026-03-29

### Added

- **Kimi CLI provider** — full support for `~/.kimi/sessions/**/*.jsonl` with tool calls, thinking blocks, token usage, and image handling
- Official brand SVG icons from lobe-icons for all providers (Claude, Codex, Gemini, Cursor CLI, OpenCode, Kimi CLI)
- ESLint + Prettier configuration with `npm run lint` and `npm run format` scripts
- MIT License
- Tauri v2 capabilities for minimal permissions
- CI checks: `cargo fmt --check`, `cargo clippy`, `tsc`, `eslint`
- Rust release profile optimization (LTO, strip, codegen-units=1)

### Fixed

- **P0 Bug**: `findSessionInTree` now recursively searches tree, fixing session operations when time grouping is enabled
- **P0 Bug**: CSS `var(--tab-hover)` → `var(--bg-tab-hover)` (4 occurrences)
- **Security**: Mermaid `securityLevel` changed from `"loose"` to `"strict"`
- **Security**: Markdown link scheme whitelist (only http/https/mailto allowed)
- **Security**: Terminal command validation with allowed prefix whitelist
- **Data safety**: `sync_provider_snapshot` skips destructive delete when scan returns <50% of indexed sessions
- Recent sessions list now refreshes on tree change (cold start, manual refresh, SQLite providers)
- Time grouping week starts Monday (ISO standard) instead of Sunday
- `strip_think_tags` O(n) single-pass instead of O(n^2)
- `str_to_provider` logs warning on unknown provider instead of silent default

### Changed

- **Module restructure (Rust)**: All providers split into sub-directories (claude/, codex/, gemini/, cursor/, opencode/, kimi/); db.rs → db/ module; exporter templates separated
- **Module restructure (Frontend)**: MessageBubble, SessionView, Explorer, App split into sub-directories; shared utilities extracted to lib/ (formatters, icons, platform, tree-utils, tree-builders)
- `row_to_session_meta()` helper eliminates 4 duplicated row mappings in db
- `FTS_CONTENT_LIMIT` constant replaces 6 magic number occurrences
- VACUUM removed from reindex hot path (only after clear)
- Cold start loads cached tree immediately, reindexes in background
- Cursor parallel scan with rayon `par_iter()`
- Avatar backgrounds removed — provider brand colors shown directly on icons
- Removed unused `lru` dependency

## [0.1.1] - 2026-03-29

### Added

- Blocked folders: sidebar panel to exclude folders from session indexing
- Auto-update support with Tauri updater plugin

### Fixed

- Blocked folders now correctly filter recent sessions
- VACUUM on reindex for smaller database size
- UI polish improvements

### Changed

- Upgraded CI actions to v5 (Node.js 24 support)
- Removed Rust test modules (SQLite disk IO issues in CI runner)

## [0.1.0] - 2026-03-28

### Added

- Multi-provider support: Claude Code, Codex, Gemini CLI, Cursor, OpenCode
- Full-text search across all session content (SQLite FTS5)
- Live session watch — auto-refresh on file changes (`⌘L`)
- Message rendering: Markdown, syntax highlighting, Mermaid diagrams, KaTeX math
- Inline image preview with click-to-expand
- Structured tool call display with diff view
- Token usage display (per-message and session totals)
- Thinking/reasoning block rendering (collapsible)
- Export: JSON, Markdown, HTML (dark mode, structured tools, thinking blocks)
- Session management: rename, trash/restore, favorites, batch operations
- Resume sessions in 7 terminal apps
- Keyboard shortcuts with overlay (`?`)
- Light / Dark / System theme
- English / Chinese localization
- Window state persistence across restarts
