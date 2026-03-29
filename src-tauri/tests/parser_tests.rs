use std::collections::HashMap;
use std::path::PathBuf;

use cc_session_lib::models::MessageRole;
use cc_session_lib::providers::claude::ClaudeProvider;
use cc_session_lib::providers::codex::CodexProvider;
use cc_session_lib::providers::kimi::KimiProvider;

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
