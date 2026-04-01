use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::Deserialize;

use crate::models::{Message, Provider};
use crate::provider::{ParsedSession, ProviderError, SessionProvider};
use crate::providers::claude::parser;

pub struct Descriptor;
impl crate::provider::ProviderDescriptor for Descriptor {
    fn owns_source_path(&self, source_path: &str) -> bool {
        let p = source_path.replace('\\', "/");
        p.contains("/.cc-mirror/") && p.contains("/config/projects/")
    }
    fn resume_command(&self, session_id: &str, variant_name: Option<&str>) -> Option<String> {
        variant_name.map(|name| format!("{name} --resume {session_id}"))
    }
    fn display_key(&self, variant_name: Option<&str>) -> String {
        match variant_name {
            Some(vn) => format!("cc-mirror:{vn}"),
            None => "cc-mirror".into(),
        }
    }
    fn try_parse_display_key(&self, display_key: &str) -> Option<String> {
        display_key
            .strip_prefix("cc-mirror:")
            .map(|v| v.to_string())
    }
    fn sort_order(&self) -> u32 {
        1
    }
    fn color(&self) -> &'static str {
        "#f472b6"
    }
    fn cli_command(&self) -> &'static str {
        ""
    }
}

#[derive(Debug, Deserialize)]
struct VariantMeta {
    name: String,
}

#[derive(Debug)]
struct Variant {
    name: String,
    projects_dir: PathBuf,
}

pub struct CcMirrorProvider {
    variants: Vec<Variant>,
}

impl CcMirrorProvider {
    pub fn new() -> Option<Self> {
        let home_dir = dirs::home_dir()?;
        let mirror_root = home_dir.join(".cc-mirror");
        if !mirror_root.exists() {
            return Some(Self {
                variants: Vec::new(),
            });
        }

        let mut variants = Vec::new();
        let entries = fs::read_dir(&mirror_root).ok()?;
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let variant_json = dir.join("variant.json");
            if !variant_json.exists() {
                continue;
            }
            let content = match fs::read_to_string(&variant_json) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let meta: VariantMeta = match serde_json::from_str(&content) {
                Ok(m) => m,
                Err(e) => {
                    log::warn!("failed to parse '{}': {}", variant_json.display(), e);
                    continue;
                }
            };
            // Sanitize variant name: only allow alphanumeric, hyphens, underscores
            let safe_name: String = meta
                .name
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if safe_name.is_empty() {
                continue;
            }
            let projects_dir = dir.join("config").join("projects");
            variants.push(Variant {
                name: safe_name,
                projects_dir,
            });
        }

        Some(Self { variants })
    }

    fn collect_jsonl_files(&self) -> Vec<(PathBuf, String)> {
        let mut all_files = Vec::new();
        for variant in &self.variants {
            if !variant.projects_dir.exists() {
                continue;
            }
            let project_dirs = match fs::read_dir(&variant.projects_dir) {
                Ok(d) => d,
                Err(_) => continue,
            };
            for entry in project_dirs.flatten() {
                let project_dir = entry.path();
                if !project_dir.is_dir() {
                    continue;
                }
                let files = match fs::read_dir(&project_dir) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                for file_entry in files.flatten() {
                    let file_path = file_entry.path();
                    let is_dir = file_path.is_dir();
                    if file_path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                        all_files.push((file_path, variant.name.clone()));
                    } else if is_dir {
                        let subagents_dir = file_path.join("subagents");
                        if subagents_dir.is_dir() {
                            if let Ok(sub_entries) = fs::read_dir(&subagents_dir) {
                                for sub_entry in sub_entries.flatten() {
                                    let sub_path = sub_entry.path();
                                    if sub_path.extension().and_then(|e| e.to_str())
                                        == Some("jsonl")
                                    {
                                        all_files.push((sub_path, variant.name.clone()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        all_files
    }

    fn variant_name_from_path(&self, source_path: &str) -> Option<String> {
        let normalized = source_path.replace('\\', "/");
        for variant in &self.variants {
            let prefix = variant.projects_dir.to_string_lossy().replace('\\', "/");
            if normalized.starts_with(&prefix) {
                return Some(variant.name.clone());
            }
        }
        None
    }
}

impl SessionProvider for CcMirrorProvider {
    fn provider(&self) -> Provider {
        Provider::CcMirror
    }

    fn watch_paths(&self) -> Vec<PathBuf> {
        self.variants
            .iter()
            .map(|v| v.projects_dir.clone())
            .collect()
    }

    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError> {
        let all_files = self.collect_jsonl_files();
        let sessions: Vec<ParsedSession> = all_files
            .par_iter()
            .filter_map(|(path, variant_name)| {
                let mut parsed = parser::parse_session_file(path)?;
                parsed.meta.provider = Provider::CcMirror;
                parsed.meta.variant_name = Some(variant_name.clone());
                Some(parsed)
            })
            .collect();
        Ok(sessions)
    }

    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        let path = PathBuf::from(source_path);
        let variant_name = self.variant_name_from_path(source_path);
        Ok(parser::parse_session_file(&path)
            .map(|mut parsed| {
                parsed.meta.provider = Provider::CcMirror;
                parsed.meta.variant_name = variant_name.clone();
                parsed
            })
            .into_iter()
            .collect())
    }

    fn load_messages(
        &self,
        _session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError> {
        let path = PathBuf::from(source_path);
        let parsed = parser::parse_session_file(&path)
            .ok_or_else(|| ProviderError::Parse("failed to parse session file".to_string()))?;

        // Resolve persisted outputs only at display time, not during indexing
        let messages = parsed
            .messages
            .into_iter()
            .map(|mut msg| {
                msg.content = parser::resolve_persisted_outputs(&msg.content);
                msg
            })
            .collect();

        Ok(messages)
    }
}
