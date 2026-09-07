use std::fs;

use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::provider::{SessionProvider, SourceState};

fn rows(start: u64, base: Option<(u64, u64)>, text: &str) -> String {
    let mut header = json!({
        "id": "thread-test", "cwd": "/tmp/project", "cli_version": "fixture",
    });
    if let Some((ordinal, bytes)) = base {
        header["history_base"] = json!({
            "thread_id": "thread-test", "end_ordinal_exclusive": ordinal, "end_byte_offset": bytes,
        });
    }
    let usage = json!({"input_tokens": 100, "cached_input_tokens": 40, "output_tokens": 10, "total_tokens": 110});
    [
        json!({"ordinal": start, "type": "session_meta", "payload": header}),
        json!({"ordinal": start + 1, "type": "turn_context", "payload": {"turn_id": format!("turn-{start}"), "model": "model-test"}}),
        json!({"ordinal": start + 2, "type": "response_item", "payload": {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": text}]}}),
        json!({"ordinal": start + 3, "type": "event_msg", "payload": {"type": "token_count", "info": {"last_token_usage": usage, "total_token_usage": usage}}}),
    ].into_iter().enumerate().map(|(i, mut row)| {
        row["timestamp"] = json!(format!("2026-09-01T00:{start:02}:{i:02}Z"));
        format!("{row}\n")
    }).collect()
}

fn fixture() -> (TempDir, CodexProvider, PathBuf, PathBuf) {
    let home = TempDir::new().unwrap();
    let dir = home.path().join(".codex/sessions");
    fs::create_dir_all(&dir).unwrap();
    let root = dir.join("root.jsonl");
    let leaf = dir.join("continuation.jsonl");
    let prefix = rows(0, None, "retained history");
    // A discarded branch after the retained boundary must not leak into the
    // new timeline or token totals.
    fs::write(
        &root,
        format!("{prefix}{}", rows(4, None, "discarded branch")),
    )
    .unwrap();
    fs::write(
        &leaf,
        rows(4, Some((4, prefix.len() as u64)), "new history"),
    )
    .unwrap();
    let provider = CodexProvider {
        home_dir: home.path().to_path_buf(),
    };
    (home, provider, root, leaf)
}

#[test]
fn pagination_preserves_prefix_and_continuation_once_across_scans_and_loads() {
    let (_home, provider, _root, leaf) = fixture();
    let sessions = provider.scan_all().unwrap();
    assert_eq!(sessions.len(), 1);
    let session = &sessions[0];
    assert_eq!(session.meta.source_path, leaf.to_string_lossy());
    assert_eq!(
        session.meta.file_size_bytes,
        fs::metadata(&leaf).unwrap().len()
    );
    assert_eq!(session.usage_events.len(), 2);
    assert_eq!(
        session
            .usage_events
            .iter()
            .map(|event| event.input_tokens)
            .sum::<u64>(),
        120
    );
    assert!(session.content_text.contains("retained history"));
    assert!(session.content_text.contains("new history"));
    assert!(!session.content_text.contains("discarded branch"));
    assert!(super::super::parser::parse_session_tail(&leaf, 100).is_none());
    let known = HashMap::from([(
        session.meta.source_path.clone(),
        SourceState {
            size: session.meta.file_size_bytes,
            mtime: session.source_mtime,
            title: Some(session.meta.title.clone()),
        },
    )]);
    for _ in 0..2 {
        let next = provider.scan_incremental(&known).unwrap();
        assert!(next.parsed.is_empty());
        assert_eq!(next.unchanged_source_paths, vec![leaf.to_string_lossy()]);
    }
    let loaded = provider
        .load_messages(&session.meta.id, &session.meta.source_path)
        .unwrap();
    assert_eq!(loaded.messages.len(), session.messages.len());
}

#[test]
fn pagination_supports_multiple_retained_segments() {
    let (home, provider, _root, leaf) = fixture();
    let next = home.path().join(".codex/sessions/next.jsonl");
    fs::write(
        &next,
        rows(
            8,
            Some((8, fs::metadata(&leaf).unwrap().len())),
            "third segment",
        ),
    )
    .unwrap();
    let sessions = provider.scan_all().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].usage_events.len(), 3);
    assert!(sessions[0].content_text.contains("retained history"));
    assert!(sessions[0].content_text.contains("third segment"));
}

#[test]
fn pagination_rejects_missing_or_misaligned_base_instead_of_indexing_a_fragment() {
    let (_home, provider, root, leaf) = fixture();
    let original = fs::read(&leaf).unwrap();
    fs::write(&leaf, rows(4, Some((4, 1)), "bad boundary")).unwrap();
    assert!(provider.scan_all().is_err());
    assert!(provider.parse_session_file(&leaf).is_none());
    fs::write(&leaf, original).unwrap();
    fs::remove_file(root).unwrap();
    assert!(provider.scan_all().is_err());
    assert!(provider.parse_session_file(&leaf).is_none());
}

#[test]
fn pagination_rename_uses_header_identity_instead_of_segment_filename() {
    let (home, provider, _root, _leaf) = fixture();
    let session = provider.scan_all().unwrap().remove(0);
    let known = HashMap::from([(
        session.meta.source_path,
        SourceState {
            size: session.meta.file_size_bytes,
            mtime: session.source_mtime,
            title: Some(session.meta.title),
        },
    )]);
    fs::write(
        home.path().join(".codex/session_index.jsonl"),
        "{\"id\":\"thread-test\",\"thread_name\":\"renamed\"}\n",
    )
    .unwrap();
    let next = provider.scan_incremental(&known).unwrap();
    assert_eq!(next.parsed.len(), 1);
    assert_eq!(next.parsed[0].meta.title, "renamed");
}
