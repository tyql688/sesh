use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::models::{Message, MessageRole, Provider, SessionMeta};
use crate::provider::ParsedSession;
use crate::provider_utils::{truncate_to_bytes, FTS_CONTENT_LIMIT};

use super::Database;

/// Rebuild the FTS content text from typed messages, keeping only the
/// dialogue roles (user + assistant). This excludes tool calls, tool output,
/// and system/thinking messages so the global search matches what the user
/// actually said or what the model actually replied. Falls back to the
/// provider-supplied content_text when messages carry no real content
/// (e.g. OpenCode emits Assistant stubs only for token accounting).
fn dialogue_content_text(messages: &[Message], fallback: &str) -> String {
    let parts: Vec<&str> = messages
        .iter()
        .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
        .map(|m| m.content.as_str())
        .filter(|c| !c.trim().is_empty())
        .collect();

    if parts.is_empty() {
        return fallback.to_string();
    }

    truncate_to_bytes(&parts.join("\n"), FTS_CONTENT_LIMIT)
}

/// A single row in session_token_stats, keyed by (date, model).
pub struct TokenStatRow {
    pub date: String,
    pub model: String,
    pub turn_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub cost_usd: f64,
}

impl Database {
    pub fn sync_provider_snapshot(
        &self,
        provider: &Provider,
        sessions: &[ParsedSession],
        aggressive: bool,
    ) -> Result<(), rusqlite::Error> {
        let provider_key = provider.key().to_string();
        let mut ids_by_source: HashMap<String, HashSet<String>> = HashMap::new();
        let mut source_paths = Vec::new();
        let mut seen_sources = HashSet::new();

        for parsed in sessions {
            let source_path = parsed.meta.source_path.clone();
            ids_by_source
                .entry(source_path.clone())
                .or_default()
                .insert(parsed.meta.id.clone());

            if seen_sources.insert(source_path.clone()) {
                source_paths.push(source_path);
            }
        }

        let current_count = self.count_sessions_for_provider(&provider_key)?;
        let scan_count = sessions.len() as u64;
        let should_delete = if aggressive {
            if scan_count == 0 {
                log::info!(
                    "provider {:?} aggressive reindex: scan returned 0 sessions, clearing stale entries",
                    provider
                );
            }
            true
        } else if scan_count == 0 {
            log::warn!(
                "provider {:?} scan returned 0 sessions, skipping deletion to protect index",
                provider
            );
            false
        } else {
            current_count <= 10 || (scan_count as f64 / current_count as f64) > 0.5
        };

        if !should_delete {
            log::warn!(
                "provider {:?} scan returned {} sessions but DB has {}, skipping destructive sync",
                provider,
                scan_count,
                current_count
            );
        }

        self.with_transaction(|conn| {
            for parsed in sessions {
                let content = dialogue_content_text(&parsed.messages, &parsed.content_text);
                upsert_session_on(conn, &parsed.meta, &content)?;
            }

            if should_delete {
                for (source_path, ids) in &ids_by_source {
                    delete_missing_sessions_for_source(conn, &provider_key, source_path, ids)?;
                }

                delete_missing_sources_for_provider(conn, &provider_key, &source_paths)?;
                conn.execute(
                    "DELETE FROM favorites WHERE session_id NOT IN (SELECT id FROM sessions)",
                    [],
                )?;
            }
            Ok(())
        })
    }

    pub fn sync_source_snapshot(
        &self,
        provider: &Provider,
        source_path: &str,
        sessions: &[ParsedSession],
    ) -> Result<(), rusqlite::Error> {
        let provider_key = provider.key().to_string();
        let ids: HashSet<String> = sessions
            .iter()
            .map(|parsed| parsed.meta.id.clone())
            .collect();

        let current_count = self.count_sessions_for_source(&provider_key, source_path)?;
        let scan_count = sessions.len() as u64;
        // For single-source sync, scan_count==0 is a valid signal (file deleted).
        // Only apply ratio guard when both sides are non-zero.
        let should_delete = scan_count == 0
            || current_count <= 10
            || (scan_count as f64 / current_count as f64) > 0.5;

        if !should_delete {
            log::warn!(
                "provider {:?} source {:?} scan returned {} sessions but DB has {}, skipping destructive sync",
                provider, source_path, scan_count, current_count
            );
        }

        self.with_transaction(|conn| {
            for parsed in sessions {
                let content = dialogue_content_text(&parsed.messages, &parsed.content_text);
                upsert_session_on(conn, &parsed.meta, &content)?;
            }

            if should_delete {
                delete_missing_sessions_for_source(conn, &provider_key, source_path, &ids)?;
                conn.execute(
                    "DELETE FROM favorites WHERE session_id NOT IN (SELECT id FROM sessions)",
                    [],
                )?;
            }
            Ok(())
        })
    }

