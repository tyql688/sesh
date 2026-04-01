use std::collections::HashMap;

use rusqlite::{params, Connection};

use crate::models::{Provider, SearchFilters, SearchResult, SessionMeta};

use super::row_mapper::row_to_session_meta;
use super::Database;

impl Database {
    pub fn get_session(&self, id: &str) -> Result<Option<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_read()?;
        let mut stmt = conn.prepare(
            "SELECT id, provider, title, project_path, project_name,
                    created_at, updated_at, message_count, file_size_bytes, source_path, is_sidechain,
                    variant_name, model, cc_version, git_branch, parent_id
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_session_meta)?;
        match rows.next() {
            Some(Ok(meta)) => Ok(Some(meta)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_read()?;
        list_sessions_from_query(
            &conn,
            "SELECT id, provider, title, project_path, project_name,
                    created_at, updated_at, message_count, file_size_bytes, source_path, is_sidechain,
                    variant_name, model, cc_version, git_branch, parent_id
             FROM sessions ORDER BY updated_at DESC",
            [],
        )
    }

    pub fn search_filtered(
        &self,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, rusqlite::Error> {
        let conn = self.lock_read()?;
        let safe_query = build_fts_query(&filters.query);
        let has_query = safe_query.is_some();
        let has_filters = filters.provider.is_some()
            || filters.project.is_some()
            || filters.after.is_some()
            || filters.before.is_some();

        if !has_query && !has_filters {
            return Ok(Vec::new());
        }

        if let Some(query) = safe_query {
            search_with_fts(&conn, filters, &query).or_else(|_| search_with_like(&conn, filters))
        } else {
            search_with_like(&conn, filters)
        }
    }

    pub fn session_count(&self) -> Result<u64, rusqlite::Error> {
        let conn = self.lock_read()?;
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
    }

    pub fn count_sessions_for_provider(&self, provider_key: &str) -> Result<u64, rusqlite::Error> {
        let conn = self.lock_read()?;
        conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE provider = ?1",
            params![provider_key],
            |row| row.get(0),
        )
    }

    pub fn count_sessions_for_source(
        &self,
        provider_key: &str,
        source_path: &str,
    ) -> Result<u64, rusqlite::Error> {
        let conn = self.lock_read()?;
        conn.query_row(
            "SELECT COUNT(*) FROM sessions WHERE provider = ?1 AND source_path = ?2",
            params![provider_key, source_path],
            |row| row.get(0),
        )
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.lock_read()?;
        let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(val)) => Ok(Some(val)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_write()?;
        conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn vacuum(&self) -> Result<(), rusqlite::Error> {
        let conn = self.lock_write()?;
        conn.execute_batch("VACUUM")
    }

    pub fn db_size_bytes(&self) -> u64 {
        std::fs::metadata(&self.db_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }

    pub fn provider_session_counts(&self) -> Result<HashMap<String, u64>, rusqlite::Error> {
        let conn = self.lock_read()?;
        let mut stmt = conn.prepare("SELECT provider, COUNT(*) FROM sessions GROUP BY provider")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })?;

        let mut counts = HashMap::new();
        for row in rows {
            let (provider, count) = row?;
            counts.insert(provider, count);
        }
        Ok(counts)
    }

    pub fn list_recent_sessions(&self, limit: usize) -> Result<Vec<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_read()?;
        list_sessions_from_query(
            &conn,
            "SELECT id, provider, title, project_path, project_name,
                    created_at, updated_at, message_count, file_size_bytes, source_path, is_sidechain,
                    variant_name, model, cc_version, git_branch, parent_id
             FROM sessions
             ORDER BY updated_at DESC
             LIMIT ?1",
            params![limit as i64],
        )
    }

    /// Returns (id, source_path) pairs for all children of a given parent session.
    pub fn list_children(&self, parent_id: &str) -> Result<Vec<(String, String)>, rusqlite::Error> {
        let conn = self.lock_read()?;
        let mut stmt = conn.prepare("SELECT id, source_path FROM sessions WHERE parent_id = ?1")?;
        let rows = stmt.query_map(params![parent_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Returns full SessionMeta for all children of a given parent session.
    pub fn get_child_sessions(
        &self,
        parent_id: &str,
    ) -> Result<Vec<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_read()?;
        let mut stmt = conn.prepare(
            "SELECT id, provider, title, project_path, project_name,
                    created_at, updated_at, message_count, file_size_bytes, source_path, is_sidechain,
                    variant_name, model, cc_version, git_branch, parent_id
             FROM sessions WHERE parent_id = ?1
             ORDER BY created_at",
        )?;
        let rows = stmt.query_map(params![parent_id], row_to_session_meta)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn add_favorite(&self, session_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_write()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT OR IGNORE INTO favorites (session_id, added_at) VALUES (?1, ?2)",
            params![session_id, now],
        )?;
        Ok(())
    }

    pub fn remove_favorite(&self, session_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_write()?;
        conn.execute(
            "DELETE FROM favorites WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn is_favorite(&self, session_id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_read()?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM favorites WHERE session_id = ?1)",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    pub fn list_favorites(&self) -> Result<Vec<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_read()?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.provider, s.title, s.project_path, s.project_name,
                    s.created_at, s.updated_at, s.message_count, s.file_size_bytes, s.source_path, s.is_sidechain,
                    s.variant_name, s.model, s.cc_version, s.git_branch, s.parent_id
             FROM favorites f
             JOIN sessions s ON s.id = f.session_id
             ORDER BY f.added_at DESC",
        )?;

        let rows = stmt.query_map([], row_to_session_meta)?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }
}

pub fn provider_to_str_pub(provider: &Provider) -> &'static str {
    provider.key()
}

fn list_sessions_from_query<P>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<SessionMeta>, rusqlite::Error>
where
    P: rusqlite::Params,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, row_to_session_meta)?;

    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(row?);
    }
    Ok(sessions)
}

