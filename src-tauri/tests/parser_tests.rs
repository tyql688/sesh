use std::collections::HashMap;
use std::path::PathBuf;

use cc_session_lib::models::MessageRole;
use cc_session_lib::provider::SessionProvider;
use cc_session_lib::providers::claude::ClaudeProvider;
use cc_session_lib::providers::codex::CodexProvider;
use cc_session_lib::providers::cursor::CursorProvider;
use cc_session_lib::providers::gemini::GeminiProvider;
use cc_session_lib::providers::kimi::KimiProvider;
use cc_session_lib::providers::opencode::OpenCodeProvider;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

// ---------------------------------------------------------------------------
// Claude parser tests
// ---------------------------------------------------------------------------

#[test]
fn claude_parses_message_count() {
    let provider = ClaudeProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("claude_session.jsonl");
    let session = provider
        .parse_session(&path)
        .expect("claude fixture must parse");

    // Expected messages:
    //  1. User: "Hello, can you help me debug..."
    //  2. System (thinking): "[thinking]\nLet me think..."
    //  3. Assistant: "Sure! I'd be happy..."
    //  4. User: "Here is my function..."
    //  5. Assistant: "I'll read the file..."
    //  6. Tool (Read): content = file contents (merged from tool_result)
    //  7. Assistant: "Your function looks correct!"
    assert_eq!(
        session.messages.len(),
        7,
        "expected 7 messages, got: {:#?}",
        session.messages
    );
}

#[test]
fn claude_first_user_message_role_and_content() {
    let provider = ClaudeProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("claude_session.jsonl");
    let session = provider
        .parse_session(&path)
        .expect("claude fixture must parse");

    let first = &session.messages[0];
    assert_eq!(first.role, MessageRole::User);
    assert!(
        first.content.contains("debug this Rust code"),
        "unexpected content: {}",
        first.content
    );
}

#[test]
fn claude_thinking_emitted_as_system_role() {
    let provider = ClaudeProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("claude_session.jsonl");
    let session = provider
        .parse_session(&path)
        .expect("claude fixture must parse");

    let thinking = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::System)
        .expect("expected a system (thinking) message");

    assert!(
        thinking.content.starts_with("[thinking]\n"),
        "thinking message must start with [thinking]\\n, got: {}",
        thinking.content
    );
    assert!(
        thinking.content.contains("Let me think"),
        "unexpected thinking content: {}",
        thinking.content
    );
}

#[test]
fn claude_tool_use_creates_tool_message() {
    let provider = ClaudeProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("claude_session.jsonl");
    let session = provider
        .parse_session(&path)
        .expect("claude fixture must parse");

    let tool_msg = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    assert_eq!(
        tool_msg.tool_name.as_deref(),
        Some("Read"),
        "expected tool_name 'Read', got: {:?}",
        tool_msg.tool_name
    );
    // tool_result should have been merged into the tool_use message
    assert!(
        tool_msg.content.contains("fn add"),
        "tool message content should include merged result, got: {}",
        tool_msg.content
    );
}

#[test]
fn claude_token_usage_attached_to_last_assistant_message() {
    let provider = ClaudeProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("claude_session.jsonl");
    let session = provider
        .parse_session(&path)
        .expect("claude fixture must parse");

    let last_assistant = session
        .messages
        .iter()
        .rfind(|m| m.role == MessageRole::Assistant)
        .expect("expected at least one assistant message");

    let usage = last_assistant
        .token_usage
        .as_ref()
        .expect("last assistant message must have token_usage");
    assert_eq!(usage.input_tokens, 300);
    assert_eq!(usage.output_tokens, 40);
    assert_eq!(usage.cache_read_input_tokens, 20);
}

#[test]
fn claude_project_path_extracted_from_cwd() {
    let provider = ClaudeProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("claude_session.jsonl");
    let session = provider
        .parse_session(&path)
        .expect("claude fixture must parse");

    assert_eq!(session.meta.project_path, "/home/user/my-project");
    assert_eq!(session.meta.project_name, "my-project");
}

