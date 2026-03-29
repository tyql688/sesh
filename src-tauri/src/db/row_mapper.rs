use crate::models::{Provider, SessionMeta};

pub fn row_to_session_meta(row: &rusqlite::Row) -> rusqlite::Result<SessionMeta> {
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
}

fn str_to_provider(s: &str) -> Provider {
    match Provider::parse(s) {
        Some(p) => p,
        None => {
            eprintln!("warning: unknown provider '{}', defaulting to Claude", s);
            Provider::Claude
        }
    }
}
