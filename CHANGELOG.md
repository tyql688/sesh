# Changelog

All notable changes to this project will be documented in this file.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
versioned with [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.2] - 2026-04-03

### Added

- **Auto-update checker** — 应用启动 2s 后自动检查更新，通过 updater store 状态机管理 idle→checking→available→downloading→ready→error 各阶段
- **StatusBar update badge** — 状态栏右侧显示更新可用徽标

### Fixed

- **Windows session image paths** — `read_image_base64` 支持 Windows verbatim-path、BOM 剥离、路径规范化
- **Updater 签名文件未生成** — `tauri.conf.json` 添加 `createUpdaterArtifacts: true`
- **Release workflow 构建路径** — 统一三平台 Prepare 为 Collect step，合并 assemble-latest-json 到 publish-release，升级 artifact actions v4→v5
- **Release artifact 下载失败** — 替换 tauri-action 为手动 `npx tauri build`，各平台通过 upload-artifact 上传产物

### Changed

- **AboutSettings** — 使用 updater store 替代本地状态管理更新信息

## [0.3.1] - 2026-04-03

### Added

- **Kimi CLI subagent support** — 从父 wire.jsonl 的 `SubagentEvent` 解析子代理会话，支持树形嵌套、"Open" 跳转、meta.json 标题提取
- **Cursor CLI JSONL transcript parser** — 全新解析器，从 `agent-transcripts/*.jsonl` 读取会话内容，替代旧 store.db blob 方式
- **Cursor CLI subagent support** — 解析 `subagents/*.jsonl`，全文匹配描述（避免前缀碰撞），按 Task 顺序排列
- **Provider deletion lifecycle** — `DeletionPlan` + `RestoreAction` + `cleanup_on_permanent_delete` 完整回收站生命周期
- **`TrashMeta.parent_id`** — 链接子会话到父会话，恢复时自动恢复所有子文件
- **Trash lifecycle integration test** — `trash_lifecycle_test.rs` 使用真实本地数据测试 4 个 provider 的完整 trash→restore→delete 流程
- **Agent "Open" button improvements** — 支持 `agentId` 匹配（Kimi）、正则回退提取截断 JSON 中的 description

### Fixed

- **Kimi 回收站恢复丢失子会话标题** — 不再在 trash 时删除 `subagents/` 目录（含 meta.json），仅在永久删除时清理
- **Kimi 树视图"乱跳"** — subagent HashMap 迭代顺序不确定 → 改为排序后迭代
- **Cursor store.db 永久删除时未清理** — 新增 `cleanup_on_permanent_delete` 钩子
- **Cursor `[REDACTED]`** — 从 assistant 文本中静默剥除
- **Cursor 子代理标题碰撞** — 120 字符前缀匹配 → 全文匹配
- **所有 provider 恢复子会话** — Codex 等平目录结构通过 `parent_id` 可靠关联
- **永久删除残留文件** — `cleanup_session_dir` 同时尝试 `with_extension("")`（Claude）和 `parent()`（Kimi/Cursor）；`is_session_dir()` 防误删共享目录
- **Gemini `chats/` 误删** — `remove_dir_all` 仅对包含 session 特征文件的目录执行
- **Claude/Codex/CC-Mirror 无子代理时残留目录** — `jsonl_subagents_deletion_plan` 改为检查 `session_dir.is_dir()` 而非仅 `subagents/`

### Changed

- Cursor CLI 数据源从 `store.db` (SQLite) 改为 `agent-transcripts/*.jsonl` (JSONL + FS events)
- Gemini 移除 `logs_parser.rs`、`orphan.rs`，精简为仅 chat 文件解析
- `lib.rs` 导出 `commands`、`db`、`indexer`、`trash_state` 模块以支持集成测试
- CLAUDE.md 全面更新：精简冗余内容、更新过时描述、移除隐私信息

## [0.3.0] - 2026-04-02

### Added