#[test]
fn claude_session_title_from_first_user_message() {
    let provider = ClaudeProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("claude_session.jsonl");
    let session = provider
        .parse_session(&path)
        .expect("claude fixture must parse");

    assert!(
        session.meta.title.contains("debug this Rust code"),
        "title should derive from first user message, got: {}",
        session.meta.title
    );
}

// ---------------------------------------------------------------------------
// Codex parser tests
// ---------------------------------------------------------------------------

#[test]
fn codex_parses_message_count() {
    let provider = CodexProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("codex_session.jsonl");
    let session = provider
        .parse_session_file(&path)
        .expect("codex fixture must parse");

    // Expected messages:
    //  1. User: "Write a hello world program"
    //  2. Assistant: "I'll create a hello world program..."  (token_usage attached)
    //  3. Tool (Bash): exec_command, content = merged output
    //  4. Assistant: "The hello world program is ready..." (token_usage attached)
    assert_eq!(
        session.messages.len(),
        4,
        "expected 4 messages, got: {:#?}",
        session.messages
    );
}

#[test]
fn codex_session_id_from_meta() {
    let provider = CodexProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("codex_session.jsonl");
    let session = provider
        .parse_session_file(&path)
        .expect("codex fixture must parse");

    assert_eq!(session.meta.id, "codex-session-abc123");
}

#[test]
fn codex_exec_command_mapped_to_bash() {
    let provider = CodexProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("codex_session.jsonl");
    let session = provider
        .parse_session_file(&path)
        .expect("codex fixture must parse");

    let tool_msg = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    assert_eq!(
        tool_msg.tool_name.as_deref(),
        Some("Bash"),
        "exec_command must map to Bash, got: {:?}",
        tool_msg.tool_name
    );
}

#[test]
fn codex_exec_command_args_remapped_to_command_key() {
    let provider = CodexProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("codex_session.jsonl");
    let session = provider
        .parse_session_file(&path)
        .expect("codex fixture must parse");

    let tool_msg = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    let input = tool_msg
        .tool_input
        .as_ref()
        .expect("Bash tool must have tool_input");
    // exec_command {"cmd":"..."} must be remapped to {"command":"..."}
    assert!(
        input.contains("\"command\""),
        "tool_input must use 'command' key, got: {}",
        input
    );
    assert!(
        input.contains("cat hello.py"),
        "tool_input must contain the command, got: {}",
        input
    );
}

#[test]
fn codex_function_call_output_merged_into_tool_message() {
    let provider = CodexProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("codex_session.jsonl");
    let session = provider
        .parse_session_file(&path)
        .expect("codex fixture must parse");

    let tool_msg = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    assert!(
        tool_msg.content.contains("Hello, World!"),
        "tool output must be merged into tool message, got: {}",
        tool_msg.content
    );
}

#[test]
fn codex_token_usage_attached_to_assistant_message() {
    let provider = CodexProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("codex_session.jsonl");
    let session = provider
        .parse_session_file(&path)
        .expect("codex fixture must parse");

    let last_assistant = session
        .messages
        .iter()
        .rfind(|m| m.role == MessageRole::Assistant)
        .expect("expected at least one assistant message");

    let usage = last_assistant
        .token_usage
        .as_ref()
        .expect("last assistant message must have token_usage");
    assert_eq!(usage.input_tokens, 120);
    assert_eq!(usage.output_tokens, 25);
    assert_eq!(usage.cache_read_input_tokens, 10);
}

#[test]
fn codex_project_path_from_session_meta() {
    let provider = CodexProvider::new().expect("home dir must be available");
    let path = fixtures_dir().join("codex_session.jsonl");
    let session = provider
        .parse_session_file(&path)
        .expect("codex fixture must parse");

    assert_eq!(session.meta.project_path, "/home/user/my-project");
}

// ---------------------------------------------------------------------------
// Kimi parser tests
// ---------------------------------------------------------------------------

