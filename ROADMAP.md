# CC Session Roadmap

## Vision / 愿景

AI Coding Session 的个人知识库 — 统一入口，随时可查、可标注、可分享、随处可访问。
Your personal knowledge base for AI coding sessions — unified access, searchable, annotatable, shareable, available anywhere.

---

## Features / 功能

### Session Tags / Labels — 会话标签
- Manual tagging (e.g., `bug fix`, `feature`, `refactor`)
- Filter and group sessions by tags
- Search syntax extension: `tag:bugfix`
- 手动打标签，按标签过滤和分组，搜索语法扩展支持 `tag:bugfix`

### Message Bookmarks — 消息书签
- Bookmark individual messages within a session
- Quick access to bookmarked messages across all sessions
- 在会话内收藏单条消息，跨会话快速跳转到关键对话片段

### Session Annotations — 会话批注
- Add notes/comments on specific messages
- Works alongside message bookmarks for knowledge retention
- 在任意消息旁添加笔记，配合书签沉淀个人知识

### Raycast-style Global Search — 全局命令面板 `✅ done`
- `Cmd+K` command palette overlay
- Unified entry: search content, jump to sessions, execute actions (export, delete, theme toggle, etc.)
- Fuzzy matching with live preview
- Prefix routing: `>` commands, `#` tags, `@` provider, `:` jump to message
- Context-aware: default candidates change based on active panel (Explorer / Session / Usage)
- Search result pagination (current hard limit: 100)
- Reference: VS Code Command Palette, Arc Command Bar
- 全局 `Cmd+K` 唤起，模糊搜索 + 实时预览 + 前缀路由 + 上下文感知（不同面板下默认候选不同）

### Session Diff / Compare — 会话对比
- Side-by-side comparison of two sessions
- Compare different AI tools solving the same problem (e.g., Claude vs Codex on same bug)
- Highlight differences in approach, tool usage, and outcomes
- 并排对比两个 session，看不同工具/方案解决同一问题的差异

### Timeline Minimap — 时间线缩略图 `✅ done`
- Visual navigation bar on the right side of long conversations
- Color-coded message density (user / assistant / tool)
- Click to jump, similar to VS Code minimap
- 长对话右侧缩略导航条，按角色着色，点击跳转

### Smart Session Grouping — 智能会话分组
- Auto-group related sessions by task (same project + time window + similar content)
- e.g., a bug fix spanning 3 sessions automatically grouped together
- Leverage existing FTS5 data for similarity scoring
- 自动按任务聚合相关 session（同项目 + 同时段 + 相似内容）

### Session Templates — 会话模板
- Extract successful sessions as reusable prompt patterns
- Record prompt sequences for replay in new projects
- 从成功的 session 中提取可复用的 prompt 模式

### Provider Health Monitor — Provider 健康监控
- Visual status of each provider's file watch state
- Show which providers are healthy, which poll failed, which paths don't exist
- Currently this info is only in logs
- 可视化各 provider 的监控状态（正常/失败/路径不存在），当前仅在日志中

### Mobile Viewer — 移动端查看
- Local HTTP/WebSocket server on desktop, mobile connects via browser
- Browse sessions, search, view messages, favorite
- LAN direct connect + optional encrypted relay for remote access
- Reference: Paseo's Daemon + WebSocket architecture
- 桌面端启动本地 server，手机浏览器同局域网访问，支持浏览/搜索/收藏

### Preview / Commit Mode — 预览 / 确认打开 `✅ done`
- Single-click session → preview (lightweight load, italic tab, replaced on next click)
- Double-click session → open (full load, pinned tab)
- Browse many sessions quickly without tab accumulation
- Reference: VS Code preview editor behavior
- 单击预览（轻量加载，斜体 tab，再点别的就替换），双击正式打开（固定 tab）

### Split View — 分屏查看 `✅ done`
- Drag tab to the right to split editor area side-by-side
- View two sessions simultaneously (e.g., compare approaches)
- Also enables: session on left + related context on right
- Reference: VS Code editor groups, Arc Split View
- 拖拽 tab 到右侧即分屏，并排查看两个 session

### Workspaces — 工作区
- Named workspaces (e.g., "Project A", "Learning", "Debugging")
- Each workspace maintains its own tabs, filters, and favorites
- Switch workspace → entire context switches (tab bar, sidebar filter, bookmarks)
- Complements Smart Session Grouping: grouping is automatic, workspaces are manual curation
- Reference: Arc Spaces
- 命名工作区，每个工作区独立维护 tab/过滤器/收藏，切换时整体切换上下文

