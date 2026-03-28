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
  浏览、搜索、管理 AI 编程会话。<br>
  Claude Code · Codex · Gemini · Cursor · OpenCode
</p>

<p align="center">
  <img alt="Tauri 2.0" src="https://img.shields.io/badge/Tauri-2.0-blue?logo=tauri">
  <img alt="Solid.js" src="https://img.shields.io/badge/Solid.js-TypeScript-2c4f7c?logo=solid">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-Backend-dea584?logo=rust">
  <img alt="License" src="https://img.shields.io/badge/License-MIT-green">
</p>

---

## 这是什么

CC Session 读取本地 AI 编程工具的会话数据，在一个统一的界面中浏览和搜索。可以查看完整对话历史、跨会话搜索、导出记录、或直接在终端中恢复会话。

## 支持的工具

| 工具        | 数据路径                                 | 格式     | 实时监听   |
|-------------|------------------------------------------|----------|------------|
| Claude Code | `~/.claude/projects/**/*.jsonl`          | JSONL    | 文件事件   |
| Codex       | `~/.codex/sessions/**/*.jsonl`           | JSONL    | 文件事件   |
| Gemini CLI  | `~/.gemini/tmp/*/chats/*.json`           | JSON     | 文件事件   |
| Cursor      | `~/.cursor/chats/**/store.db`            | SQLite   | 轮询       |
| OpenCode    | `~/.local/share/opencode/opencode.db`    | SQLite   | 轮询       |

每个工具解析：消息、工具调用（含输入/输出）、思考/推理块、Token 用量、内嵌图片。

## 功能

- **全文搜索** — SQLite FTS5，跨所有会话内容
- **实时监听** — 会话更新时自动刷新（`⌘L` 切换）
- **消息渲染** — Markdown、语法高亮、Mermaid 图表、KaTeX 数学公式、内嵌图片、结构化工具调用 diff
- **Token 统计** — 单条消息和会话级别的 Token 计数，区分缓存命中/写入
- **导出** — JSON、Markdown、HTML（暗色模式、可折叠工具、思考块）
- **会话管理** — 重命名、回收站/恢复、收藏、批量删除/导出
- **恢复会话** — 在 Terminal、iTerm2、Ghostty、Kitty、Warp、WezTerm、Alacritty 中打开
- **键盘驱动** — `⌘K` 搜索、`⌘1-9` 标签、`⌘B` 收藏、`⌘L` 监听、`?` 快捷键面板
- **双语** — 英文 / 中文

## 快捷键

| 按键 | 功能 |
|------|------|
| `⌘K` | 搜索 |
| `⌘1-9` | 切换标签 |
| `⌘W` / `⇧⌘W` | 关闭标签 / 关闭全部 |
| `⌘]` / `⌘[` | 下/上一个标签 |
| `⇧⌘R` | 在终端恢复 |
| `⇧⌘E` | 导出 |
| `⌘B` | 切换收藏 |
| `⌘L` | 切换实时监听 |
| `⌘⌫` | 删除会话 |
| `⌘F` | 会话内搜索 |
| `?` | 显示快捷键 |

## 安装

从 [Releases](https://github.com/tyql688/cc-session/releases) 下载最新 DMG。

> **macOS Gatekeeper：** 应用未经代码签名。首次打开时 macOS 可能提示 *"CC Session 已损坏，无法打开"*，执行以下命令修复：
> ```bash
> xattr -cr /Applications/CC Session.app
> ```

## 从源码构建

需要 [Rust](https://rustup.rs/) 和 [Node.js](https://nodejs.org/) 18+。

```bash
git clone https://github.com/tyql688/cc-session.git
cd cc-session
npm install
npm run tauri build              # 生产构建
npx tauri build --bundles dmg    # 仅 DMG
```

## 开发

```bash
npm run tauri dev                # 热重载开发
npx tsc --noEmit                 # 前端类型检查
cd src-tauri && cargo clippy     # Rust 检查
```

## 架构

```
src/                     # Solid.js 前端
  components/            # UI 组件
  stores/                # 响应式状态（toast, search, theme, settings, favorites）
  i18n/                  # en.json, zh.json
  styles/                # CSS 变量、布局、消息样式
  lib/                   # 类型、Tauri IPC 封装
src-tauri/               # Rust 后端
  src/providers/         # 会话解析器（claude, codex, gemini, cursor, opencode）
  src/commands/          # Tauri IPC 处理
  src/exporter/          # JSON, Markdown, HTML 导出
  src/db.rs              # SQLite + FTS5 索引
  src/indexer.rs         # 并行扫描 + 批量事务
  src/watcher.rs         # 文件系统监听
  src/terminal.rs        # 终端启动（7 种终端）
```

添加新工具：在 Rust 端实现 `SessionProvider` trait 即可，前端无需改动。

## 许可证

[MIT](LICENSE)
