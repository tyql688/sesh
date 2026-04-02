//! Integration test: trash → restore → trash → empty-trash lifecycle
//! using REAL local session data for Kimi, Codex, Cursor, OpenCode.
//!
//! Run: cd src-tauri && cargo test --test trash_lifecycle_test -- --nocapture

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cc_session_lib::db::Database;
use cc_session_lib::models::{Provider, SessionMeta};
use cc_session_lib::provider::{self, make_provider};
use cc_session_lib::trash_state;

// ── helpers ──────────────────────────────────────────────────────────────────

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("cc-session")
}

fn db() -> Arc<Database> {
    Arc::new(Database::open(&data_dir()).expect("open db"))
}

/// Query sessions by provider from DB.
fn query_sessions(db: &Database, provider: &Provider) -> Vec<SessionMeta> {
    db.list_sessions()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.provider == *provider)
        .collect()
}

fn parents(sessions: &[SessionMeta]) -> Vec<&SessionMeta> {
    sessions.iter().filter(|s| s.parent_id.is_none()).collect()
}

fn children(sessions: &[SessionMeta]) -> Vec<&SessionMeta> {
    sessions.iter().filter(|s| s.parent_id.is_some()).collect()
}

/// Call trash_session logic without Tauri State wrapper.
fn do_trash(db: &Database, session_id: &str, provider_str: &str) -> Result<(), String> {
    let trash_dir = trash_state::trash_dir()?;
    let meta = db
        .get_session(session_id)
        .map_err(|e| e.to_string())?
        .ok_or("session not found in DB")?;

    let provider_enum =
        Provider::parse(provider_str).ok_or_else(|| format!("bad provider: {provider_str}"))?;
    let provider_impl =
        make_provider(&provider_enum).ok_or("cannot create provider")?;

    let db_children = db.get_child_sessions(session_id).unwrap_or_default();
    let plan = provider_impl.deletion_plan(&meta, &db_children);

    let now = chrono::Utc::now().timestamp();
    let meta_path = trash_state::trash_meta_path(&trash_dir);
    let mut entries = trash_state::read_trash_meta(&meta_path);
    let records = provider::execute_trash(&plan, &meta, provider_str, &trash_dir, now)?;
    entries.extend(records);

    let shared_path = trash_state::shared_deletions_path(&trash_dir);
    if plan.file_action == provider::FileAction::Shared {
        trash_state::add_shared_deletion(&shared_path, &meta.id, provider_str, &meta.source_path)?;
    }

    trash_state::atomic_write_json(&meta_path, &entries)?;
    db.delete_session(session_id).map_err(|e| e.to_string())?;
    Ok(())
}

/// Call restore_session logic without Tauri State wrapper.
fn do_restore(db: &Database, trash_id: &str) -> Result<(), String> {
    let trash_dir = trash_state::trash_dir()?;
    let meta_path = trash_state::trash_meta_path(&trash_dir);
    let shared_path = trash_state::shared_deletions_path(&trash_dir);

    let entries = trash_state::read_trash_meta(&meta_path);
    let entry = match entries.iter().find(|e| e.id == trash_id) {
        Some(e) => e.clone(),
        None => return Ok(()), // already restored
    };

    // Session directory prefix for matching file-based children
    let session_dir_prefix = {
        let p = std::path::Path::new(&entry.original_path);
        let dir = p.with_extension("");
        format!("{}/", dir.display())
    };

    let mut child_entries = Vec::new();
    let remaining: Vec<_> = entries
        .into_iter()
        .filter(|e| {
            if e.id == trash_id {
                return false;
            }
            // Embedded children (Kimi)
            if e.trash_file.is_empty()
                && !entry.trash_file.is_empty()
                && e.original_path == entry.original_path
                && e.provider == entry.provider
            {
                return false;
            }
            // File-based children (Codex, Cursor, Claude)
            if !e.trash_file.is_empty()
                && e.provider == entry.provider
                && e.original_path.starts_with(&session_dir_prefix)
            {
                child_entries.push(e.clone());
                return false;
            }
            true
        })
        .collect();

    let provider_enum = Provider::parse(&entry.provider);
    let provider_impl = provider_enum.as_ref().and_then(make_provider);
    let action = provider_impl
        .as_ref()
        .map(|p| p.restore_action(&entry))
        .unwrap_or_else(|| provider::infer_restore_action(&entry));

    let needs_sync = provider::execute_restore(&action, &entry, &trash_dir, &remaining)?;

    // Restore file-based children
    for child in &child_entries {
        let child_action = provider_impl
            .as_ref()
            .map(|p| p.restore_action(child))
            .unwrap_or_else(|| provider::infer_restore_action(child));
        let _ = provider::execute_restore(&child_action, child, &trash_dir, &remaining);
    }

    if action == provider::RestoreAction::UndoSharedDeletion {
        trash_state::remove_shared_deletion(&shared_path, &entry.id, &entry.original_path)?;
    }

    trash_state::atomic_write_json(&meta_path, &remaining)?;

    if needs_sync {
        if let Some(prov) = &provider_enum {
            if let Some(p) = make_provider(prov) {
                let sessions = p.scan_source(&entry.original_path).unwrap_or_default();
                let _ = db.sync_source_snapshot(prov, &entry.original_path, &sessions);
            }
        }
    }
    Ok(())
}