#[test]
fn kimi_parses_message_count() {
    let provider = KimiProvider::new().expect("home dir must be available");
    let path = fixtures_dir()
        .join("kimi")
        .join("abc123def456")
        .join("session-uuid-0001")
        .join("wire.jsonl");
    let project_map = HashMap::new();
    let session = provider
        .parse_session_file(&path, &project_map)
        .expect("kimi fixture must parse");

    // Expected messages:
    //  1. User: "List files in the current directory"
    //  2. System (thinking): "[thinking]\nThe user wants to list files..."
    //  3. Tool (Bash): Shell call, content = merged output
    //  4. Assistant: "Here are the files..." (token_usage attached)
    assert_eq!(
        session.messages.len(),
        4,
        "expected 4 messages, got: {:#?}",
        session.messages
    );
}

#[test]
fn kimi_user_message_role_and_content() {
    let provider = KimiProvider::new().expect("home dir must be available");
    let path = fixtures_dir()
        .join("kimi")
        .join("abc123def456")
        .join("session-uuid-0001")
        .join("wire.jsonl");
    let project_map = HashMap::new();
    let session = provider
        .parse_session_file(&path, &project_map)
        .expect("kimi fixture must parse");

    let first = &session.messages[0];
    assert_eq!(first.role, MessageRole::User);
    assert!(
        first.content.contains("List files"),
        "unexpected content: {}",
        first.content
    );
}

#[test]
fn kimi_thinking_emitted_as_system_role() {
    let provider = KimiProvider::new().expect("home dir must be available");
    let path = fixtures_dir()
        .join("kimi")
        .join("abc123def456")
        .join("session-uuid-0001")
        .join("wire.jsonl");
    let project_map = HashMap::new();
    let session = provider
        .parse_session_file(&path, &project_map)
        .expect("kimi fixture must parse");

    let thinking = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::System)
        .expect("expected a thinking (System) message");

    assert!(
        thinking.content.starts_with("[thinking]\n"),
        "thinking message must start with [thinking]\\n, got: {}",
        thinking.content
    );
}

#[test]
fn kimi_shell_tool_mapped_to_bash() {
    let provider = KimiProvider::new().expect("home dir must be available");
    let path = fixtures_dir()
        .join("kimi")
        .join("abc123def456")
        .join("session-uuid-0001")
        .join("wire.jsonl");
    let project_map = HashMap::new();
    let session = provider
        .parse_session_file(&path, &project_map)
        .expect("kimi fixture must parse");

    let tool_msg = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    assert_eq!(
        tool_msg.tool_name.as_deref(),
        Some("Bash"),
        "Shell must map to Bash, got: {:?}",
        tool_msg.tool_name
    );
}

#[test]
fn kimi_tool_result_merged_into_tool_call() {
    let provider = KimiProvider::new().expect("home dir must be available");
    let path = fixtures_dir()
        .join("kimi")
        .join("abc123def456")
        .join("session-uuid-0001")
        .join("wire.jsonl");
    let project_map = HashMap::new();
    let session = provider
        .parse_session_file(&path, &project_map)
        .expect("kimi fixture must parse");

    let tool_msg = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    assert!(
        tool_msg.content.contains("main.rs"),
        "tool result must be merged, got: {}",
        tool_msg.content
    );
}

#[test]
fn kimi_token_usage_from_status_update() {
    let provider = KimiProvider::new().expect("home dir must be available");
    let path = fixtures_dir()
        .join("kimi")
        .join("abc123def456")
        .join("session-uuid-0001")
        .join("wire.jsonl");
    let project_map = HashMap::new();
    let session = provider
        .parse_session_file(&path, &project_map)
        .expect("kimi fixture must parse");

    // StatusUpdate attaches to last assistant or tool message
    let last_with_usage = session
        .messages
        .iter()
        .rev()
        .find(|m| m.token_usage.is_some())
        .expect("expected at least one message with token_usage");

    let usage = last_with_usage.token_usage.as_ref().unwrap();
    // input_tokens = input_other(80) + input_cache_read(10) + input_cache_creation(5) = 95
    assert_eq!(usage.input_tokens, 95);
    assert_eq!(usage.output_tokens, 35);
    assert_eq!(usage.cache_read_input_tokens, 10);
    assert_eq!(usage.cache_creation_input_tokens, 5);
}

