<p align="center">
  <a href="README.md">English</a> | <a href="README.zh-CN.md">中文</a>
</p>

<p align="center">
  <img src="assets/logo-text.svg" alt="CC Session" width="240">
</p>

<p align="center">
  浏览、搜索、恢复和管理你的 AI 编程会话，一个桌面应用搞定。
</p>

<p align="center">
  <a href="https://github.com/tyql688/cc-session/releases/latest"><img alt="Latest Release" src="https://img.shields.io/github/v/release/tyql688/cc-session?style=flat-square&color=blue"></a>

  <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows-lightgrey?style=flat-square">
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/tyql688/cc-session?style=flat-square"></a>
</p>

---

## 为什么需要 CC Session？

Claude Code、Codex、Gemini CLI 等 AI 编程工具会在本地存储会话数据，但没有方便的方式来浏览、搜索和回顾历史对话。CC Session 将所有工具的会话集中到一个统一界面 — 查看完整对话历史、跨工具全文搜索、导出记录、或直接在终端中恢复任意会话。

## 功能

- **统一视图** — 多种 AI 编程工具的所有会话集中展示
- **全文搜索** — SQLite FTS5，跨所有会话内容搜索（`⌘K`）
- **恢复会话** — 在 Terminal、iTerm2、Ghostty、Kitty、Warp、WezTerm、Alacritty 中快速恢复（`⇧⌘R`）
- **实时监听** — 活跃会话更新时自动刷新（`⌘L`）
- **丰富渲染** — Markdown、语法高亮、Mermaid 图表、KaTeX 数学公式、内嵌图片、结构化工具调用 diff
- **Token 统计** — 单条消息和会话级别 Token 计数，区分缓存命中/写入
- **导出** — JSON、Markdown 或独立 HTML（暗色模式、可折叠工具和思考块）
- **会话管理** — 重命名、回收站/恢复、收藏、批量操作
- **自动更新** — 内置更新器自动检查新版本
- **键盘驱动** — 完整的键盘导航（`?` 查看所有快捷键）
- **双语** — 英文 / 中文
- **屏蔽文件夹** — 隐藏特定项目目录的会话

## 支持的工具

| 工具        | 数据路径                              |  格式  | 实时监听 |
| :---------- | :------------------------------------ | :----: | :------: |
| Claude Code | `~/.claude/projects/**/*.jsonl`       | JSONL  | 文件事件 |
| Codex CLI   | `~/.codex/sessions/**/*.jsonl`        | JSONL  | 文件事件 |
| Gemini CLI  | `~/.gemini/tmp/*/chats/*.json`        |  JSON  | 文件事件 |
| Kimi CLI    | `~/.kimi/sessions/**/*.jsonl`         | JSONL  | 文件事件 |
| Cursor CLI  | `~/.cursor/chats/**/store.db`         | SQLite |   轮询   |
| OpenCode    | `~/.local/share/opencode/opencode.db` | SQLite |   轮询   |

每个工具解析：消息、工具调用（含输入/输出）、思考/推理块、Token 用量、内嵌图片。

## 安装

从 [Releases](https://github.com/tyql688/cc-session/releases) 下载最新版本：

- **macOS** — `.dmg`
- **Windows** — `.exe`（NSIS 安装包）

> **macOS Gatekeeper：** 应用未经代码签名。首次打开时 macOS 可能会阻止运行，执行以下命令修复：
>
> ```bash
> xattr -cr /Applications/CC Session.app
> ```

## 快捷键

| 按键         | 功能                |
| :----------- | :------------------ |
| `⌘K`         | 搜索                |
| `⌘1-9`       | 切换标签            |
| `⌘W` / `⇧⌘W` | 关闭标签 / 关闭全部 |
| `⌘]` / `⌘[`  | 下/上一个标签       |
| `⇧⌘R`        | 在终端恢复          |
| `⇧⌘E`        | 导出                |
| `⌘B`         | 切换收藏            |
| `⌘L`         | 切换实时监听        |
| `⌘⌫`         | 删除会话            |
| `⌘F`         | 会话内搜索          |
| `?`          | 显示所有快捷键      |

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

## 技术栈

[Tauri 2.0](https://v2.tauri.app/) · [Solid.js](https://www.solidjs.com/) · Rust · SQLite FTS5

## 许可证

[MIT](LICENSE)
