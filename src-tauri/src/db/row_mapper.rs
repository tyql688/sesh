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
        variant_name: row.get::<_, Option<String>>(11).unwrap_or(None),
        model: row.get::<_, Option<String>>(12).unwrap_or(None),
        cc_version: row.get::<_, Option<String>>(13).unwrap_or(None),
        git_branch: row.get::<_, Option<String>>(14).unwrap_or(None),
    })
}

fn str_to_provider(s: &str) -> Provider {
    match Provider::parse(s) {
        Some(p) => p,
        None => {
            log::warn!("unknown provider '{}', defaulting to Claude", s);
            Provider::Claude
        }
    }
}