#[test]
fn kimi_session_id_from_parent_directory() {
    let provider = KimiProvider::new().expect("home dir must be available");
    let path = fixtures_dir()
        .join("kimi")
        .join("abc123def456")
        .join("session-uuid-0001")
        .join("wire.jsonl");
    let project_map = HashMap::new();
    let session = provider
        .parse_session_file(&path, &project_map)
        .expect("kimi fixture must parse");

    // Session ID = parent directory name (the session UUID dir)
    assert_eq!(session.meta.id, "session-uuid-0001");
}

// ---------------------------------------------------------------------------
// Gemini chat parser tests
// ---------------------------------------------------------------------------

fn gemini_fixture_path() -> PathBuf {
    fixtures_dir().join("gemini").join("session-test.json")
}

fn gemini_parsed_session() -> cc_session_lib::provider::ParsedSession {
    let provider = GeminiProvider::new().expect("home dir must be available");
    let path = gemini_fixture_path();
    let project_map = HashMap::new();
    provider
        .parse_chat_file_for_test(&path, &project_map)
        .expect("gemini fixture must parse")
}

#[test]
fn gemini_parses_message_count() {
    let session = gemini_parsed_session();

    // Expected messages:
    //  1. User: "List the files in the current directory"
    //  2. Assistant: "I'll run a shell command to list the files for you."
    //  3. Tool (Bash): Shell call with merged result (token_usage attached here)
    //  4. User: "Thanks, that looks good!"
    assert_eq!(
        session.messages.len(),
        4,
        "expected 4 messages, got: {:#?}",
        session.messages
    );
}

#[test]
fn gemini_first_user_message() {
    let session = gemini_parsed_session();

    let first = &session.messages[0];
    assert_eq!(first.role, MessageRole::User);
    assert!(
        first.content.contains("List the files"),
        "unexpected content: {}",
        first.content
    );
}

#[test]
fn gemini_tool_call_parsed() {
    let session = gemini_parsed_session();

    let tool_msg = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    // Shell displayName maps to canonical "Bash"
    assert_eq!(
        tool_msg.tool_name.as_deref(),
        Some("Bash"),
        "Shell must map to Bash, got: {:?}",
        tool_msg.tool_name
    );

    // tool_input must contain the command key (Bash remapping)
    let input = tool_msg
        .tool_input
        .as_ref()
        .expect("Bash tool must have tool_input");
    assert!(
        input.contains("\"command\""),
        "tool_input must use 'command' key, got: {}",
        input
    );
    assert!(
        input.contains("ls -la"),
        "tool_input must contain the shell command, got: {}",
        input
    );

    // Tool result must be merged into content
    assert!(
        tool_msg.content.contains("main.rs"),
        "tool result must be merged into content, got: {}",
        tool_msg.content
    );
}

#[test]
fn gemini_token_usage() {
    let session = gemini_parsed_session();

    // Token usage is on the model message's last tool call (the Bash tool message)
    let tool_msg = session
        .messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    let usage = tool_msg
        .token_usage
        .as_ref()
        .expect("last tool message must carry token_usage from the model turn");
    assert_eq!(usage.input_tokens, 150);
    assert_eq!(usage.output_tokens, 45);
    assert_eq!(usage.cache_read_input_tokens, 20);
}

// ---------------------------------------------------------------------------
// Cursor CLI parser tests
// ---------------------------------------------------------------------------