### Related Sessions — 关联会话推荐
- Show related sessions at the bottom of session detail view
- Sources: same project recent sessions, annotation cross-references, FTS5 content similarity
- Enable annotation references: "this approach was validated in session X message #42"
- Reference: Obsidian backlinks
- 会话详情底部展示关联 session（同项目/被批注引用/内容相似），类似 Obsidian 双向链接

### Configurable ActivityBar — ActivityBar 可配置
- Allow users to show/hide ActivityBar icons
- Keep core 4-5 items visible, others available via settings
- Prevent navigation bloat as features grow (5-7 top-level items is the UX limit)
- Reference: VS Code Activity Bar customization, Linear sidebar principles
- 支持显示/隐藏 ActivityBar 图标，核心项常驻，其余按需配置，防止导航膨胀

### Session Sharing — 会话分享
- One-click share link or self-contained HTML export
- Selective sharing: share partial messages, not entire session
- Auto-redact sensitive paths and usernames on export
- 一键分享链接，支持部分消息分享，导出自动脱敏

---

## Performance Optimization / 性能优化

### Watcher Debouncing — 文件监听去抖 `✅ done`
- Current: every JSONL chunk write triggers a separate reindex event (`watcher.rs:32`)
- Fix: batch changed paths and emit after 500ms window of no new changes
- Impact: ~50% reduction in redundant reindexing
- 当前每次文件写入立即触发 reindex，应合并为 500ms 窗口批量处理

### Incremental Sync with mtime — 基于 mtime 的增量同步 `⚫ low priority`
- Current: `reindex_filtered()` does full `scan_all()` every time (`indexer.rs:67-102`)
- Fix: track `last_indexed_mtime` per source file, skip unchanged files
- Impact: ~80% faster cold-check reindex
- 当前每次全量扫描，应记录文件修改时间，跳过未变化的文件

### Virtual Scrolling for Messages — 消息虚拟滚动 `🔧 partial`
- Current: all messages exist as DOM nodes, 500 messages = 500 nodes (`SessionView/index.tsx:51`)
- Session search mode bypasses lazy loading entirely (`SessionView/index.tsx:92`)
- Fix: keep only visible + 2-screen buffer in DOM
- Status: progressive loading (batch=80) exists, but not true virtual scroll; search mode still loads all
- 当前所有消息都是 DOM 节点，搜索模式更是全量渲染，应改为虚拟滚动

### Conditional Export Bundling — 导出按需打包 `✅ done`
- Current: KaTeX (264KB) + Mermaid (2.8MB) included in every HTML export (`templates.rs`)
- Fix: detect usage in session content, only include when needed
- 当前每个 HTML 导出都打包 3.1MB JS，应检测内容按需包含

### Lazy highlight.js Languages — 按需加载语法高亮 `🔧 partial`
- Current: 30+ languages registered at startup (`CodeBlock.tsx:36-75`)
- Fix: load language grammars on first encounter
- Status: static imports at module load, not truly lazy — needs dynamic import
- 当前启动时全量注册 30+ 语言，应按需加载

### Global Image Cache — 全局图片缓存 `✅ done`
- Current: same URL re-fetched per component instance (`ImagePreview.tsx`)
- Fix: shared cache keyed by URL, deduplicate across messages
- 同一 URL 图片在不同消息中重复加载，应全局缓存去重

---

## Database Optimization / 数据库优化

### FTS5 Tokenizer — 全文搜索分词器 `✅ done`
- Current: default tokenizer, suboptimal for code paths and identifiers (`db/mod.rs:89`)
- Fix: configure `unicode61` tokenizer for better code/path search
- 默认分词器对代码和路径搜索效果差，应配置 unicode61

### Search Pagination — 搜索分页
- Current: hard limit 100 results, no pagination UI
- Fix: add pagination or "load more" for search results
- 当前硬限 100 条无分页，应支持翻页或加载更多

---

## Code Quality / 代码质量

### UsagePanel Component Split — UsagePanel 组件拆分
- Current: 1323-line god component with charts + tables + sorting + maintenance
- Fix: split into `<UsageChart>`, `<ProjectCostTable>`, `<SessionCostTable>`, `<MaintenancePanel>`
- 1323 行巨型组件，应拆分为独立子组件

### Parser Error Surfacing — 解析错误用户可见 `🔧 partial`
- Current: malformed JSONL silently skipped with `log::warn` (`claude/parser.rs:112-118`)
- File read failures return `None` silently (`claude/parser.rs:66-70`)
- Fix: return `Result<ParsedSession, ParseError>`, surface errors as toast notifications
- Status: errors logged with log::warn but not surfaced to UI
- 解析失败和文件读取失败静默跳过，用户不知道 session 缺消息