fn search_with_fts(
    conn: &Connection,
    filters: &SearchFilters,
    query: &str,
) -> Result<Vec<SearchResult>, rusqlite::Error> {
    let mut sql = String::from(
        "SELECT s.id, s.provider, s.title, s.project_path, s.project_name,
                s.created_at, s.updated_at, s.message_count, s.file_size_bytes, s.source_path, s.is_sidechain,
                s.variant_name, s.model, s.cc_version, s.git_branch, s.parent_id,
                snippet(sessions_fts, -1, '<mark>', '</mark>', '...', 64) AS snip
         FROM sessions_fts
         JOIN sessions s ON s.rowid = sessions_fts.rowid
         WHERE sessions_fts MATCH ?",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(query.to_string())];
    append_search_filters(&mut sql, &mut param_values, filters);
    sql.push_str(" ORDER BY rank LIMIT 100");
    query_search_results(conn, &sql, &param_values)
}

fn search_with_like(
    conn: &Connection,
    filters: &SearchFilters,
) -> Result<Vec<SearchResult>, rusqlite::Error> {
    let mut sql = String::from(
        "SELECT s.id, s.provider, s.title, s.project_path, s.project_name,
                s.created_at, s.updated_at, s.message_count, s.file_size_bytes, s.source_path, s.is_sidechain,
                s.variant_name, s.model, s.cc_version, s.git_branch, s.parent_id,
                CASE
                    WHEN ?1 <> '' THEN substr(s.content_text, 1, 200)
                    ELSE ''
                END AS snip
         FROM sessions s
         WHERE 1=1",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(filters.query.trim().to_string())];

    if !filters.query.trim().is_empty() {
        sql.push_str(
            " AND (
                s.title LIKE '%' || ?2 || '%'
                OR s.content_text LIKE '%' || ?2 || '%'
                OR s.project_name LIKE '%' || ?2 || '%'
            )",
        );
        param_values.push(Box::new(filters.query.trim().to_string()));
    }

    let next_index = param_values.len() + 1;
    append_search_filters_numbered(&mut sql, &mut param_values, filters, next_index);
    sql.push_str(" ORDER BY s.created_at DESC LIMIT 100");
    query_search_results(conn, &sql, &param_values)
}

fn append_search_filters(
    sql: &mut String,
    param_values: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    filters: &SearchFilters,
) {
    if let Some(ref provider) = filters.provider {
        sql.push_str(" AND s.provider = ?");
        param_values.push(Box::new(provider.clone()));
    }
    if let Some(ref project) = filters.project {
        sql.push_str(" AND s.project_name LIKE '%' || ? || '%'");
        param_values.push(Box::new(project.clone()));
    }
    if let Some(after) = filters.after {
        sql.push_str(" AND s.created_at > ?");
        param_values.push(Box::new(after));
    }
    if let Some(before) = filters.before {
        sql.push_str(" AND s.created_at < ?");
        param_values.push(Box::new(before));
    }
}

fn append_search_filters_numbered(
    sql: &mut String,
    param_values: &mut Vec<Box<dyn rusqlite::types::ToSql>>,
    filters: &SearchFilters,
    mut next_index: usize,
) {
    if let Some(ref provider) = filters.provider {
        sql.push_str(&format!(" AND s.provider = ?{next_index}"));
        param_values.push(Box::new(provider.clone()));
        next_index += 1;
    }
    if let Some(ref project) = filters.project {
        sql.push_str(&format!(
            " AND s.project_name LIKE '%' || ?{next_index} || '%'"
        ));
        param_values.push(Box::new(project.clone()));
        next_index += 1;
    }
    if let Some(after) = filters.after {
        sql.push_str(&format!(" AND s.created_at > ?{next_index}"));
        param_values.push(Box::new(after));
        next_index += 1;
    }
    if let Some(before) = filters.before {
        sql.push_str(&format!(" AND s.created_at < ?{next_index}"));
        param_values.push(Box::new(before));
    }
}

fn query_search_results(
    conn: &Connection,
    sql: &str,
    param_values: &[Box<dyn rusqlite::types::ToSql>],
) -> Result<Vec<SearchResult>, rusqlite::Error> {
    let mut stmt = conn.prepare(sql)?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(std::convert::AsRef::as_ref)
        .collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(SearchResult {
            session: row_to_session_meta(row)?,
            snippet: row.get(16)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

fn build_fts_query(raw: &str) -> Option<String> {
    let tokens: Vec<String> = raw
        .split_whitespace()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
        .collect();

    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" AND "))
    }
}
