mod images;
pub mod parser;

use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;

use crate::models::{Message, Provider};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};

pub struct ClaudeProvider {
    home_dir: PathBuf,
}

impl ClaudeProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        Some(Self { home_dir })
    }

    /// Thin wrapper for tests — delegates to the free function in parser module.
    pub fn parse_session(&self, path: &std::path::Path) -> Option<crate::provider::ParsedSession> {
        let buf = path.to_path_buf();
        parser::parse_session_file(&buf)
    }

    fn projects_dir(&self) -> PathBuf {
        self.home_dir.join(".claude").join("projects")
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
                    "cannot read Claude projects dir '{}': {}",
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
            let files = match fs::read_dir(&project_dir) {
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

impl SessionProvider for ClaudeProvider {
    fn provider(&self) -> Provider {
        Provider::Claude
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

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);

        let parsed = parser::parse_session_file(&path)
            .ok_or_else(|| ProviderError::Parse("failed to parse session file".to_string()))?;

        Ok(parsed.messages)
    }
}