/// Creates a temporary SQLite database that mimics a Cursor CLI `store.db`.
///
/// Returns the path to the `store.db` file inside a UUID-named subdirectory
/// so that `parse_session_db` can derive a session ID from the parent dir.
///
/// Row layout (ordered by rowid):
///  1. user   – `<user_query>Hello</user_query>` wrapped in content string
///  2. assistant – plain text reply
///  3. assistant – tool-call array stored as JSON-array-as-string (Cursor real format)
///  4. tool   – tool-result array stored as JSON-array-as-string
fn create_cursor_test_db() -> (tempfile::TempDir, std::path::PathBuf) {
    use rusqlite::Connection;

    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let session_dir = tmp.path().join("cursor-session-uuid-0001");
    std::fs::create_dir_all(&session_dir).expect("failed to create temp cursor session dir");

    let db_path = session_dir.join("store.db");
    let conn = Connection::open(&db_path).expect("failed to create cursor test DB");

    conn.execute_batch("CREATE TABLE blobs (data BLOB NOT NULL);")
        .expect("failed to create blobs table");

    // Row 1: user message with <user_query> tags (Cursor real format)
    let user_blob = r#"{"role":"user","content":"<user_info>\nWorkspace Path: /home/user/cursor-project\n</user_info>\n<user_query>\nHello, can you help me list files?\n</user_query>"}"#;

    // Row 2: assistant plain text reply
    let assistant_blob = r#"{"role":"assistant","content":"Sure! I can run a shell command to list the files for you."}"#;

    // Row 3: assistant with tool-call — content is a JSON-array serialised as a string
    // (This matches real Cursor CLI format: "content" is a string that starts with '[')
    let tool_call_content = r#"[{"type":"tool-call","toolName":"Shell","toolCallId":"c1","args":{"command":"ls -la"}}]"#;
    let assistant_tool_blob = format!(
        r#"{{"role":"assistant","content":{}}}"#,
        serde_json::to_string(tool_call_content).unwrap()
    );

    // Row 4: tool result — content is a JSON-array serialised as a string
    let tool_result_content = r#"[{"type":"tool-result","toolCallId":"c1","toolName":"Shell","result":"file.txt\nREADME.md\nmain.rs"}]"#;
    let tool_blob = format!(
        r#"{{"role":"tool","content":{}}}"#,
        serde_json::to_string(tool_result_content).unwrap()
    );

    for blob in &[
        user_blob.to_string(),
        assistant_blob.to_string(),
        assistant_tool_blob,
        tool_blob,
    ] {
        conn.execute(
            "INSERT INTO blobs (data) VALUES (CAST(? AS BLOB))",
            rusqlite::params![blob.as_bytes()],
        )
        .expect("failed to insert cursor test row");
    }

    // Return TempDir to keep it alive; dropped at end of test = cleanup
    (tmp, db_path)
}

#[test]
fn cursor_parses_message_count() {
    let provider = CursorProvider::new().expect("home dir must be available");
    let (_tmp, db_path) = create_cursor_test_db();

    // Expected messages from load_messages:
    //  1. User: "Hello, can you help me list files?"  (user_query tag stripped)
    //  2. Assistant: "Sure! I can run a shell command..."
    //  3. Tool (Bash): Shell → Bash, content = merged result "file.txt\nREADME.md\nmain.rs"
    let messages = provider
        .load_messages("cursor-session-uuid-0001", db_path.to_str().unwrap())
        .expect("cursor test DB must load messages");

    assert_eq!(
        messages.len(),
        3,
        "expected 3 messages, got: {:#?}",
        messages
    );
}

#[test]
fn cursor_user_message_extracted() {
    let provider = CursorProvider::new().expect("home dir must be available");
    let (_tmp, db_path) = create_cursor_test_db();

    let messages = provider
        .load_messages("cursor-session-uuid-0001", db_path.to_str().unwrap())
        .expect("cursor test DB must load messages");

    let user_msg = messages
        .iter()
        .find(|m| m.role == MessageRole::User)
        .expect("expected a User message");

    // <user_query> tags must be stripped, <user_info> block must be discarded
    assert!(
        user_msg.content.contains("Hello"),
        "user content should contain the query text, got: {}",
        user_msg.content
    );
    assert!(
        !user_msg.content.contains("<user_query>"),
        "user_query tags must be stripped, got: {}",
        user_msg.content
    );
    assert!(
        !user_msg.content.contains("<user_info>"),
        "user_info block must be discarded, got: {}",
        user_msg.content
    );
}

