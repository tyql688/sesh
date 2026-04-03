//! Golden tests for tool display rendering in the HTML exporter.
//! Fixtures shared with frontend vitest tests.

use serde::Deserialize;

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
