# Changelog

All notable changes to this project will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioned with [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.5] - 2026-04-03

### Added

- Locate active session button in Explorer header (#12)
- Collapse Explorer sidebar button (#12)
- Drag-to-resize Explorer sidebar (#12)
- Cache read/write token totals in session toolbar (#12)
- Shorten home directory paths in session toolbar (#12)
- Full markdown rendering in HTML export (#12)
- Provider-aware auto-polling for OpenCode and Gemini sessions (#12)
- Trash consistency audit at startup (#12)

### Fixed

- Prevent trashed OpenCode sessions from reappearing after reindex (#12)
- Escape code fence language in HTML export to prevent XSS (#12)
- Log and backup corrupt trash state files instead of silent fallback (#12)
- Fix protective sync for background polling (#12)

## [0.3.3] - 2026-04-03

### Fixed

- Show "up to date" feedback when no update is available (64944ae)
- Fix timer race where error timeout could overwrite subsequent update state (64944ae)
- Show detailed error message on download/install failure (64944ae)

## [0.3.2] - 2026-04-03

### Added

- Auto-check for updates on app startup with status bar badge (c7a72fc)

### Fixed

- Fix session image paths on Windows (8117bed)
- Fix updater signature file not generated in release builds (06bdfe6)
- Fix release workflow build paths and artifact downloads (#11)

## [0.3.1] - 2026-04-03

### Added

- Kimi CLI subagent support with tree navigation and "Open" jump (#10)
- Cursor CLI JSONL transcript parser, replacing old store.db approach (#10)
- Cursor CLI subagent support (#10)
- Trash lifecycle with parent-child session linking and cascading restore (#10)
- Agent "Open" button improvements with agentId matching and regex fallback (#10)

### Fixed

- Preserve Kimi subagent titles after trash by keeping subagents/ directory (#10)
- Fix Kimi tree view order instability (#10)
- Clean up Cursor store.db on permanent delete (#10)
- Strip `[REDACTED]` markers from Cursor assistant text (#10)
- Fix Cursor subagent title collision with full-text matching (#10)
- Restore child sessions reliably across all providers via parent_id (#10)
- Prevent accidental deletion of shared Gemini directories (#10)

## [0.3.0] - 2026-04-02

### Added

- Provider Bridge architecture with per-provider metadata descriptors (#9)
- Per-provider trash strategy replacing centralized if/else dispatch (#9)
- TypeScript provider registry replacing all switch/if-else blocks (#9)
- Claude, Codex, and OpenCode subagent session support (#8, #9)
- Per-message model display showing model name, version, and git branch (#8)
- Session metadata extraction (model, cc_version, git_branch) (#8)
- Parse `<persisted-output>` references in tool results (#8)
- Tab keep-alive preserving scroll position across tab switches (#8)
- Agent jump-to-subagent button in tool calls (#8)
- Orphan subagent management with show/hide toggle (#9)
- Ctrl+Click folder to select all child sessions (#9)
- Recent sessions filtering and metadata display (#9)

### Fixed

- Fix Cursor sessions being permanently deleted instead of trashed (#9)
- Fix Gemini shared log file reviving after trash (#9)
- Sanitize CC-Mirror variant name in resume command to prevent shell injection (#9)
- Wrap OpenCode delete in SQLite transaction (#9)
- Fix subagent tree rendering and deep reveal (#9)
- Fix scroll position restore on tab switch (#8)
- Reclaim disk space properly on clear index (#9)

### Removed

- Replace providers.ts with provider-registry.ts (#9)

## [0.2.1] - 2026-03-30

### Added

- Windows custom titlebar with minimize/maximize/close buttons (#7)
- Search request versioning to discard stale results (#7)
- Export privacy redaction replacing home paths with `~` (#7)

### Fixed

- Fix CC-Mirror image loading by adding asset scope (#7)
- Fix provider disable filter to support all 7 providers (#7)
- Fix HTML export image detection for markers with trailing text (#7)
- Validate image paths against HOME/tmp allowlist before reading (#7)
- Skip physical deletion of shared SQLite files (Cursor/OpenCode) (#7)
- Use component-aware path validation instead of string prefix matching (#7)

## [0.2.0] - 2026-03-30

### Added

- CC-Mirror provider for multi-variant Claude Code sessions (#5)
- Parser golden tests for Gemini, Cursor CLI, and OpenCode (#6)

### Fixed

- Fix Windows terminal resume for CMD and Windows Terminal (#4)
- Wrap external SQLite connections with `PRAGMA query_only` (#2)
- Prevent full index deletion when provider scan returns 0 sessions (#2)
- Harden terminal command validation (#2)
- Add ErrorBoundary to prevent white screen crashes (#2)

## [0.1.5] - 2026-03-30

### Added

- Linux platform support with terminal auto-detection (#1)
- Linux release builds (.deb and .AppImage) (#1)
- Rust parser golden tests for Claude, Codex, and Kimi (4d97b2e)
- Async heavy commands (reindex, sync, batch delete/export) no longer block main thread (bb34012)
- HTML export with base64 image inlining (94dc93d)

### Fixed

- Fix Kimi incremental sync path detection (1485b2c)
- Fix Windows terminal detection and temp path validation (#1)
- Fix provider constructors panicking when HOME is unavailable (#1)

## [0.1.2] - 2026-03-29

### Added

- Kimi CLI provider with tool calls, thinking blocks, token usage, and images (f8d244f)
- Official brand SVG icons for all providers (3ce4710)
- ESLint + Prettier configuration (1e7d5b4)

### Fixed

- Fix recursive tree search breaking session operations when time grouping is enabled (e2f2a60)
- Fix CSS variable typo `var(--tab-hover)` → `var(--bg-tab-hover)` (e2f2a60)
- Tighten Mermaid security level from "loose" to "strict" (e2f2a60)
- Restrict markdown link schemes to http/https/mailto (e2f2a60)
- Skip destructive sync when scan returns <50% of indexed sessions (e2f2a60)

## [0.1.1] - 2026-03-29

### Added

- Blocked folders panel to exclude folders from session indexing (2247fe1)
- Auto-update support with Tauri updater plugin (2247fe1)

## [0.1.0] - 2026-03-28

### Added

- Multi-provider support: Claude Code, Codex, Gemini CLI, Cursor, OpenCode
- Full-text search across all session content (SQLite FTS5)
- Live session watch with auto-refresh on file changes
- Markdown rendering with syntax highlighting, Mermaid diagrams, and KaTeX math
- Inline image preview with click-to-expand
- Structured tool call display with diff view
- Token usage display (per-message and session totals)
- Collapsible thinking/reasoning blocks
- Export to JSON, Markdown, and HTML
- Session management: rename, trash/restore, favorites, batch operations
- Resume sessions in 7 terminal apps
- Keyboard shortcuts with overlay
- Light / Dark / System theme
- English / Chinese localization
- Window state persistence across restarts