#[test]
fn cursor_tool_call_merged() {
    let provider = CursorProvider::new().expect("home dir must be available");
    let (_tmp, db_path) = create_cursor_test_db();

    let messages = provider
        .load_messages("cursor-session-uuid-0001", db_path.to_str().unwrap())
        .expect("cursor test DB must load messages");

    let tool_msg = messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    // Cursor "Shell" must map to canonical "Bash"
    assert_eq!(
        tool_msg.tool_name.as_deref(),
        Some("Bash"),
        "Shell must map to Bash, got: {:?}",
        tool_msg.tool_name
    );

    // tool_input must be remapped to {"command": "ls -la"}
    let input = tool_msg
        .tool_input
        .as_ref()
        .expect("Bash tool must have tool_input");
    assert!(
        input.contains("\"command\""),
        "tool_input must use 'command' key, got: {}",
        input
    );
    assert!(
        input.contains("ls -la"),
        "tool_input must contain the shell command, got: {}",
        input
    );

    // Tool result must be merged into the same message via toolCallId
    assert!(
        tool_msg.content.contains("main.rs"),
        "tool result must be merged into tool message content, got: {}",
        tool_msg.content
    );
}

// ---------------------------------------------------------------------------
// OpenCode parser tests
// ---------------------------------------------------------------------------

/// Create a temporary SQLite database matching the OpenCode schema.
/// Returns the `TempDir` (must be kept alive for the test) and the DB path.
fn create_opencode_test_db() -> (tempfile::TempDir, PathBuf) {
    use rusqlite::{params, Connection};

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("opencode.db");

    let conn = Connection::open(&db_path).expect("open db");

    conn.execute_batch(
        "CREATE TABLE project (
            id           TEXT    PRIMARY KEY,
            name         TEXT    NOT NULL,
            worktree     TEXT    NOT NULL,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL
         );
         CREATE TABLE session (
            id           TEXT    PRIMARY KEY,
            title        TEXT    NOT NULL,
            directory    TEXT    NOT NULL,
            project_id   TEXT,
            parent_id    TEXT,
            time_created INTEGER NOT NULL,
            time_updated INTEGER NOT NULL
         );
         CREATE TABLE message (
            id           TEXT    PRIMARY KEY,
            session_id   TEXT    NOT NULL,
            data         TEXT    NOT NULL,
            time_created INTEGER NOT NULL
         );
         CREATE TABLE part (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id   TEXT    NOT NULL,
            session_id   TEXT    NOT NULL,
            data         TEXT    NOT NULL,
            time_created INTEGER NOT NULL
         );",
    )
    .expect("create tables");

    // project
    conn.execute(
        "INSERT INTO project (id, name, worktree, time_created, time_updated)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            "proj-001",
            "my-opencode-project",
            "/home/user/my-opencode-project",
            1705321200000_i64,
            1705321200000_i64,
        ],
    )
    .expect("insert project");

    // session
    // time_created = 1705321200000 ms → epoch s = 1705321200
    // time_updated = 1705324800000 ms → epoch s = 1705324800
    conn.execute(
        "INSERT INTO session
             (id, title, directory, project_id, parent_id, time_created, time_updated)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            "session-oc-0001",
            "Test OpenCode Session",
            "/home/user/my-opencode-project",
            "proj-001",
            Option::<String>::None,
            1705321200000_i64,
            1705324800000_i64,
        ],
    )
    .expect("insert session");

    // message 1 — user
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        params![
            "msg-001",
            "session-oc-0001",
            r#"{"role":"user","time":{"created":1705321200000}}"#,
            1705321200000_i64,
        ],
    )
    .expect("insert msg-001");

    // message 2 — assistant (text + tool part, carries token usage)
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        params![
            "msg-002",
            "session-oc-0001",
            r#"{"role":"assistant","time":{"created":1705321260000},"tokens":{"input":50,"output":100,"cache":{"read":10,"write":5}}}"#,
            1705321260000_i64,
        ],
    )
    .expect("insert msg-002");

    // message 3 — second user
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        params![
            "msg-003",
            "session-oc-0001",
            r#"{"role":"user","time":{"created":1705321320000}}"#,
            1705321320000_i64,
        ],
    )
    .expect("insert msg-003");

    // part — user text (msg-001)
    conn.execute(
        "INSERT INTO part (message_id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        params![
            "msg-001",
            "session-oc-0001",
            r#"{"type":"text","text":"List files in the project directory"}"#,
            1705321200000_i64,
        ],
    )
    .expect("insert part user text");

    // part — assistant text (msg-002)
    conn.execute(
        "INSERT INTO part (message_id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        params![
            "msg-002",
            "session-oc-0001",
            r#"{"type":"text","text":"Sure, let me list the files for you."}"#,
            1705321260000_i64,
        ],
    )
    .expect("insert part assistant text");

    // part — tool call (msg-002)
    conn.execute(
        "INSERT INTO part (message_id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        params![
            "msg-002",
            "session-oc-0001",
            r#"{"type":"tool","tool":"Bash","state":{"status":"completed","input":"{\"command\":\"ls -la\"}","output":"total 8\ndrwxr-xr-x main.rs\ndrwxr-xr-x lib.rs","time":{"start":1705321265000}}}"#,
            1705321265000_i64,
        ],
    )
    .expect("insert part tool");

    // part — user text (msg-003)
    conn.execute(
        "INSERT INTO part (message_id, session_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
        params![
            "msg-003",
            "session-oc-0001",
            r#"{"type":"text","text":"Thanks, that looks good!"}"#,
            1705321320000_i64,
        ],
    )
    .expect("insert part user text 2");

    (dir, db_path)
}

