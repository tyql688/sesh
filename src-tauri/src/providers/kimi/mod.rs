pub mod parser;
mod tools;

use std::collections::HashMap;
use std::path::PathBuf;

use rayon::prelude::*;
use walkdir::WalkDir;

use crate::models::{Message, Provider};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};

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
            .filter_map(|path| self.parse_session_file(path, &project_map))
            .collect();

        Ok(sessions)
    }

    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        let path = PathBuf::from(source_path);
        let project_map = self.build_project_map();
        Ok(self
            .parse_session_file(&path, &project_map)
            .into_iter()
            .collect())
    }

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);
        let project_map = self.build_project_map();

        let parsed = self
            .parse_session_file(&path, &project_map)
            .ok_or_else(|| ProviderError::Parse("failed to parse kimi session file".to_string()))?;

        Ok(parsed.messages)
    }
}
