pub mod parser;
mod tools;

use std::path::PathBuf;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::models::{Message, Provider};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};

pub struct CodexProvider {
    home_dir: PathBuf,
}

impl CodexProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        Some(Self { home_dir })
    }

    fn sessions_dir(&self) -> PathBuf {
        self.home_dir.join(".codex").join("sessions")
    }

    fn archived_sessions_dir(&self) -> PathBuf {
        self.home_dir.join(".codex").join("archived_sessions")
    }

    fn collect_jsonl_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for dir in [self.sessions_dir(), self.archived_sessions_dir()] {
            if !dir.exists() {
                continue;
            }
            for entry in WalkDir::new(&dir)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                    files.push(path.to_path_buf());
                }
            }
        }
        files
    }
}

impl SessionProvider for CodexProvider {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        vec![self.sessions_dir(), self.archived_sessions_dir()]
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let files = self.collect_jsonl_files();
        if files.is_empty() {
            return Ok(Vec::new());
        }

        let sessions: Vec<ParsedSession> = files
            .par_iter()
            .filter_map(|path| self.parse_session_file(path))
            .collect();

        Ok(sessions)
    }

    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        let path = PathBuf::from(source_path);
        Ok(self.parse_session_file(&path).into_iter().collect())
    }

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);

        let parsed = self.parse_session_file(&path).ok_or_else(|| {
            ProviderError::Parse("failed to parse codex session file".to_string())
        })?;

        Ok(parsed.messages)
    }
}