#[test]
fn opencode_parses_session_meta() {
    let (_dir, db_path) = create_opencode_test_db();
    let provider = OpenCodeProvider::with_db_path(db_path);

    let sessions = provider.scan_all().expect("scan_all must succeed");
    assert_eq!(sessions.len(), 1, "expected exactly 1 session");

    let meta = &sessions[0].meta;
    assert_eq!(meta.id, "session-oc-0001");
    assert_eq!(meta.title, "Test OpenCode Session");
    assert_eq!(meta.project_path, "/home/user/my-opencode-project");
    // time_created ms → epoch seconds
    assert_eq!(meta.created_at, 1705321200);
    // time_updated ms → epoch seconds
    assert_eq!(meta.updated_at, 1705324800);
}

#[test]
fn opencode_parses_message_count() {
    let (_dir, db_path) = create_opencode_test_db();
    let provider = OpenCodeProvider::with_db_path(db_path.clone());

    let sessions = provider.scan_all().expect("scan_all must succeed");
    // 3 rows exist in the message table
    assert_eq!(
        sessions[0].meta.message_count, 3,
        "expected 3 DB message rows in meta"
    );

    // load_messages expands them into parsed Message structs:
    //  1. User: "List files..."
    //  2. Assistant: "Sure, let me list..."
    //  3. Tool (Bash): ls output
    //  4. User: "Thanks..."
    let messages = provider
        .load_messages("session-oc-0001", &db_path.to_string_lossy())
        .expect("load_messages must succeed");

    assert_eq!(
        messages.len(),
        4,
        "expected 4 parsed messages, got: {:#?}",
        messages
    );
}

#[test]
fn opencode_tool_message_parsed() {
    let (_dir, db_path) = create_opencode_test_db();
    let provider = OpenCodeProvider::with_db_path(db_path.clone());

    let messages = provider
        .load_messages("session-oc-0001", &db_path.to_string_lossy())
        .expect("load_messages must succeed");

    let tool_msg = messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    assert_eq!(
        tool_msg.tool_name.as_deref(),
        Some("Bash"),
        "tool_name must be 'Bash', got: {:?}",
        tool_msg.tool_name
    );

    let input = tool_msg
        .tool_input
        .as_ref()
        .expect("tool message must have tool_input");
    assert!(
        input.contains("ls -la"),
        "tool_input must contain the command, got: {}",
        input
    );
    assert!(
        tool_msg.content.contains("main.rs"),
        "tool output must contain ls result, got: {}",
        tool_msg.content
    );
}

