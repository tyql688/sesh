use std::path::Path;

use serde_json::Value;

use crate::models::{Provider, SessionMeta};
use crate::provider::ParsedSession;
use crate::provider_utils::{session_title, truncate_to_bytes, FTS_CONTENT_LIMIT};

use super::tools::*;
use super::CursorProvider;

impl CursorProvider {
    pub(super) fn parse_session_db(&self, db_path: &Path) -> Option<ParsedSession> {
        let conn = Self::open_db(db_path)?;
        let rows = Self::read_blobs(&conn);

        let mut first_user_message: Option<String> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut message_count: u32 = 0;
        let mut project_path = String::new();

        for raw in &rows {
            let msg: Value = match serde_json::from_str(raw) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "user" => {
                    let text = extract_text_content(&msg);
                    // Extract project path from user_info before filtering
                    if project_path.is_empty() {
                        if let Some(wp) = extract_workspace_path(&text) {
                            project_path = wp;
                        }
                    }
                    let clean = extract_user_text(&text);
                    if !clean.is_empty() {
                        if first_user_message.is_none() {
                            first_user_message = Some(clean.clone());
                        }
                        content_parts.push(clean);
                        message_count += 1;
                    }
                }
                "assistant" => {
                    let text = extract_text_content(&msg);
                    let clean = strip_think_tags(&text);
                    if !clean.is_empty() {
                        content_parts.push(clean);
                    }
                    message_count += 1;
                }
                "tool" => {
                    message_count += 1;
                }
                _ => {}
            }
        }

        if message_count == 0 {
            return None;
        }

        let title = session_title(first_user_message.as_deref());
        let project_name = Path::new(&project_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let file_size = std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0);

        // Session ID from directory name (UUID)
        let session_id = db_path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Approximate timestamps from file metadata
        let created_at = std::fs::metadata(db_path)
            .ok()
            .and_then(|m| m.created().ok())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
            })
            .unwrap_or(0);
        let updated_at = std::fs::metadata(db_path)
            .ok()
            .and_then(|m| m.modified().ok())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
            })
            .unwrap_or(created_at);

        Some(ParsedSession {
            meta: SessionMeta {
                id: session_id,
                provider: Provider::Cursor,
                title,
                project_path: project_path.clone(),
                project_name,
                created_at,
                updated_at,
                message_count,
                file_size_bytes: file_size,
                source_path: db_path.to_string_lossy().to_string(),
                is_sidechain: false,
                variant_name: None,
                model: None,
                cc_version: None,
                git_branch: None,
                parent_id: None,
            },
            messages: Vec::new(),
            content_text: truncate_to_bytes(&content_parts.join("\n"), FTS_CONTENT_LIMIT),
        })
    }
}
