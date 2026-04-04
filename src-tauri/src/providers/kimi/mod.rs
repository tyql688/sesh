pub mod parser;
mod tools;

use std::collections::HashMap;
use std::path::PathBuf;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::models::{Message, Provider, SessionMeta};
use crate::provider::{
    ChildPlan, DeletionPlan, FileAction, ParsedSession, ProviderError, SessionProvider,
};

pub struct Descriptor;
impl crate::provider::ProviderDescriptor for Descriptor {
    fn owns_source_path(&self, source_path: &str) -> bool {
        source_path.replace('\\', "/").contains("/.kimi/sessions/")
    }
    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("kimi --session {session_id}"))
    }
    fn display_key(&self, _variant_name: Option<&str>) -> String {
        "kimi".into()
    }
    fn sort_order(&self) -> u32 {
        6
    }
    fn color(&self) -> &'static str {
        "#1783ff"
    }
    fn cli_command(&self) -> &'static str {
        "kimi"
    }
    fn avatar_svg(&self) -> &'static str {
        r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M19.738 5.776c.163-.209.306-.4.457-.585.07-.087.064-.153-.004-.244-.655-.861-.717-1.817-.34-2.787.283-.73.909-1.072 1.674-1.145.477-.045.945.004 1.379.236.57.305.902.77 1.01 1.412.086.512.07 1.012-.075 1.508-.257.878-.888 1.333-1.753 1.448-.718.096-1.446.108-2.17.157-.056.004-.113 0-.178 0z" fill="#027AFF"/><path d="M17.962 1.844h-4.326l-3.425 7.81H5.369V1.878H1.5V22h3.87v-8.477h6.824a3.025 3.025 0 002.743-1.75V22h3.87v-8.477a3.87 3.87 0 00-3.588-3.86v-.01h-2.125a3.94 3.94 0 002.323-2.12l2.545-5.689z" fill="currentColor"/></svg>"##
    }
}

pub struct KimiProvider {
    kimi_dir: PathBuf,
}

impl KimiProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        Some(Self {
            kimi_dir: home_dir.join(".kimi"),
        })
    }

    fn sessions_dir(&self) -> PathBuf {
        self.kimi_dir.join("sessions")
    }

    /// Read ~/.kimi/kimi.json and build a map from MD5 directory name to project path.
    fn build_project_map(&self) -> HashMap<String, String> {
        let config_path = self.kimi_dir.join("kimi.json");
        let mut map = HashMap::new();

        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => return map,
        };

        let json: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return map,
        };

        if let Some(work_dirs) = json.get("work_dirs").and_then(|v| v.as_array()) {
            for entry in work_dirs {
                if let Some(path) = entry.get("path").and_then(|v| v.as_str()) {
                    let md5_hash = format!("{:x}", md5::compute(path.as_bytes()));
                    map.insert(md5_hash, path.to_string());
                }
            }
        }

        map
    }

    fn collect_wire_files(&self) -> Vec<PathBuf> {
        let sessions_dir = self.sessions_dir();
        if !sessions_dir.exists() {
            return Vec::new();
        }

        let mut files = Vec::new();
        for entry in WalkDir::new(&sessions_dir)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            let path = entry.path();
            if path.is_file() && path.file_name().is_some_and(|n| n == "wire.jsonl") {
                files.push(path.to_path_buf());
            }
        }
        files
    }
}

impl SessionProvider for KimiProvider {
    fn provider(&self) -> Provider {
        Provider::Kimi
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        vec![self.sessions_dir()]
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let files = self.collect_wire_files();
        if files.is_empty() {
            return Ok(Vec::new());
        }

        let project_map = self.build_project_map();

        let sessions: Vec<ParsedSession> = files
            .par_iter()
            .flat_map(|path| self.parse_session_with_subagents(path, &project_map))
            .collect();

        Ok(sessions)
    }

    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        let path = PathBuf::from(source_path);
        let project_map = self.build_project_map();
        Ok(self.parse_session_with_subagents(&path, &project_map))
    }

    fn deletion_plan(&self, meta: &SessionMeta, children: &[SessionMeta]) -> DeletionPlan {
        if meta.parent_id.is_some() {
            // Child session (subagent): embedded in parent wire.jsonl
            return DeletionPlan {
                file_action: FileAction::Skip,
                child_plans: Vec::new(),
                cleanup_dirs: Vec::new(),
            };
        }

        // Parent session: remove own file, children are embedded (Skip)
        let child_plans = children
            .iter()
            .map(|c| ChildPlan {
                id: c.id.clone(),
                source_path: c.source_path.clone(),
                title: c.title.clone(),
                file_action: FileAction::Skip,
            })
            .collect();

        // Don't include subagents/ in cleanup_dirs — it contains meta.json
        // files needed for title restoration. permanent_delete_trash and
        // empty_trash handle subagent directory cleanup separately.
        DeletionPlan {
            file_action: FileAction::Remove,
            child_plans,
            cleanup_dirs: Vec::new(),
        }
    }

    fn restore_action(&self, entry: &crate::models::TrashMeta) -> crate::provider::RestoreAction {
        if entry.trash_file.is_empty() {
            // Embedded child — parent file handles it
            crate::provider::RestoreAction::Noop
        } else {
            crate::provider::RestoreAction::MoveBack
        }
    }

    fn load_messages(
        &self,
        session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);
        let project_map = self.build_project_map();

        // Parse parent + subagents, find the one matching session_id
        let sessions = self.parse_session_with_subagents(&path, &project_map);
        let parsed = sessions
            .into_iter()
            .find(|s| s.meta.id == session_id)
            .ok_or_else(|| {
                ProviderError::Parse(format!("session {session_id} not found in {}", source_path))
            })?;

        Ok(parsed.messages)
    }
}
