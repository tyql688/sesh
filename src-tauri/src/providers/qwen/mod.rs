pub mod parser;

use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;

use crate::models::{Provider, SessionMeta};
use crate::provider::{DeletionPlan, LoadedSession, ParsedSession, ProviderError, SessionProvider};

pub struct Descriptor;
impl crate::provider::ProviderDescriptor for Descriptor {
    fn owns_source_path(&self, source_path: &str) -> bool {
        let p = source_path.replace('\\', "/");
        p.contains("/.qwen/projects/") && p.contains("/chats/")
    }
    fn resume_command(&self, session_id: &str, _variant_name: Option<&str>) -> Option<String> {
        Some(format!("qwen --resume {session_id}"))
    }
    fn display_key(&self, _variant_name: Option<&str>) -> String {
        "qwen".into()
    }
    fn sort_order(&self) -> u32 {
        7
    }
    fn color(&self) -> &'static str {
        "#6c3cf5"
    }
    fn cli_command(&self) -> &'static str {
        "qwen"
    }
    fn avatar_svg(&self) -> &'static str {
        r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><defs><linearGradient id="qwen-grad" x1="0%" y1="0%" x2="100%" y2="0%"><stop offset="0%" stop-color="#6336E7" stop-opacity=".84"/><stop offset="100%" stop-color="#6F69F7" stop-opacity=".84"/></linearGradient></defs><path d="M12.604 1.34c.393.69.784 1.382 1.174 2.075a.18.18 0 00.157.091h5.552c.174 0 .322.11.446.327l1.454 2.57c.19.337.24.478.024.837-.26.43-.513.864-.76 1.3l-.367.658c-.106.196-.223.28-.04.512l2.652 4.637c.172.301.111.494-.043.77-.437.785-.882 1.564-1.335 2.34-.159.272-.352.375-.68.37-.777-.016-1.552-.01-2.327.016a.099.099 0 00-.081.05 575.097 575.097 0 01-2.705 4.74c-.169.293-.38.363-.725.364-.997.003-2.002.004-3.017.002a.537.537 0 01-.465-.271l-1.335-2.323a.09.09 0 00-.083-.049H4.982c-.285.03-.553-.001-.805-.092l-1.603-2.77a.543.543 0 01-.002-.54l1.207-2.12a.198.198 0 000-.197 550.951 550.951 0 01-1.875-3.272l-.79-1.395c-.16-.31-.173-.496.095-.965.465-.813.927-1.625 1.387-2.436.132-.234.304-.334.584-.335a338.3 338.3 0 012.589-.001.124.124 0 00.107-.063l2.806-4.895a.488.488 0 01.422-.246c.524-.001 1.053 0 1.583-.006L11.704 1c.341-.003.724.032.9.34zm-3.432.403a.06.06 0 00-.052.03L6.254 6.788a.157.157 0 01-.135.078H3.253c-.056 0-.07.025-.041.074l5.81 10.156c.025.042.013.062-.034.063l-2.795.015a.218.218 0 00-.2.116l-1.32 2.31c-.044.078-.021.118.068.118l5.716.008c.046 0 .08.02.104.061l1.403 2.454c.046.081.092.082.139 0l5.006-8.76.783-1.382a.055.055 0 01.096 0l1.424 2.53a.122.122 0 00.107.062l2.763-.02a.04.04 0 00.035-.02.041.041 0 000-.04l-2.9-5.086a.108.108 0 010-.113l.293-.507 1.12-1.977c.024-.041.012-.062-.035-.062H9.2c-.059 0-.073-.026-.043-.077l1.434-2.505a.107.107 0 000-.114L9.225 1.774a.06.06 0 00-.053-.031zm6.29 8.02c.046 0 .058.02.034.06l-.832 1.465-2.613 4.585a.056.056 0 01-.05.029.058.058 0 01-.05-.029L8.498 9.841c-.02-.034-.01-.052.028-.054l.216-.012 6.722-.012z" fill="url(#qwen-grad)" fill-rule="nonzero"/></svg>"##
    }
}

pub struct QwenProvider {
    home_dir: PathBuf,
}

impl QwenProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        Some(Self { home_dir })
    }

    fn projects_dir(&self) -> PathBuf {
        self.home_dir.join(".qwen").join("projects")
    }

    fn collect_jsonl_files(&self) -> Vec<PathBuf> {
        let projects_dir = self.projects_dir();
        if !projects_dir.exists() {
            return Vec::new();
        }
        let mut all_files: Vec<PathBuf> = Vec::new();
        let project_dirs = match fs::read_dir(&projects_dir) {
            Ok(d) => d,
            Err(e) => {
                log::warn!(
                    "cannot read Qwen projects dir '{}': {}",
                    projects_dir.display(),
                    e
                );
                return Vec::new();
            }
        };
        for entry in project_dirs {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let project_dir = entry.path();
            if !project_dir.is_dir() {
                continue;
            }
            let chats_dir = project_dir.join("chats");
            if !chats_dir.is_dir() {
                continue;
            }
            let files = match fs::read_dir(&chats_dir) {
                Ok(f) => f,
                Err(_) => continue,
            };
            for file_entry in files {
                let file_entry = match file_entry {
                    Ok(e) => e,
                    Err(_) => continue,
                };
                let file_path = file_entry.path();
                if file_path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    all_files.push(file_path);
                }
            }
        }
        all_files
    }
}

impl SessionProvider for QwenProvider {
    fn provider(&self) -> Provider {
        Provider::Qwen
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        vec![self.projects_dir()]
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let all_files = self.collect_jsonl_files();
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
        // Qwen has no subagent files — simple single-file deletion.
        DeletionPlan {
            file_action: crate::provider::FileAction::Remove,
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
                "failed to parse Qwen session file '{}'",
                path.display()
            ))
        })?;
        Ok(LoadedSession {
            messages: parsed.messages,
            parse_warning_count: parsed.parse_warning_count,
        })
    }
}