### Explorer O(n^2) Lookup — Explorer 查找优化 `✅ done`
- Current: `findSessionProjectPath()` traverses full tree per selected session (`Explorer/index.tsx:242-270`)
- Fix: build session ID -> project path map during tree construction, O(1) lookup
- 每个选中 session 遍历整棵树，应在建树时构建映射表

---

## Technical Debt / 技术债

> Surfaced by a full TS + Rust audit. Priorities tied to CLAUDE.md's "No Silent Fallbacks" rule and the 800-line file limit.
> 由 TS + Rust 全量审查梳理，优先级对齐 CLAUDE.md 的 "No Silent Fallbacks" 和 800 行文件上限约定。

### Silent Fallback Cleanup — 静默兜底清理 `🔴 critical`
- Rust: `tool_metadata.rs:155-258` ~40+ `.unwrap_or_default()` chains; `indexer.rs:69,199-200` `.unwrap_or(0)` sort fallbacks; `pricing.rs:264-265` cost defaults to 0.0 masking missing pricing data
- TS: `UsagePanel.tsx:341,407,418` `?? 0`; `App/index.tsx:88-105` Tauri failure only logs to console, UI keeps stale data; 20+ `catch → console.warn` sites in `lib/tools.ts:166`, `CodeBlock.tsx:103,125`, `ImagePreview.tsx:35,257`
- Fix: propagate explicitly; `log::warn!` + skip on missing data; surface to toast at Tauri boundary
- 两侧系统性使用默认值掩盖数据缺失，违反 "No Silent Fallbacks" 铁律

### Production unwrap/expect — 生产代码 panic 风险 `🔴 critical` `✅ done`
- ~~`provider.rs:418`~~ `.find().expect()` on enum replaced with exhaustive match (compile-time enforcement)
- ~~`providers/claude/parser.rs:314`~~ unconditional `.unwrap()` replaced with `if let Some(parent_id) = parent_id.as_ref()` pattern
- ~~`pricing.rs:384-399`~~ dismissed on audit — all sites inside `#[cfg(test)]`
- ~~`db/sync.rs:453,497`~~ dismissed on audit — both sites inside `#[cfg(test)]`
- `db/queries.rs:156` `if let Ok(conn) = self.lock_read()` silent error also cleaned up in the same pass (logs each failure mode)
- 生产代码 unwrap/expect 完全消除

### Non-deterministic Lookups — 非确定性迭代查找 `🔴 critical` `✅ done`
- `providers/cc_mirror.rs:266-268` `.iter().find()` for variant lookup — audited and cleaned up
- Resolution: prefixes are disjoint by construction (unique FS dir under `~/.cc-mirror/`); precomputed `Variant.normalized_prefix` + invariant doc comment makes this explicit and eliminates per-call allocation
- Reviewer confirmed: `iter().find()` here is semantically deterministic (prefix-match on disjoint paths), not the CLAUDE.md-forbidden `HashMap::iter()` pattern
- 审计后确认为假阳性，但仍做了预计算清理和不变量注释

### Store Immutability Violations — Store 不可变性违规 `🔴 critical` `✅ done`
- `stores/editorGroups.ts:233,270,294,296` direct `.splice()` / `.push()` inside `setGroups` callbacks — all 4 sites converted to `slice + spread`
- Vitest coverage (34 tests in `editorGroups.test.ts`) verifies edge cases: negative indices, indices past length, undefined insertIndex
- 4 处数组突变全部改成不可变展开，测试覆盖确认无回归

### usage_hash Placeholder — usage_hash 占位 `🔴 critical`
- `models.rs:110` `Option<String>` with `#[serde(skip, default)]` — `None` silently disables cross-file usage dedup
- CLAUDE.md explicitly calls out `usage_hash: None` as an anti-pattern
- Fix: compute unconditionally, or `log::warn!` + skip dedup when source lacks required fields
- CLAUDE.md 点名的 antipattern

### Provider Parser Abstraction — Provider Parser 抽象 `🟡 high impact`
- 8 providers independently implement message aggregation, token stats, tool metadata, usage dedup
- ~5000+ lines of parallel code across codex/claude/kimi/cursor/qwen/copilot/gemini/cc_mirror parsers
- Adding a 9th provider means re-copying the whole pipeline
- Fix: extract a `ProviderParser` trait + shared helpers, start with tool metadata (most duplicated)
- 杠杆最大的一笔重构，8 个 provider 的 parser 平行演进，约 5000+ 行重复

