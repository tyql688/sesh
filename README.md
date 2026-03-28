<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-CN.md">中文</a>
</p>

<pre align="center">
 ╔═══╗ ╔═══╗ ╔═══╗ ╔╗ ╔╗
 ║ ══╣ ║══   ║ ══╣ ║╚═╝║
 ╠══ ║ ║══   ╠══ ║ ║ ══╣
 ╚═══╝ ╚═══╝ ╚═══╝ ╚╗ ╔╝
</pre>

<p align="center">
  Browse, search, and manage AI coding sessions.<br>
  Claude Code · Codex · Gemini · Cursor · OpenCode
</p>

<p align="center">
  <img alt="Tauri 2.0" src="https://img.shields.io/badge/Tauri-2.0-blue?logo=tauri">
  <img alt="Solid.js" src="https://img.shields.io/badge/Solid.js-TypeScript-2c4f7c?logo=solid">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-Backend-dea584?logo=rust">
  <img alt="License" src="https://img.shields.io/badge/License-MIT-green">
</p>

---

## What It Does

CC Session reads session data from your AI coding tools and presents them in a unified, searchable interface. Think of it as a session browser — you can view full conversation histories, search across all sessions, export them, or resume directly in your terminal.

## Supported Providers

| Provider    | Data Source                              | Format   | Live Watch |
|-------------|------------------------------------------|----------|------------|
| Claude Code | `~/.claude/projects/**/*.jsonl`          | JSONL    | FS events  |
| Codex       | `~/.codex/sessions/**/*.jsonl`           | JSONL    | FS events  |
| Gemini CLI  | `~/.gemini/tmp/*/chats/*.json`           | JSON     | FS events  |
| Cursor      | `~/.cursor/chats/**/store.db`            | SQLite   | Polling    |
| OpenCode    | `~/.local/share/opencode/opencode.db`    | SQLite   | Polling    |

Each provider parses: messages, tool calls (with input/output), thinking/reasoning blocks, token usage, and inline images.

## Features

- **Full-text search** across all session content (SQLite FTS5)
- **Live watch** — auto-refreshes when sessions update (`⌘L` to toggle)
- **Message rendering** — Markdown, syntax highlighting, Mermaid diagrams, KaTeX math, inline images, structured tool call diffs
- **Token usage** — per-message and session-level token counts with cache hit/write breakdown
- **Export** — JSON, Markdown, HTML (dark mode, collapsible tools, thinking blocks)
- **Session management** — rename, trash/restore, favorites, batch delete/export
- **Resume** — open any session in Terminal, iTerm2, Ghostty, Kitty, Warp, WezTerm, or Alacritty
- **Keyboard-driven** — `⌘K` search, `⌘1-9` tabs, `⌘B` favorite, `⌘L` watch, `?` shortcuts overlay
- **i18n** — English / Chinese

## Shortcuts

| Key | Action |
|-----|--------|
| `⌘K` | Search |
| `⌘1-9` | Switch tab |
| `⌘W` / `⇧⌘W` | Close tab / Close all |
| `⌘]` / `⌘[` | Next / Prev tab |
| `⇧⌘R` | Resume in terminal |
| `⇧⌘E` | Export |
| `⌘B` | Toggle favorite |
| `⌘L` | Toggle live watch |
| `⌘⌫` | Delete session |
| `⌘F` | Find in session |
| `?` | Show shortcuts |

## Install

Download the latest DMG from [Releases](https://github.com/tyql688/cc-session/releases).

> **macOS Gatekeeper:** The app is not code-signed. On first launch macOS may show *"CC Session is damaged and can't be opened"*. Fix with:
> ```bash
> xattr -cr /Applications/CC Session.app
> ```

## Build from Source

Requires [Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/) 18+.

```bash
git clone https://github.com/tyql688/cc-session.git
cd cc-session
npm install
npm run tauri build          # Production build
npx tauri build --bundles dmg  # DMG only
```

## Development

```bash
npm run tauri dev             # Dev with hot reload
npx tsc --noEmit              # Type-check frontend
cd src-tauri && cargo clippy  # Lint Rust
```

## Architecture

```
src/                     # Solid.js frontend
  components/            # UI components
  stores/                # Reactive state (toast, search, theme, settings, favorites)
  i18n/                  # en.json, zh.json
  styles/                # CSS variables, layout, messages
  lib/                   # Types, Tauri IPC wrappers
src-tauri/               # Rust backend
  src/providers/         # Session parsers (claude, codex, gemini, cursor, opencode)
  src/commands/          # Tauri IPC handlers
  src/exporter/          # JSON, Markdown, HTML export
  src/db.rs              # SQLite + FTS5 index
  src/indexer.rs         # Parallel scan with batch transactions
  src/watcher.rs         # File system watcher
  src/terminal.rs        # Terminal launch (7 terminals)
```

Adding a new provider: implement `SessionProvider` trait in Rust → done. No frontend changes needed.

## License

[MIT](LICENSE)
