use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::models::{Provider, SearchFilters, SearchResult, SessionMeta};
use crate::provider::ParsedSession;

pub struct Database {
    conn: Mutex<Connection>,
    db_path: std::path::PathBuf,
}

impl Database {
    /// Acquire the database connection lock, recovering from mutex poisoning.
    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, rusqlite::Error> {
        self.conn.lock().map_err(|_| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                Some("database mutex poisoned".to_string()),
            )
        })
    }
}

impl Database {
    pub fn with_transaction<T, F>(&self, f: F) -> Result<T, rusqlite::Error>
    where
        F: FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    {
        let conn = self.lock_conn()?;
        conn.execute_batch("BEGIN IMMEDIATE TRANSACTION")?;
        match f(&conn) {
            Ok(value) => {
                conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(err) => {
                let _ = conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn open(data_dir: &Path) -> Result<Self, rusqlite::Error> {
        std::fs::create_dir_all(data_dir).ok();
        let db_path = data_dir.join("sessions.db");
        let conn = Connection::open(&db_path)?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -2000;",
        )?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id              TEXT PRIMARY KEY,
                provider        TEXT NOT NULL,
                title           TEXT NOT NULL DEFAULT '',
                project_path    TEXT NOT NULL DEFAULT '',
                project_name    TEXT NOT NULL DEFAULT '',
                created_at      INTEGER NOT NULL DEFAULT 0,
                updated_at      INTEGER NOT NULL DEFAULT 0,
                message_count   INTEGER NOT NULL DEFAULT 0,
                file_size_bytes INTEGER NOT NULL DEFAULT 0,
                source_path     TEXT NOT NULL DEFAULT '',
                content_text    TEXT NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_sessions_provider ON sessions(provider);
            CREATE INDEX IF NOT EXISTS idx_sessions_project_name ON sessions(project_name);
            CREATE INDEX IF NOT EXISTS idx_sessions_created_at ON sessions(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_sessions_provider_updated ON sessions(provider, updated_at DESC);

            CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
                title, content_text, project_name,
                content='sessions',
                content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS sessions_ai AFTER INSERT ON sessions BEGIN
                INSERT INTO sessions_fts(rowid, title, content_text, project_name)
                VALUES (new.rowid, new.title, new.content_text, new.project_name);
            END;

            CREATE TRIGGER IF NOT EXISTS sessions_ad AFTER DELETE ON sessions BEGIN
                INSERT INTO sessions_fts(sessions_fts, rowid, title, content_text, project_name)
                VALUES ('delete', old.rowid, old.title, old.content_text, old.project_name);
            END;

            CREATE TRIGGER IF NOT EXISTS sessions_au AFTER UPDATE ON sessions BEGIN
                INSERT INTO sessions_fts(sessions_fts, rowid, title, content_text, project_name)
                VALUES ('delete', old.rowid, old.title, old.content_text, old.project_name);
                INSERT INTO sessions_fts(rowid, title, content_text, project_name)
                VALUES (new.rowid, new.title, new.content_text, new.project_name);
            END;

            CREATE TABLE IF NOT EXISTS meta (
                key   TEXT PRIMARY KEY,
                value TEXT
            );

            CREATE TABLE IF NOT EXISTS favorites (
                session_id TEXT PRIMARY KEY,
                added_at   INTEGER NOT NULL
            );

            -- Add title_custom column for user-renamed sessions (safe to re-run)
            ",
        )?;

        // Migration: add title_custom column if not exists
        let has_title_custom: bool = {
            let mut stmt = conn.prepare(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'title_custom'",
            )?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            count > 0
        };
        if !has_title_custom {
            conn.execute_batch(
                "ALTER TABLE sessions ADD COLUMN title_custom INTEGER NOT NULL DEFAULT 0;",
            )?;
        }

        // Migration: add is_sidechain column if not exists
        let has_is_sidechain: bool = {
            let mut stmt = conn.prepare(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'is_sidechain'",
            )?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            count > 0
        };
        if !has_is_sidechain {
            conn.execute_batch(
                "ALTER TABLE sessions ADD COLUMN is_sidechain INTEGER NOT NULL DEFAULT 0;",
            )?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
            db_path,
        })
    }

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

        self.with_transaction(|conn| {
            for parsed in sessions {
                upsert_session_on(conn, &parsed.meta, &parsed.content_text)?;
            }

            for (source_path, ids) in &ids_by_source {
                delete_missing_sessions_for_source(conn, &provider_key, source_path, ids)?;
            }

            delete_missing_sources_for_provider(conn, &provider_key, &source_paths)?;
            conn.execute(
                "DELETE FROM favorites WHERE session_id NOT IN (SELECT id FROM sessions)",
                [],
            )?;
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

        self.with_transaction(|conn| {
            for parsed in sessions {
                upsert_session_on(conn, &parsed.meta, &parsed.content_text)?;
            }

            delete_missing_sessions_for_source(conn, &provider_key, source_path, &ids)?;
            conn.execute(
                "DELETE FROM favorites WHERE session_id NOT IN (SELECT id FROM sessions)",
                [],
            )?;
            Ok(())
        })
    }

    pub fn get_session(&self, id: &str) -> Result<Option<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, provider, title, project_path, project_name,
                    created_at, updated_at, message_count, file_size_bytes, source_path, is_sidechain
             FROM sessions WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(SessionMeta {
                id: row.get(0)?,
                provider: str_to_provider(&row.get::<_, String>(1)?),
                title: row.get(2)?,
                project_path: row.get(3)?,
                project_name: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                message_count: row.get(7)?,
                file_size_bytes: row.get(8)?,
                source_path: row.get(9)?,
                is_sidechain: row.get::<_, i64>(10).unwrap_or(0) != 0,
            })
        })?;
        match rows.next() {
            Some(Ok(meta)) => Ok(Some(meta)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn rename_session(&self, id: &str, new_title: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "UPDATE sessions SET title = ?1, title_custom = 1 WHERE id = ?2",
            params![new_title, id],
        )?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<(), rusqlite::Error> {
        self.with_transaction(|conn| {
            conn.execute_batch(
                "DELETE FROM favorites; DELETE FROM sessions; DELETE FROM meta;"
            )?;
            Ok(())
        })
    }

    pub fn delete_session(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute("DELETE FROM favorites WHERE session_id = ?1", params![id])?;
        conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        list_sessions_from_query(
            &conn,
            "SELECT id, provider, title, project_path, project_name,
                    created_at, updated_at, message_count, file_size_bytes, source_path, is_sidechain
             FROM sessions ORDER BY updated_at DESC",
            [],
        )
    }

    pub fn search_filtered(
        &self,
        filters: &SearchFilters,
    ) -> Result<Vec<SearchResult>, rusqlite::Error> {
        let conn = self.lock_conn()?;
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
        let conn = self.lock_conn()?;
        conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(Ok(val)) => Ok(Some(val)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn set_meta(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
        conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn db_size_bytes(&self) -> u64 {
        std::fs::metadata(&self.db_path)
            .map(|m| m.len())
            .unwrap_or(0)
    }

    pub fn provider_session_counts(&self) -> Result<HashMap<String, u64>, rusqlite::Error> {
        let conn = self.lock_conn()?;
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
        let conn = self.lock_conn()?;
        list_sessions_from_query(
            &conn,
            "SELECT id, provider, title, project_path, project_name,
                    created_at, updated_at, message_count, file_size_bytes, source_path, is_sidechain
             FROM sessions
             ORDER BY updated_at DESC
             LIMIT ?1",
            params![limit as i64],
        )
    }

    pub fn add_favorite(&self, session_id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.lock_conn()?;
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
        let conn = self.lock_conn()?;
        conn.execute(
            "DELETE FROM favorites WHERE session_id = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn is_favorite(&self, session_id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM favorites WHERE session_id = ?1)",
            params![session_id],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    pub fn list_favorites(&self) -> Result<Vec<SessionMeta>, rusqlite::Error> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.provider, s.title, s.project_path, s.project_name,
                    s.created_at, s.updated_at, s.message_count, s.file_size_bytes, s.source_path, s.is_sidechain
             FROM favorites f
             JOIN sessions s ON s.id = f.session_id
             ORDER BY f.added_at DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(SessionMeta {
                id: row.get(0)?,
                provider: str_to_provider(&row.get::<_, String>(1)?),
                title: row.get(2)?,
                project_path: row.get(3)?,
                project_name: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                message_count: row.get(7)?,
                file_size_bytes: row.get(8)?,
                source_path: row.get(9)?,
                is_sidechain: row.get::<_, i64>(10).unwrap_or(0) != 0,
            })
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            sessions.push(row?);
        }
        Ok(sessions)
    }
}

fn provider_to_str(provider: &Provider) -> &'static str {
    provider.key()
}

pub fn provider_to_str_pub(provider: &Provider) -> &'static str {
    provider_to_str(provider)
}

fn str_to_provider(s: &str) -> Provider {
    Provider::from_str(s).unwrap_or(Provider::Claude)
}

fn upsert_session_on(
    conn: &Connection,
    meta: &SessionMeta,
    content_text: &str,
) -> Result<(), rusqlite::Error> {
    let provider_str = provider_to_str(&meta.provider);

    conn.execute(
        "INSERT INTO sessions (id, provider, title, project_path, project_name,
            created_at, updated_at, message_count, file_size_bytes, source_path, content_text, is_sidechain)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
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
            is_sidechain = excluded.is_sidechain",
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

fn list_sessions_from_query<P>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<SessionMeta>, rusqlite::Error>
where
    P: rusqlite::Params,
{
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |row| {
        Ok(SessionMeta {
            id: row.get(0)?,
            provider: str_to_provider(&row.get::<_, String>(1)?),
            title: row.get(2)?,
            project_path: row.get(3)?,
            project_name: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
            message_count: row.get(7)?,
            file_size_bytes: row.get(8)?,
            source_path: row.get(9)?,
            is_sidechain: row.get::<_, i64>(10).unwrap_or(0) != 0,
        })
    })?;

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
    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        param_values.iter().map(std::convert::AsRef::as_ref).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(SearchResult {
            session: SessionMeta {
                id: row.get(0)?,
                provider: str_to_provider(&row.get::<_, String>(1)?),
                title: row.get(2)?,
                project_path: row.get(3)?,
                project_name: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
                message_count: row.get(7)?,
                file_size_bytes: row.get(8)?,
                source_path: row.get(9)?,
                is_sidechain: row.get::<_, i64>(10).unwrap_or(0) != 0,
            },
            snippet: row.get(11)?,
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

fn repeat_vars(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::models::{Message, Provider, SearchFilters, SessionMeta};
    use crate::provider::ParsedSession;

    fn test_db() -> (Database, std::path::PathBuf) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("cc-session-db-test-{unique}"));
        let db = Database::open(&dir).expect("open test db");
        (db, dir)
    }

    fn parsed_session(id: &str, source_path: &str, content_text: &str) -> ParsedSession {
        ParsedSession {
            meta: SessionMeta {
                id: id.to_string(),
                provider: Provider::Claude,
                title: format!("Session {id}"),
                project_path: "/tmp/workspace".to_string(),
                project_name: "workspace".to_string(),
                created_at: 1,
                updated_at: 1,
                message_count: 1,
                file_size_bytes: 64,
                source_path: source_path.to_string(),
                is_sidechain: false,
            },
            messages: vec![Message {
                role: crate::models::MessageRole::User,
                content: content_text.to_string(),
                timestamp: None,
                tool_name: None,
                tool_input: None,
                token_usage: None,
            }],
            content_text: content_text.to_string(),
        }
    }

    #[test]
    fn build_fts_query_quotes_each_token() {
        assert_eq!(
            build_fts_query(r#"provider/path "quoted""#),
            Some(r#""provider/path" AND """quoted""""#.to_string())
        );
    }

    #[test]
    fn sync_provider_snapshot_removes_stale_rows() {
        let (db, dir) = test_db();

        db.sync_provider_snapshot(
            &Provider::Claude,
            &[
                parsed_session("one", "/tmp/one.jsonl", "first session"),
                parsed_session("two", "/tmp/two.jsonl", "second session"),
            ],
        )
        .expect("seed snapshot");

        db.add_favorite("one").expect("favorite session");
        assert_eq!(db.list_sessions().expect("list sessions").len(), 2);

        db.sync_provider_snapshot(
            &Provider::Claude,
            &[parsed_session(
                "two",
                "/tmp/two.jsonl",
                "second session updated",
            )],
        )
        .expect("resync snapshot");

        let sessions = db.list_sessions().expect("list synced sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "two");
        assert!(!db.is_favorite("one").expect("favorite cleanup"));

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn search_filtered_falls_back_for_plain_text_queries() {
        let (db, dir) = test_db();

        db.sync_provider_snapshot(
            &Provider::Claude,
            &[parsed_session(
                "alpha",
                "/tmp/alpha.jsonl",
                "path/to/file and quoted text",
            )],
        )
        .expect("seed searchable session");

        let results = db
            .search_filtered(&SearchFilters {
                query: "path/to/file".to_string(),
                ..SearchFilters::default()
            })
            .expect("search with punctuation");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].session.id, "alpha");

        fs::remove_dir_all(dir).ok();
    }
}
