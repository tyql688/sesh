pub mod parser;

use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;

use crate::models::{Provider, SessionMeta};
use crate::provider::{
    DeletionPlan, FileAction, LoadedSession, ParsedSession, ProviderError, SessionProvider,
};

pub struct Descriptor;
impl crate::provider::ProviderDescriptor for Descriptor {
    fn owns_source_path(&self, source_path: &str) -> bool {
        let p = source_path.replace('\\', "/");
        p.contains("/.copilot/session-state/") && p.ends_with("/events.jsonl")
    }
    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("copilot --resume={session_id}"))
    }
    fn display_key(&self, _variant_name: Option<&str>) -> String {
        "copilot".into()
    }
    fn sort_order(&self) -> u32 {
        8
    }
    fn color(&self) -> &'static str {
        "#171717"
    }
    fn cli_command(&self) -> &'static str {
        "copilot"
    }
    fn avatar_svg(&self) -> &'static str {
        r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M23.922 16.997C23.061 18.492 18.063 22.02 12 22.02S.939 18.492.078 16.997A.6.6 0 0 1 0 16.741v-2.869a1 1 0 0 1 .053-.22c.372-.935 1.347-2.292 2.605-2.656c.167-.429.414-1.055.644-1.517a10 10 0 0 1-.052-1.086c0-1.331.282-2.499 1.132-3.368c.397-.406.89-.717 1.474-.952C7.255 2.937 9.248 1.98 11.978 1.98s4.767.957 6.166 2.093c.584.235 1.077.546 1.474.952c.85.869 1.132 2.037 1.132 3.368c0 .368-.014.733-.052 1.086c.23.462.477 1.088.644 1.517c1.258.364 2.233 1.721 2.605 2.656a.8.8 0 0 1 .053.22v2.869a.6.6 0 0 1-.078.256m-11.75-5.992h-.344a4 4 0 0 1-.355.508c-.77.947-1.918 1.492-3.508 1.492c-1.725 0-2.989-.359-3.782-1.259a2 2 0 0 1-.085-.104L4 11.746v6.585c1.435.779 4.514 2.179 8 2.179s6.565-1.4 8-2.179v-6.585l-.098-.104s-.033.045-.085.104c-.793.9-2.057 1.259-3.782 1.259c-1.59 0-2.738-.545-3.508-1.492a4 4 0 0 1-.355-.508m2.328 3.25c.549 0 1 .451 1 1v2c0 .549-.451 1-1 1s-1-.451-1-1v-2c0-.549.451-1 1-1m-5 0c.549 0 1 .451 1 1v2c0 .549-.451 1-1 1s-1-.451-1-1v-2c0-.549.451-1 1-1m3.313-6.185c.136 1.057.403 1.913.878 2.497c.442.544 1.134.938 2.344.938c1.573 0 2.292-.337 2.657-.751c.384-.435.558-1.15.558-2.361c0-1.14-.243-1.847-.705-2.319c-.477-.488-1.319-.862-2.824-1.025c-1.487-.161-2.192.138-2.533.529c-.269.307-.437.808-.438 1.578v.021q0 .397.063.893m-1.626 0q.063-.496.063-.894v-.02c-.001-.77-.169-1.271-.438-1.578c-.341-.391-1.046-.69-2.533-.529c-1.505.163-2.347.537-2.824 1.025c-.462.472-.705 1.179-.705 2.319c0 1.211.175 1.926.558 2.361c.365.414 1.084.751 2.657.751c1.21 0 1.902-.394 2.344-.938c.475-.584.742-1.44.878-2.497" fill="#171717" fill-rule="nonzero"/></svg>"##
    }
}

pub struct CopilotProvider {
    home_dir: PathBuf,
}

impl CopilotProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        Some(Self { home_dir })
    }

    fn session_state_dir(&self) -> PathBuf {
        self.home_dir.join(".copilot").join("session-state")
    }

    fn collect_events_files(&self) -> Vec<PathBuf> {
        let state_dir = self.session_state_dir();
        if !state_dir.exists() {
            return Vec::new();
        }
        let entries = match fs::read_dir(&state_dir) {
            Ok(d) => d,
            Err(e) => {
                log::warn!(
                    "cannot read Copilot session-state dir '{}': {}",
                    state_dir.display(),
                    e
                );
                return Vec::new();
            }
        };
        let mut files = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let session_dir = entry.path();
            if !session_dir.is_dir() {
                continue;
            }
            let events_file = session_dir.join("events.jsonl");
            if events_file.exists() {
                files.push(events_file);
            }
        }
        files
    }
}

impl SessionProvider for CopilotProvider {
    fn provider(&self) -> Provider {
        Provider::Copilot
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        vec![self.session_state_dir()]
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let all_files = self.collect_events_files();
        let sessions: Vec<ParsedSession> = all_files
            .par_iter()
            .filter_map(parser::parse_session_file)
            .collect();
        Ok(sessions)
    }

    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        let path = PathBuf::from(source_path);
        Ok(parser::parse_session_file(&path).into_iter().collect())
    }

    fn deletion_plan(&self, _meta: &SessionMeta, _children: &[SessionMeta]) -> DeletionPlan {
        // Only remove events.jsonl — the session directory contains workspace.yaml,
        // checkpoints, and other Copilot state that must survive trash/restore.
        DeletionPlan {
            file_action: FileAction::Remove,
            child_plans: Vec::new(),
            cleanup_dirs: Vec::new(),
        }
    }

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<LoadedSession, ProviderError> {
        let path = PathBuf::from(source_path);
        let parsed = parser::parse_session_file(&path).ok_or_else(|| {
            ProviderError::Parse(format!(
                "failed to parse Copilot events file '{}'",
                path.display()
            ))
        })?;
        Ok(LoadedSession {
            messages: parsed.messages,
            parse_warning_count: parsed.parse_warning_count,
        })
    }
}
