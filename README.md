<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-CN.md">中文</a>
</p>

<p align="center">
  <img src="assets/logo-text.svg" alt="CC Session" width="240">
</p>

<p align="center">
  Browse, search, resume and manage your AI coding sessions in one desktop app.
</p>

<p align="center">
  <a href="https://github.com/tyql688/cc-session/releases/latest"><img alt="Latest Release" src="https://img.shields.io/github/v/release/tyql688/cc-session?style=flat-square&color=blue"></a>

  <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey?style=flat-square">
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/tyql688/cc-session?style=flat-square"></a>
</p>

---

## Why CC Session?

AI coding tools like Claude Code, Codex, and Gemini CLI store session data locally, but there's no easy way to browse, search, or revisit past conversations. CC Session brings all your sessions together in one unified interface — view full conversation histories, search across all providers with full-text search, export records, and resume any session directly in your terminal.

## Features

- **Unified view** — All your AI coding sessions from multiple providers in one place
- **Full-text search** — Search across all session content with SQLite FTS5 (`⌘K`)
- **Resume sessions** — Jump back into any session in Terminal, iTerm2, Ghostty, Kitty, Warp, WezTerm, or Alacritty (`⇧⌘R`)
- **Live watch** — Auto-refreshes when active sessions update (`⌘L`)
- **Rich rendering** — Markdown, syntax highlighting, Mermaid diagrams, KaTeX math, inline images, structured tool call diffs
- **Token usage** — Per-message and session-level token counts with cache hit/write breakdown
- **Export** — JSON, Markdown, or self-contained HTML (dark mode, collapsible tools & thinking blocks)
- **Session management** — Rename, trash/restore, favorites, batch operations
- **Auto-update** — Built-in updater checks for new releases automatically
- **Keyboard-driven** — Full keyboard navigation (`?` to see all shortcuts)
- **i18n** — English / Chinese
- **Blocked folders** — Hide sessions from specific project directories

## Supported Providers

| Provider | Data Source | Format | Live Watch |
|:---|:---|:---:|:---:|
| Claude Code | `~/.claude/projects/**/*.jsonl` | JSONL | FS events |
| Codex CLI | `~/.codex/sessions/**/*.jsonl` | JSONL | FS events |
| Gemini CLI | `~/.gemini/tmp/*/chats/*.json` | JSON | FS events |
| Kimi CLI | `~/.kimi/sessions/**/*.jsonl` | JSONL | FS events |
| Cursor CLI | `~/.cursor/chats/**/store.db` | SQLite | Polling |
| OpenCode | `~/.local/share/opencode/opencode.db` | SQLite | Polling |

Each provider parses: messages, tool calls (with input/output), thinking/reasoning blocks, token usage, and inline images.

## Install

Download the latest release from [Releases](https://github.com/tyql688/cc-session/releases):

- **macOS** — `.dmg`
- **Windows** — `.exe` (NSIS installer)

> **macOS Gatekeeper:** The app is not code-signed. On first launch, macOS may block it. Fix with:
> ```bash
> xattr -cr /Applications/CC Session.app
> ```

## Keyboard Shortcuts

| Key | Action |
|:---|:---|
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
| `?` | Show all shortcuts |

## Build from Source

Requires [Rust](https://rustup.rs/) and [Node.js](https://nodejs.org/) 18+.

```bash
git clone https://github.com/tyql688/cc-session.git
cd cc-session
npm install
npm run tauri build              # Production build
npx tauri build --bundles dmg    # DMG only
```

## Development

```bash
npm run tauri dev                # Dev with hot reload
npx tsc --noEmit                 # Type-check frontend
cd src-tauri && cargo clippy     # Lint Rust
```

## Built With

[Tauri 2.0](https://v2.tauri.app/) · [Solid.js](https://www.solidjs.com/) · Rust · SQLite FTS5

## License

[MIT](LICENSE)