/// Call empty_trash logic (no Tauri state needed).
fn do_empty_trash() -> Result<(), String> {
    // empty_trash is a pub #[tauri::command] fn — callable directly.
    cc_session_lib::commands::trash::empty_trash()
}

fn kimi_session_dirs() -> Vec<PathBuf> {
    let dir = dirs::home_dir().unwrap().join(".kimi/sessions");
    if !dir.exists() {
        return vec![];
    }
    let mut out = vec![];
    for h in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if h.path().is_dir() {
            for s in std::fs::read_dir(h.path()).into_iter().flatten().flatten() {
                if s.path().is_dir() {
                    out.push(s.path());
                }
            }
        }
    }
    out
}

fn cursor_store_dirs() -> Vec<PathBuf> {
    let dir = dirs::home_dir().unwrap().join(".cursor/chats");
    if !dir.exists() {
        return vec![];
    }
    let mut out = vec![];
    for h in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        if h.path().is_dir() {
            for s in std::fs::read_dir(h.path()).into_iter().flatten().flatten() {
                if s.path().is_dir() && s.path().join("store.db").exists() {
                    out.push(s.path());
                }
            }
        }
    }
    out
}

// ── test ─────────────────────────────────────────────────────────────────────

#[test]
fn lifecycle_trash_restore_delete() {
    let db = db();
    let test_providers = [Provider::Kimi, Provider::Codex, Provider::Cursor, Provider::OpenCode];

    // ── PHASE 1: Initial state ──────────────────────────────────────────────
    println!("\n===== PHASE 1: Initial State =====");
    struct Snap {
        provider_key: String,
        parent_ids: Vec<String>,
        child_count: usize,
        child_titles: Vec<String>,
        source_files: Vec<PathBuf>,
    }
    let mut snaps: Vec<Snap> = vec![];

    for prov in &test_providers {
        let sess = query_sessions(&db, prov);
        let p = parents(&sess);
        let c = children(&sess);
        let srcs: Vec<PathBuf> = sess
            .iter()
            .map(|s| PathBuf::from(&s.source_path))
            .filter(|p| p.exists())
            .collect();
        println!(
            "  {}: {} parents, {} children, {} source files on disk",
            prov.key(), p.len(), c.len(), srcs.len()
        );
        assert!(!p.is_empty(), "{}: no parent sessions — generate data first", prov.key());
        snaps.push(Snap {
            provider_key: prov.key().to_string(),
            parent_ids: p.iter().map(|s| s.id.clone()).collect(),
            child_count: c.len(),
            child_titles: c.iter().map(|s| s.title.clone()).collect(),
            source_files: srcs,
        });
    }

    // ── PHASE 2: Trash ──────────────────────────────────────────────────────
    println!("\n===== PHASE 2: Trash =====");
    for snap in &snaps {
        for pid in &snap.parent_ids {
            do_trash(&db, pid, &snap.provider_key)
                .unwrap_or_else(|e| panic!("{}: trash failed: {e}", snap.provider_key));
        }
    }
    for prov in &test_providers {
        let sess = query_sessions(&db, prov);
        assert!(sess.is_empty(), "{}: DB should be empty after trash", prov.key());
        println!("  {}: DB empty ✓", prov.key());
    }

    // ── PHASE 3: Restore ────────────────────────────────────────────────────
    println!("\n===== PHASE 3: Restore =====");
    for snap in &snaps {
        for pid in &snap.parent_ids {
            do_restore(&db, pid)
                .unwrap_or_else(|e| panic!("{}: restore failed: {e}", snap.provider_key));
        }
    }
    for (i, prov) in test_providers.iter().enumerate() {
        let sess = query_sessions(&db, prov);
        let p = parents(&sess);
        let c = children(&sess);
        let snap = &snaps[i];

        assert_eq!(
            p.len(), snap.parent_ids.len(),
            "{}: parent count mismatch after restore", prov.key()
        );
        assert_eq!(
            c.len(), snap.child_count,
            "{}: child count mismatch after restore ({} vs {})", prov.key(), c.len(), snap.child_count
        );
        println!("  {}: {} parents, {} children ✓", prov.key(), p.len(), c.len());

        // Verify child titles preserved (especially Kimi subagent names from meta.json)
        if *prov == Provider::Kimi {
            let mut restored_titles: Vec<String> = c.iter().map(|s| s.title.clone()).collect();
            let mut original_titles = snap.child_titles.clone();
            restored_titles.sort();
            original_titles.sort();
            assert_eq!(
                restored_titles, original_titles,
                "Kimi child titles changed after restore!\n  before: {:?}\n  after:  {:?}",
                original_titles, restored_titles
            );
            println!("  Kimi child titles preserved ✓");
        }

        // Verify source files still exist
        for f in &snap.source_files {
            assert!(f.exists(), "{}: source file gone after restore: {}", prov.key(), f.display());
        }
        println!("  {}: source files intact ✓", prov.key());
    }

    // ── PHASE 4: Trash again + Empty trash ──────────────────────────────────
    println!("\n===== PHASE 4: Trash + Empty =====");
    for prov in &test_providers {
        let sess = query_sessions(&db, prov);
        for s in parents(&sess) {
            do_trash(&db, &s.id, prov.key())
                .unwrap_or_else(|e| panic!("{}: second trash failed: {e}", prov.key()));
        }
    }
    do_empty_trash().expect("empty_trash failed");
    println!("  empty_trash completed");

    // ── PHASE 5: Final verification ─────────────────────────────────────────
    println!("\n===== PHASE 5: Final Verification =====");

    for prov in &test_providers {
        let sess = query_sessions(&db, prov);
        assert!(sess.is_empty(), "{}: DB not empty after empty_trash", prov.key());
        println!("  {}: DB clean ✓", prov.key());
    }

    // Kimi: no orphan session directories
    let kimi_dirs = kimi_session_dirs();
    assert!(kimi_dirs.is_empty(), "Kimi orphan dirs: {:?}", kimi_dirs);
    println!("  Kimi: no orphan dirs ✓");

    // Cursor: no store.db directories
    let cursor_stores = cursor_store_dirs();
    assert!(cursor_stores.is_empty(), "Cursor store.db dirs: {:?}", cursor_stores);
    println!("  Cursor: no store.db dirs ✓");

    // File-based providers: source files should be gone
    for snap in &snaps {
        if snap.provider_key == "opencode" {
            continue; // DB-based, file always exists
        }
        for f in &snap.source_files {
            assert!(
                !f.exists(),
                "{}: source file still exists: {}",
                snap.provider_key, f.display()
            );
        }
        println!("  {}: source files cleaned ✓", snap.provider_key);
    }

    // OpenCode: sessions purged from opencode.db
    let oc_db = dirs::home_dir().unwrap().join(".local/share/opencode/opencode.db");
    if oc_db.exists() {
        let conn = rusqlite::Connection::open(&oc_db).unwrap();
        let count: u64 = conn
            .query_row("SELECT COUNT(*) FROM session", [], |row| row.get(0))
            .unwrap_or(0);
        println!("  OpenCode: {} sessions in opencode.db (should be 0)", count);
        assert_eq!(count, 0, "OpenCode sessions not purged from opencode.db");
        println!("  OpenCode: opencode.db clean ✓");
    }

    // Trash metadata should be empty
    let trash_dir = trash_state::trash_dir().unwrap();
    let meta_path = trash_state::trash_meta_path(&trash_dir);
    let remaining = trash_state::read_trash_meta(&meta_path);
    assert!(remaining.is_empty(), "Trash metadata not empty: {} entries", remaining.len());
    println!("  Trash metadata empty ✓");

    println!("\n===== ALL TESTS PASSED =====\n");
}