### Large File Splits — 大文件拆分
- Rust: `providers/codex/parser.rs` 1652, `providers/claude/parser.rs` 1385, `providers/kimi/parser.rs` 944, `provider.rs` 907, `db/queries.rs` 848
- TS: `MarkdownRenderer.tsx` 833 (UsagePanel 1337 already tracked above under Code Quality)
- All exceed the 800-line limit set in CLAUDE.md
- Fix: most of the Rust parsers collapse naturally once Provider Parser Abstraction lands
- 超出 800 行上限的文件清单

### SessionView Effect Decoupling — SessionView Effect 解耦 `🟡 medium`
- `SessionView/index.tsx:151-397` has 6+ chained `createEffect` (favoriteVersion ↔ isFavorite ↔ setStarred ↔ bumpFavoriteVersion)
- Live-watch effect at 342-397 manages unwatch fn + debounce + listeners inline, leak risk
- Fix: extract `useLiveWatch`, `useFavoriteSync`, `useAutoLoad` hooks
- 多级联 effect 互相触发，存在循环 / 泄露隐患

### Tauri Error Wrapper — Tauri 错误统一包装 `🟡 medium`
- Every `invoke` caller does ad-hoc try/catch; many forget (`lib/tauri.ts` is raw invoke, no boundary)
- Backend returns `String` errors, losing anyhow chain
- Fix: `invokeWithToast()` wrapper on frontend; consistent `anyhow::Context` at backend command boundaries
- Tauri 边界错误处理不一致，anyhow 栈信息在序列化时丢失

### Core Module Test Coverage — 核心模块单测 `🟡 medium`
- `watcher.rs` (640), `indexer.rs` (639), `services/*`, `commands/*` have no `#[cfg(test)]` modules
- Only covered indirectly via integration tests in `tests/`
- Fix: add unit tests for watcher file-state transitions, indexer diff, lifecycle/resolution services
- 核心服务仅靠集成测试，边界场景易漏

### Clone Hot Path — Clone 热路径 `⚫ low priority`
- `providers/codex/parser.rs:49`, `tool_metadata.rs:11`, `providers/opencode/mod.rs:25` frequent `Message`/`Value` clones
- Fix: `Rc`/`Arc` only after profiling confirms a bottleneck
- 暂无性能数据支撑，profile 后再动

---

## Detail Improvements / 细节改进

### Batch Operation Failure Feedback — 批量操作失败反馈 `✅ done`
- Show per-item success/failure counts (e.g., "Trashed 8/10, 2 failed")
- 显示逐项成功/失败计数

### Tab Overflow Handling — Tab 溢出处理 `✅ done`
- Reference: VSCode tab bar (scroll + overflow menu)
- VSCode 风格 tab 滚动 + 溢出菜单

### i18n Completeness — i18n 完整性 `🔧 partial`
- Audit hardcoded strings and route through `t()`
- Status: mostly compliant, a few hardcoded strings remain (e.g., keyboard shortcuts)
- 审查硬编码字符串，全部走 `t()`

### Image Cache Persistence — 图片缓存持久化 `✅ done`
- Copy temp file images to `~/.cc-session/cache/images/{hash}.ext`
- Prevent image loss from OS temp cleanup
- 将临时目录图片缓存到持久路径，防止 OS 清理丢失

### Markdown Export: Usage Summary — Markdown 导出用量摘要 `✅ done`
- Add token usage and cost summary at the top of markdown exports
- 在 Markdown 导出顶部添加 token 用量和费用摘要

---

## Done / 已完成

### ~~Status Bar Enhancements~~
- Last scan time and today's total cost displayed in status bar

### ~~Search Debounce Tuning~~
- Debounce increased from 150ms to 300ms to reduce redundant queries

### ~~Redundant Array Allocation~~
- Removed spread+reverse in SessionView visibleEntries, using slice().reverse()

### ~~HashMap Pre-allocation~~
- Token stats computation uses pre-allocated HashMap capacity

### ~~Session Duration Display~~
- Show time span from first to last message (e.g., "23 min")

### ~~Trash Bulk Restore Confirmation~~
- Confirmation dialog with item count before bulk restore

### ~~Tool Call Header Hover Style~~
- Hover highlight on collapsible tool headers

### ~~Rename Title Length Limit~~
- Cap session title at 200 characters, frontend counter + backend truncation

### ~~Rename Dialog Auto-focus~~
- Already implemented (InputDialog.tsx auto-focus + select-all)
