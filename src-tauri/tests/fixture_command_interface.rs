use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cc_session_lib::command_test_helpers;
use cc_session_lib::db::Database;
use cc_session_lib::indexer::Indexer;
use cc_session_lib::models::{Provider, ProviderSnapshot, SessionDetail, SessionMeta, TrashMeta};
use cc_session_lib::provider;
use tempfile::TempDir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvOverride {
    key: &'static str,
    original: Option<String>,
}

impl EnvOverride {
    fn set(key: &'static str, value: &Path) -> Self {
        let original = env::var(key).ok();
        env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvOverride {
    fn drop(&mut self) {
        if let Some(value) = &self.original {
            env::set_var(self.key, value);
        } else {
            env::remove_var(self.key);
        }
    }
}

fn override_home_env(home: &Path) -> Vec<EnvOverride> {
    let mut guards = vec![
        EnvOverride::set("HOME", home),
        EnvOverride::set("USERPROFILE", home),
    ];

    if cfg!(windows) {
        let home_str = home.to_string_lossy().to_string();
        let mut parts = home_str.splitn(2, ':');
        if let (Some(drive), Some(rest)) = (parts.next(), parts.next()) {
            guards.push(EnvOverride {
                key: "HOMEDRIVE",
                original: env::var("HOMEDRIVE").ok(),
            });
            env::set_var("HOMEDRIVE", format!("{drive}:"));

            guards.push(EnvOverride {
                key: "HOMEPATH",
                original: env::var("HOMEPATH").ok(),
            });
            env::set_var("HOMEPATH", rest);
        }
    }

    guards
}

fn override_data_env(data_home: &Path) -> Vec<EnvOverride> {
    vec![
        EnvOverride::set("XDG_DATA_HOME", data_home),
        EnvOverride::set("LOCALAPPDATA", data_home),
        EnvOverride::set("APPDATA", data_home),
    ]
}

fn normalized(path: &str) -> String {
    path.replace('\\', "/")
}

fn write_claude_fixture(home: &Path) -> PathBuf {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("claude_session.jsonl");
    let project_dir = home
        .join(".claude")
        .join("projects")
        .join("fixture-project");
    fs::create_dir_all(&project_dir).expect("create project dir");
    let target = project_dir.join("fixture-session-001.jsonl");
    fs::copy(&fixture, &target).expect("copy fixture");
    target
}

#[test]
fn command_interface_uses_fixture_provider_data_without_manual_deletes() {
    let _env_lock = ENV_LOCK.lock().expect("env lock");
    let home = TempDir::new().expect("home temp dir");
    let data_home = TempDir::new().expect("data temp dir");
    let db_dir = TempDir::new().expect("db temp dir");
    let _home_guards = override_home_env(home.path());
    let _data_guards = override_data_env(data_home.path());

    let source_path = write_claude_fixture(home.path());
    let db = Arc::new(Database::open(db_dir.path()).expect("open temp db"));
    let indexer = Indexer::new(Arc::clone(&db), provider::all_runtimes());

    let indexed = indexer.reindex().expect("rebuild index");
    assert_eq!(indexed, 1);

    let snapshots: Vec<ProviderSnapshot> =
        command_test_helpers::get_provider_snapshots(&db).expect("provider snapshots");
    let claude_snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.key == Provider::Claude)
        .expect("claude snapshot");
    assert!(claude_snapshot.exists);
    assert_eq!(claude_snapshot.session_count, 1);
    assert!(normalized(&claude_snapshot.path).contains(".claude/projects"));

    let recent: Vec<SessionMeta> = db.list_recent_sessions(10).expect("list recent");
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, "fixture-session-001");
    assert_eq!(recent[0].provider, Provider::Claude);
    assert_eq!(recent[0].project_name, "my-project");

    let detail: SessionDetail =
        command_test_helpers::get_session_detail(&db, "fixture-session-001")
            .expect("get session detail");
    assert_eq!(detail.meta.id, "fixture-session-001");
    assert_eq!(detail.meta.project_name, "my-project");
    assert!(!detail.messages.is_empty());

    let resume_command = command_test_helpers::get_resume_command(&db, "fixture-session-001")
        .expect("get resume command");
    assert_eq!(resume_command, "claude --resume fixture-session-001");

    command_test_helpers::trash_session(&db, "fixture-session-001").expect("trash session");
    assert!(
        !source_path.exists(),
        "source should be removed from original location after trash"
    );
    assert!(
        command_test_helpers::get_session_detail(&db, "fixture-session-001").is_err(),
        "trashed session should disappear from indexed detail"
    );

    let trash_entries: Vec<TrashMeta> = command_test_helpers::list_trash().expect("list trash");
    let trash_entry = trash_entries
        .iter()
        .find(|entry| entry.id == "fixture-session-001")
        .expect("fixture session in trash");
    assert_eq!(trash_entry.project_name, "my-project");
    assert!(!trash_entry.trash_file.is_empty());

    command_test_helpers::restore_session(&db, "fixture-session-001").expect("restore session");
    assert!(
        source_path.exists(),
        "restored session should move back to original source path"
    );
    let restored: SessionDetail =
        command_test_helpers::get_session_detail(&db, "fixture-session-001")
            .expect("restored session detail");
    assert_eq!(restored.meta.project_name, "my-project");

    command_test_helpers::delete_session(&db, "fixture-session-001").expect("delete session");
    assert!(
        !source_path.exists(),
        "direct delete should permanently remove source file"
    );
    assert!(
        command_test_helpers::get_session_detail(&db, "fixture-session-001").is_err(),
        "deleted session should be unavailable"
    );
}