- **Provider Bridge architecture** — `ProviderDescriptor` trait 让每个 provider 在自己的模块中定义静态元数据（颜色、排序、路径匹配、resume 命令、SVG 图标），`Provider` enum 通过 `descriptor()` 零开销桥接
- **Provider trash strategy** — `TrashResult` 枚举 + `trash_session()` trait 方法，每个 provider 定义自己的回收站策略（移动文件 vs 软删除），替代集中式 if/else 分支
- **TypeScript provider registry** — `provider-registry.ts` 用 `Record<Provider, ProviderDef>` 替代所有 switch/if-else，包含 watch 策略、debounce、resume 命令、显示标签
- **Claude subagent sessions** — 扫描并解析 Claude 子代理会话，树形嵌套显示在父会话下
- **Codex subagent support** — 解析 Codex 子代理会话，支持导航和回收站级联
- **OpenCode subagent support** — 共享 DB 删除和子会话支持
- **Per-message model display** — 助手消息显示模型名称（model/version/branch）
- **Session metadata** — 提取并显示 model、cc_version、git_branch 信息
- **Persisted-output references** — 解析 `<persisted-output>` 引用在工具结果中
- **Tab keep-alive** — 标签页切换保持滚动位置，CSS display 切换替代卸载
- **Agent jump-to-subagent** — Agent 工具调用显示跳转按钮
- **Orphan subagent management** — `showOrphans` 开关，孤儿子代理显示 ⤷ 图标
- **Ctrl+Click folder select** — 文件夹节点 Ctrl+Click 选中所有子会话
- **Recent sessions improvements** — 过滤子代理，显示模型/时间/代理数
- **Compile-time Provider::all() safety** — 使用固定大小数组确保新增 variant 不会被遗漏
- **Provider unit tests** — Rust 4 个 provider 路径匹配/display key 测试 + TS 8 个 registry 测试

### Fixed

- **Cursor trash bug** — Cursor 会话现在正确进入回收站，而非永久删除
- **Gemini mixed storage** — `logs.json`（共享）软删除，chat JSON 文件物理移动，不再 trash 后复活
- **CC-Mirror variant sanitize** — resume 命令中 variant_name 现在经过 sanitize，防止 shell 注入
- **OpenCode delete transaction** — `delete_from_source` 使用 SQLite 事务包裹，防止中途失败导致数据不一致
- **delete_from_source error logging** — 删除失败不再静默吞没，改为 `log::warn!` 记录
- **Bash icon consistency** — HTML 导出中 Bash 图标与前端对齐（💻）
- **Subagent tree rendering** — 子代理始终可见，无需展开文件夹
- **Tree reveal** — 递归 DFS 搜索任意嵌套深度，包括子代理
- **Scroll position restore** — 切换标签后恢复滚动位置
- **Clear index** — 正确回收磁盘空间（WAL checkpoint + vacuum）

### Changed

- **Provider enum 精简** — 从 14 个方法减至 5 个（key/label/parse/all/descriptor），静态元数据移入各 provider 的 Descriptor
- **SessionProvider trait** — 仅保留实例行为（scan/load/watch/trash/delete），移除所有静态查询
- **trash.rs** — 纯调度层 + metadata 管理，零 provider 特判
- **sessions.rs / terminal.rs / indexer.rs** — 通过 descriptor 派发，零 provider 特判
- **icons.tsx** — switch 替换为 Record 查找
- **SessionView watch** — 通过 registry 驱动，替代 `.endsWith(".db")` 和 Gemini 硬编码延迟

### Removed

- **providers.ts** — 被 provider-registry.ts 替代
- **PROVIDER_PATH_PATTERNS** — 路径匹配移入各 provider descriptor
- **is_shared_file()** — 替换为 `descriptor().is_shared_file()`
- **delete_from_source_db()** — 替换为 provider trait 方法
- **Dead frontend exports** — `deleteSession`、`deleteSessionsBatch`、`getResumeCommand` 从 tauri.ts 移除

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
