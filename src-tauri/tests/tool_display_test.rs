//! Golden tests for tool display rendering in the HTML exporter.
//! Fixtures shared with frontend vitest tests.

use serde::Deserialize;
use serde_json::json;

use cc_session_lib::models::{
    Message, MessageRole, Provider, SessionDetail, SessionMeta, ToolMetadata,
};

#[derive(Deserialize)]
struct GoldenCase {
    tool_name: String,
    tool_input: String,
    expected_keywords: Vec<String>,
}

#[test]
fn test_render_tool_detail_golden() {
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tool_display/golden.json");
    let data = std::fs::read_to_string(&fixture_path)
        .unwrap_or_else(|e| panic!("failed to read fixture: {e}"));
    let cases: Vec<GoldenCase> =
        serde_json::from_str(&data).unwrap_or_else(|e| panic!("failed to parse fixture: {e}"));

    for case in &cases {
        let html = cc_session_lib::exporter_test_helpers::render_tool_detail_pub(
            &case.tool_name,
            &case.tool_input,
        );
        for keyword in &case.expected_keywords {
            assert!(
                html.contains(keyword) || html.contains(&html_escape(keyword)),
                "tool={}: expected keyword '{}' not found in output:\n{}",
                case.tool_name,
                keyword,
                html
            );
        }
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn test_session(messages: Vec<Message>) -> SessionDetail {
    SessionDetail {
        meta: SessionMeta {
            id: "tool-html-test".to_string(),
            provider: Provider::Claude,
            title: "Tool HTML Test".to_string(),
            project_path: "/tmp/project".to_string(),
            project_name: "project".to_string(),
            created_at: 1_766_000_000,
            updated_at: 1_766_000_000,
            message_count: messages.len() as u32,
            file_size_bytes: 1,
            source_path: "/tmp/session.jsonl".to_string(),
            is_sidechain: false,
            variant_name: None,
            model: None,
            cc_version: None,
            git_branch: None,
            parent_id: None,
        },
        messages,
    }
}

fn tool_message(name: &str, input: Option<String>, metadata: Option<ToolMetadata>) -> Message {
    tool_message_with_content(
        name,
        "raw output that should be hidden for structured diffs",
        input,
        metadata,
    )
}

fn tool_message_with_content(
    name: &str,
    content: &str,
    input: Option<String>,
    metadata: Option<ToolMetadata>,
) -> Message {
    Message {
        role: MessageRole::Tool,
        content: content.to_string(),
        timestamp: None,
        tool_name: Some(name.to_string()),
        tool_input: input,
        tool_metadata: metadata,
        token_usage: None,
        model: None,
        usage_hash: None,
    }
}

#[test]
fn test_render_session_html_uses_tool_metadata() {
    let detail = test_session(vec![
        tool_message(
            "Edit",
            Some(
                json!({
                    "file_path": "/tmp/project/src/app.py",
                    "old_string": "old",
                    "new_string": "new"
                })
                .to_string(),
            ),
            Some(ToolMetadata {
                raw_name: "Edit".to_string(),
                canonical_name: "Edit".to_string(),
                display_name: "Edit".to_string(),
                category: "file".to_string(),
                summary: Some("src/app.py".to_string()),
                status: Some("success".to_string()),
                ids: Default::default(),
                mcp: None,
                result_kind: Some("file_patch".to_string()),
                structured: Some(json!({
                    "filePath": "/tmp/project/src/app.py",
                    "oldString": "old",
                    "newString": "new"
                })),
            }),
        ),
        tool_message(
            "mcp__server__browser_snapshot",
            Some(json!({}).to_string()),
            Some(ToolMetadata {
                raw_name: "mcp__server__browser_snapshot".to_string(),
                canonical_name: "mcp__server__browser_snapshot".to_string(),
                display_name: "browser snapshot".to_string(),
                category: "mcp".to_string(),
                summary: Some("page snapshot".to_string()),
                status: Some("success".to_string()),
                ids: Default::default(),
                mcp: Some(cc_session_lib::models::McpToolMetadata {
                    server: "server".to_string(),
                    tool: "browser_snapshot".to_string(),
                    display: "browser snapshot".to_string(),
                }),
                result_kind: Some("mcp".to_string()),
                structured: Some(json!({"list":[{"type":"text","text":"snapshot"}]})),
            }),
        ),
        tool_message(
            "Edit",
            None,
            Some(ToolMetadata {
                raw_name: "Edit".to_string(),
                canonical_name: "Edit".to_string(),
                display_name: "Edit".to_string(),
                category: "file".to_string(),
                summary: Some("src/patch.rs".to_string()),
                status: Some("success".to_string()),
                ids: Default::default(),
                mcp: None,
                result_kind: Some("file_patch".to_string()),
                structured: Some(json!({
                    "filePath": "/tmp/project/src/patch.rs",
                    "structuredPatch": [{
                        "oldStart": 7,
                        "oldLines": 2,
                        "newStart": 7,
                        "newLines": 2,
                        "lines": [" context", "-old", "+new"]
                    }]
                })),
            }),
        ),
        tool_message_with_content(
            "TaskUpdate",
            "task status raw output",
            None,
            Some(ToolMetadata {
                raw_name: "TaskUpdate".to_string(),
                canonical_name: "TaskUpdate".to_string(),
                display_name: "TaskUpdate".to_string(),
                category: "task".to_string(),
                summary: Some("11 → completed".to_string()),
                status: Some("success".to_string()),
                ids: Default::default(),
                mcp: None,
                result_kind: Some("task_status".to_string()),
                structured: Some(json!({
                    "taskId": "11",
                    "statusChange": {
                        "from": "in_progress",
                        "to": "completed"
                    }
                })),
            }),
        ),
    ]);

    let html = cc_session_lib::exporter_test_helpers::render_session_html_pub(&detail);
    assert!(html.contains("tool-line-diff"));
    assert!(html.contains("tool-diff-line remove"));
    assert!(html.contains("tool-diff-line add"));
    assert!(html.contains("@@ -7,2 +7,2 @@"));
    assert!(html.contains("browser snapshot"));
    assert!(html.contains("server"));
    assert!(html.contains("in_progress → completed"));
    assert_eq!(
        html.matches("raw output that should be hidden").count(),
        1,
        "structured file_patch output should appear only for the MCP sample, not the Edit diff"
    );
}

#[test]
fn test_render_tool_detail_shortens_home_paths_in_patch_headers() {
    let input = json!({
        "patch": "*** Begin Patch\n*** Update File: /Users/alice/project/src/app.ts\n@@\n-old\n+new\n*** End Patch\n"
    })
    .to_string();
    let html = cc_session_lib::exporter_test_helpers::render_tool_detail_pub("Edit", &input);

    assert!(html.contains("*** Update File: ~/project/src/app.ts"));
    assert!(!html.contains("/Users/alice"));
}
