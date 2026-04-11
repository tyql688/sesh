# CC Session Roadmap

## Features

### Session Tags / Labels
- Manual tagging (e.g., `bug fix`, `feature`, `refactor`)
- Filter and group sessions by tags

### Message Bookmarks
- Bookmark individual messages within a session
- Quick access to bookmarked messages across all sessions

### Session Annotations
- Add notes/comments on specific messages
- Works alongside message bookmarks

### Raycast-style Global Search
- `Cmd+K` command palette overlay
- Unified entry: search content, jump to sessions, execute actions (export, delete, theme toggle, etc.)
- Fuzzy matching with live preview
- Prefix routing: `>` commands, `#` tags, `@` provider
- Search result pagination (current hard limit: 100)

### Mobile Viewer
- Reference: Paseo's Daemon + WebSocket architecture
- Local HTTP/WebSocket server on desktop, mobile connects via browser or app
- LAN direct connect + optional encrypted relay for remote access

## Detail Improvements

### Batch Operation Failure Feedback
- Show per-item success/failure counts (e.g., "Trashed 8/10, 2 failed")
- Backend already has TODO at `sessions.rs:93`

### Tab Overflow Handling
- Reference: VSCode tab bar (scroll + overflow menu)

### i18n Completeness
- Audit hardcoded strings and route through `t()`

### Status Bar: Last Scan Time
- Show when the last index scan completed

### Status Bar: Today's Cost
- Display today's total spend (data already available via `get_usage_stats()`)

### Image Cache Persistence
- Copy temp file images (`/tmp/`, `/var/folders/`) to persistent cache on first load
- Prevent image loss from OS temp cleanup
- Cache path: `~/.cc-session/cache/images/{hash}.ext`

### Markdown Export: Usage Summary
- Add token usage and cost summary at the top of markdown exports

## Done

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