    pub fn rename_session(&self, id: &str, new_title: &str) -> Result<(), rusqlite::Error> {
        let title = if new_title.chars().count() > 200 {
            new_title
                .chars()
                .take(200)
                .collect::<String>()
                .trim_end()
                .to_string()
        } else {
            new_title.to_string()
        };
        let conn = self.lock_write()?;
        conn.execute(
            "UPDATE sessions SET title = ?1, title_custom = 1 WHERE id = ?2",
            params![title, id],
        )?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<(), rusqlite::Error> {
        // Delete all data and rebuild FTS index.
        // Note: VACUUM is impossible while two connections are open,
        // so free pages remain in the file but get reused by subsequent writes.
        self.with_transaction(|conn| {
            conn.execute_batch(
                "DELETE FROM session_token_stats;
                 DELETE FROM favorites;
                 DELETE FROM sessions;
                 DELETE FROM meta;
                 INSERT INTO sessions_fts(sessions_fts) VALUES('rebuild');",
            )
        })
    }

    pub fn clear_usage_stats(&self) -> Result<(), rusqlite::Error> {
        let conn = self.lock_write()?;
        conn.execute("DELETE FROM session_token_stats", [])?;
        Ok(())
    }

    /// Delete this session and all its children from DB.
    pub fn delete_session(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_write()?;
        conn.execute("DELETE FROM favorites WHERE session_id IN (SELECT id FROM sessions WHERE parent_id = ?1)", params![id])?;
        conn.execute("DELETE FROM sessions WHERE parent_id = ?1", params![id])?;
        conn.execute("DELETE FROM favorites WHERE session_id = ?1", params![id])?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Replace all token stats for a session. Called during indexing.
    /// Deletes existing rows first, then inserts new per-(date, model) aggregates.
    pub fn replace_token_stats(
        &self,
        session_id: &str,
        stats: &[TokenStatRow],
    ) -> Result<(), rusqlite::Error> {
        let conn = self.lock_write()?;
        conn.execute(
            "DELETE FROM session_token_stats WHERE session_id = ?1",
            params![session_id],
        )?;
        let mut stmt = conn.prepare_cached(
            "INSERT INTO session_token_stats
                (session_id, date, model, turn_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for row in stats {
            stmt.execute(params![
                session_id,
                row.date,
                row.model,
                row.turn_count as i64,
                row.input_tokens as i64,
                row.output_tokens as i64,
                row.cache_read_tokens as i64,
                row.cache_write_tokens as i64,
                row.cost_usd,
            ])?;
        }
        Ok(())
    }

    /// Replace token stats for multiple sessions atomically within a single
    /// transaction.  The frontend reads via a separate connection, so without
    /// a transaction the reader can observe a partially-updated state (e.g.
    /// after a DELETE but before the matching INSERTs), causing usage numbers
    /// to "jump" on every poll cycle.
    pub fn replace_token_stats_batch(
        &self,
        batch: &[(&str, &[TokenStatRow])],
    ) -> Result<(), rusqlite::Error> {
        self.with_transaction(|conn| {
            let mut insert = conn.prepare_cached(
                "INSERT INTO session_token_stats
                    (session_id, date, model, turn_count, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, cost_usd)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for &(session_id, stats) in batch {
                conn.execute(
                    "DELETE FROM session_token_stats WHERE session_id = ?1",
                    params![session_id],
                )?;
                for row in stats {
                    insert.execute(params![
                        session_id,
                        row.date,
                        row.model,
                        row.turn_count as i64,
                        row.input_tokens as i64,
                        row.output_tokens as i64,
                        row.cache_read_tokens as i64,
                        row.cache_write_tokens as i64,
                        row.cost_usd,
                    ])?;
                }
            }
            Ok(())
        })
    }
}

fn upsert_session_on(
    conn: &Connection,
    meta: &SessionMeta,
    content_text: &str,
) -> Result<(), rusqlite::Error> {
    let provider_str = meta.provider.key();

    conn.execute(
        "INSERT INTO sessions (id, provider, title, project_path, project_name,
            created_at, updated_at, message_count, file_size_bytes, source_path, content_text, is_sidechain,
            variant_name, model, cc_version, git_branch, parent_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
         ON CONFLICT(id) DO UPDATE SET
            provider = excluded.provider,
            title = CASE WHEN sessions.title_custom = 1 THEN sessions.title ELSE excluded.title END,
            project_path = excluded.project_path,
            project_name = excluded.project_name,
            created_at = excluded.created_at,
            updated_at = excluded.updated_at,
            message_count = excluded.message_count,
            file_size_bytes = excluded.file_size_bytes,
            source_path = excluded.source_path,
            content_text = excluded.content_text,
            is_sidechain = excluded.is_sidechain,
            variant_name = excluded.variant_name,
            model = excluded.model,
            cc_version = excluded.cc_version,
            git_branch = excluded.git_branch,
            parent_id = excluded.parent_id",
        params![
            meta.id,
            provider_str,
            meta.title,
            meta.project_path,
            meta.project_name,
            meta.created_at,
            meta.updated_at,
            meta.message_count,
            meta.file_size_bytes,
            meta.source_path,
            content_text,
            meta.is_sidechain as i64,
            meta.variant_name,
            meta.model,
            meta.cc_version,
            meta.git_branch,
            meta.parent_id,
        ],
    )?;
    Ok(())
}

fn delete_missing_sessions_for_source(
    conn: &Connection,
    provider_key: &str,
    source_path: &str,
    ids: &HashSet<String>,
) -> Result<(), rusqlite::Error> {
    let mut sql = String::from("DELETE FROM sessions WHERE provider = ?1 AND source_path = ?2");
    let mut params_refs: Vec<&dyn rusqlite::types::ToSql> = vec![&provider_key, &source_path];
    let mut ids_vec: Vec<&String> = ids.iter().collect();
    ids_vec.sort();

    if !ids_vec.is_empty() {
        sql.push_str(" AND id NOT IN (");
        sql.push_str(&repeat_vars(ids_vec.len()));
        sql.push(')');
        for id in &ids_vec {
            params_refs.push(*id);
        }
    }

    conn.execute(&sql, params_refs.as_slice())?;
    Ok(())
}

fn delete_missing_sources_for_provider(
    conn: &Connection,
    provider_key: &str,
    source_paths: &[String],
) -> Result<(), rusqlite::Error> {
    if source_paths.is_empty() {
        conn.execute(
            "DELETE FROM sessions WHERE provider = ?1",
            params![provider_key],
        )?;
        return Ok(());
    }

    let mut sql = String::from("DELETE FROM sessions WHERE provider = ?1 AND source_path NOT IN (");
    sql.push_str(&repeat_vars(source_paths.len()));
    sql.push(')');

    let mut params_refs: Vec<&dyn rusqlite::types::ToSql> = vec![&provider_key];
    for source_path in source_paths {
        params_refs.push(source_path);
    }

    conn.execute(&sql, params_refs.as_slice())?;
    Ok(())
}

fn repeat_vars(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{Database, SessionMeta, TokenStatRow};
    use crate::models::Provider;
    use crate::provider::ParsedSession;
    use tempfile::TempDir;

    fn sample_meta(session_id: &str) -> SessionMeta {
        SessionMeta {
            id: session_id.to_string(),
            provider: Provider::Claude,
            title: "Test".into(),
            project_path: "/tmp/project".into(),
            project_name: "project".into(),
            created_at: 1_775_635_200,
            updated_at: 1_775_635_200,
            message_count: 1,
            file_size_bytes: 0,
            source_path: "/tmp/source.jsonl".into(),
            is_sidechain: false,
            variant_name: None,
            model: Some("claude-opus-4-6".into()),
            cc_version: None,
            git_branch: None,
            parent_id: None,
        }
    }

    #[test]
    fn replace_token_stats_clears_existing_rows_when_empty() {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let meta = sample_meta("session-1");
        db.sync_provider_snapshot(
            &Provider::Claude,
            &[ParsedSession {
                meta: meta.clone(),
                messages: Vec::new(),
                content_text: String::new(),
                parse_warning_count: 0,
            }],
            true,
        )
        .unwrap();

        db.replace_token_stats(
            &meta.id,
            &[TokenStatRow {
                date: "2026-04-09".into(),
                model: "claude-opus-4-6".into(),
                turn_count: 1,
                input_tokens: 100,
                output_tokens: 50,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                cost_usd: 0.01,
            }],
        )
        .unwrap();

        db.replace_token_stats(&meta.id, &[]).unwrap();

        let conn = db.lock_read().unwrap();
        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM session_token_stats WHERE session_id = ?1",
                [meta.id.as_str()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn clear_usage_stats_preserves_sessions() {
        let dir = TempDir::new().unwrap();
        let db = Database::open(dir.path()).unwrap();
        let meta = sample_meta("session-2");
        db.sync_provider_snapshot(
            &Provider::Claude,
            &[ParsedSession {
                meta: meta.clone(),
                messages: Vec::new(),
                content_text: String::new(),
                parse_warning_count: 0,
            }],
            true,
        )
        .unwrap();
        db.replace_token_stats(
            &meta.id,
            &[TokenStatRow {
                date: "2026-04-10".into(),
                model: "claude-opus-4-6".into(),
                turn_count: 1,
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                cost_usd: 0.001,
            }],
        )
        .unwrap();

        db.clear_usage_stats().unwrap();

        assert!(db.get_session(&meta.id).unwrap().is_some());
        let conn = db.lock_read().unwrap();
        let usage_rows: u64 = conn
            .query_row("SELECT COUNT(*) FROM session_token_stats", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(usage_rows, 0);
    }
}