#[test]
fn opencode_token_usage() {
    // Build a minimal DB where the assistant message has ONLY a tool part
    // (no text). In that case the provider attaches token_usage to the last
    // tool message, which is the only way to observe it in this code path.
    use rusqlite::{params, Connection};

    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("opencode.db");
    let conn = Connection::open(&db_path).expect("open db");

    conn.execute_batch(
        "CREATE TABLE project (
             id TEXT PRIMARY KEY, name TEXT NOT NULL, worktree TEXT NOT NULL,
             time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL
         );
         CREATE TABLE session (
             id TEXT PRIMARY KEY, title TEXT NOT NULL, directory TEXT NOT NULL,
             project_id TEXT, parent_id TEXT,
             time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL
         );
         CREATE TABLE message (
             id TEXT PRIMARY KEY, session_id TEXT NOT NULL,
             data TEXT NOT NULL, time_created INTEGER NOT NULL
         );
         CREATE TABLE part (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             message_id TEXT NOT NULL, session_id TEXT NOT NULL,
             data TEXT NOT NULL, time_created INTEGER NOT NULL
         );",
    )
    .expect("create tables");

    conn.execute(
        "INSERT INTO project (id, name, worktree, time_created, time_updated)
         VALUES ('p1','proj','/proj',1705321200000,1705321200000)",
        [],
    )
    .expect("insert project");
    conn.execute(
        "INSERT INTO session (id, title, directory, project_id, parent_id, time_created, time_updated)
         VALUES ('s1','Token Test','/proj','p1',NULL,1705321200000,1705321200000)",
        [],
    )
    .expect("insert session");

    // user message
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES (?1,?2,?3,?4)",
        params![
            "m1",
            "s1",
            r#"{"role":"user","time":{"created":1705321200000}}"#,
            1705321200000_i64
        ],
    )
    .expect("insert user msg");

    // assistant message — carries token usage, has NO text part
    conn.execute(
        "INSERT INTO message (id, session_id, data, time_created) VALUES (?1,?2,?3,?4)",
        params![
            "m2", "s1",
            r#"{"role":"assistant","time":{"created":1705321260000},"tokens":{"input":50,"output":100,"cache":{"read":10,"write":5}}}"#,
            1705321260000_i64
        ],
    )
    .expect("insert assistant msg");

    // user text part
    conn.execute(
        "INSERT INTO part (message_id, session_id, data, time_created) VALUES (?1,?2,?3,?4)",
        params![
            "m1",
            "s1",
            r#"{"type":"text","text":"Hello"}"#,
            1705321200000_i64
        ],
    )
    .expect("insert user part");

    // assistant has only a tool part (no text part → token_usage goes onto tool msg)
    conn.execute(
        "INSERT INTO part (message_id, session_id, data, time_created) VALUES (?1,?2,?3,?4)",
        params![
            "m2", "s1",
            r#"{"type":"tool","tool":"Bash","state":{"status":"completed","input":"{\"command\":\"echo hi\"}","output":"hi"}}"#,
            1705321260000_i64
        ],
    )
    .expect("insert tool part");

    drop(conn);

    let provider = OpenCodeProvider::with_db_path(db_path.clone());
    let messages = provider
        .load_messages("s1", &db_path.to_string_lossy())
        .expect("load_messages must succeed");

    // The assistant turn has no text parts, so token_usage lands on the tool message.
    let tool_msg = messages
        .iter()
        .find(|m| m.role == MessageRole::Tool)
        .expect("expected a Tool message");

    let usage = tool_msg
        .token_usage
        .as_ref()
        .expect("tool message must carry token_usage when assistant has no text parts");
    assert_eq!(usage.input_tokens, 50);
    assert_eq!(usage.output_tokens, 100);
    assert_eq!(usage.cache_read_input_tokens, 10);
    assert_eq!(usage.cache_creation_input_tokens, 5);
}
