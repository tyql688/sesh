mod queries;
mod row_mapper;
mod sync;

pub use queries::provider_to_str_pub;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

pub struct Database {
    write_conn: Mutex<Connection>,
    read_conn: Mutex<Connection>,
    db_path: std::path::PathBuf,
}

impl Database {
    /// Acquire the write connection lock, recovering from mutex poisoning.
    fn lock_write(&self) -> Result<std::sync::MutexGuard<'_, Connection>, rusqlite::Error> {
        self.write_conn.lock().map_err(|_| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                Some("write mutex poisoned".to_string()),
            )
        })
    }

    /// Acquire the read connection lock, recovering from mutex poisoning.
    fn lock_read(&self) -> Result<std::sync::MutexGuard<'_, Connection>, rusqlite::Error> {
        self.read_conn.lock().map_err(|_| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_LOCKED),
                Some("read mutex poisoned".to_string()),
            )
        })
    }
}

impl Database {
    pub fn with_transaction<T, F>(&self, f: F) -> Result<T, rusqlite::Error>
    where
        F: FnOnce(&Connection) -> Result<T, rusqlite::Error>,
    {
        let conn = self.lock_write()?;
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

        let write_conn = Connection::open(&db_path)?;

        write_conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -2000;",
        )?;

        write_conn.execute_batch(
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
            let mut stmt = write_conn.prepare(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'title_custom'",
            )?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            count > 0
        };
        if !has_title_custom {
            write_conn.execute_batch(
                "ALTER TABLE sessions ADD COLUMN title_custom INTEGER NOT NULL DEFAULT 0;",
            )?;
        }

        // Migration: add is_sidechain column if not exists
        let has_is_sidechain: bool = {
            let mut stmt = write_conn.prepare(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'is_sidechain'",
            )?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            count > 0
        };
        if !has_is_sidechain {
            write_conn.execute_batch(
                "ALTER TABLE sessions ADD COLUMN is_sidechain INTEGER NOT NULL DEFAULT 0;",
            )?;
        }

        // Migration: add variant_name column if not exists
        let has_variant_name: bool = {
            let mut stmt = write_conn.prepare(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name = 'variant_name'",
            )?;
            let count: i64 = stmt.query_row([], |row| row.get(0))?;
            count > 0
        };
        if !has_variant_name {
            write_conn.execute_batch("ALTER TABLE sessions ADD COLUMN variant_name TEXT;")?;
        }

        let read_conn = Connection::open(&db_path)?;
        read_conn.pragma_update(None, "journal_mode", "WAL")?;
        read_conn.pragma_update(None, "query_only", "ON")?;

        Ok(Self {
            write_conn: Mutex::new(write_conn),
            read_conn: Mutex::new(read_conn),
            db_path,
        })
    }
}
