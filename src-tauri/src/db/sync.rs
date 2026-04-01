use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};

use crate::models::{Provider, SessionMeta};
use crate::provider::ParsedSession;

use super::Database;

impl Database {
    pub fn sync_provider_snapshot(
        &self,
        provider: &Provider,
        sessions: &[ParsedSession],
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
        let should_delete = if scan_count == 0 {
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
                upsert_session_on(conn, &parsed.meta, &parsed.content_text)?;
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
                upsert_session_on(conn, &parsed.meta, &parsed.content_text)?;
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
        let conn = self.lock_write()?;
        conn.execute(
            "UPDATE sessions SET title = ?1, title_custom = 1 WHERE id = ?2",
            params![new_title, id],
        )?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<(), rusqlite::Error> {
        // Drop all data and rebuild tables from scratch.
        // We can't VACUUM while two connections are open, so instead we
        // DROP + recreate tables. This reclaims space within the same file
        // without needing exclusive access.
        let conn = self.lock_write()?;
        conn.execute_batch(
            "DROP TABLE IF EXISTS favorites;
             DROP TABLE IF EXISTS sessions_fts;
             DROP TABLE IF EXISTS sessions;
             DROP TABLE IF EXISTS meta;",
        )?;
        // Recreate tables (same schema as Database::open)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                 id TEXT PRIMARY KEY,
                 provider TEXT NOT NULL,
                 title TEXT NOT NULL DEFAULT '',
                 project_path TEXT NOT NULL DEFAULT '',
                 project_name TEXT NOT NULL DEFAULT '',
                 created_at INTEGER NOT NULL DEFAULT 0,
                 updated_at INTEGER NOT NULL DEFAULT 0,
                 message_count INTEGER NOT NULL DEFAULT 0,
                 file_size_bytes INTEGER NOT NULL DEFAULT 0,
                 source_path TEXT NOT NULL DEFAULT '',
                 content_text TEXT NOT NULL DEFAULT '',
                 is_sidechain INTEGER NOT NULL DEFAULT 0,
                 title_custom INTEGER NOT NULL DEFAULT 0,
                 variant_name TEXT,
                 model TEXT,
                 cc_version TEXT,
                 git_branch TEXT,
                 parent_id TEXT
             );
             CREATE TABLE IF NOT EXISTS meta (
                 key TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS favorites (
                 session_id TEXT PRIMARY KEY REFERENCES sessions(id),
                 added_at INTEGER NOT NULL DEFAULT 0
             );
             CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
                 title, content_text, content=sessions, content_rowid=rowid
             );",
        )?;
        drop(conn);
        // Checkpoint WAL to truncate WAL file
        let conn = self.lock_write()?;
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        Ok(())
    }

    pub fn delete_session(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_write()?;
        // Cascade: delete child sessions first
        conn.execute("DELETE FROM favorites WHERE session_id IN (SELECT id FROM sessions WHERE parent_id = ?1)", params![id])?;
        conn.execute("DELETE FROM sessions WHERE parent_id = ?1", params![id])?;
        // Delete the session itself
        conn.execute("DELETE FROM favorites WHERE session_id = ?1", params![id])?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }
}

fn provider_to_str(provider: &Provider) -> &'static str {
    provider.key()
}

fn upsert_session_on(
    conn: &Connection,
    meta: &SessionMeta,
    content_text: &str,
) -> Result<(), rusqlite::Error> {
    let provider_str = provider_to_str(&meta.provider);

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
