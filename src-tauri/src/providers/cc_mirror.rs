use std::fs;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::Deserialize;

use crate::models::{Message, Provider, SessionMeta};
use crate::provider::{DeletionPlan, ParsedSession, ProviderError, SessionProvider};
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
    fn avatar_svg(&self) -> &'static str {
        r##"<svg width="24" height="24" viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path d="M4.709 15.955l4.72-2.647.08-.23-.08-.128H9.2l-.79-.048-2.698-.073-2.339-.097-2.266-.122-.571-.121L0 11.784l.055-.352.48-.321.686.06 1.52.103 2.278.158 1.652.097 2.449.255h.389l.055-.157-.134-.098-.103-.097-2.358-1.596-2.552-1.688-1.336-.972-.724-.491-.364-.462-.158-1.008.656-.722.881.06.225.061.893.686 1.908 1.476 2.491 1.833.365.304.145-.103.019-.073-.164-.274-1.355-2.446-1.446-2.49-.644-1.032-.17-.619a2.97 2.97 0 01-.104-.729L6.283.134 6.696 0l.996.134.42.364.62 1.414 1.002 2.229 1.555 3.03.456.898.243.832.091.255h.158V9.01l.128-1.706.237-2.095.23-2.695.08-.76.376-.91.747-.492.584.28.48.685-.067.444-.286 1.851-.559 2.903-.364 1.942h.212l.243-.242.985-1.306 1.652-2.064.73-.82.85-.904.547-.431h1.033l.76 1.129-.34 1.166-1.064 1.347-.881 1.142-1.264 1.7-.79 1.36.073.11.188-.02 2.856-.606 1.543-.28 1.841-.315.833.388.091.395-.328.807-1.969.486-2.309.462-3.439.813-.042.03.049.061 1.549.146.662.036h1.622l3.02.225.79.522.474.638-.079.485-1.215.62-1.64-.389-3.829-.91-1.312-.329h-.182v.11l1.093 1.068 2.006 1.81 2.509 2.33.127.578-.322.455-.34-.049-2.205-1.657-.851-.747-1.926-1.62h-.128v.17l.444.649 2.345 3.521.122 1.08-.17.353-.608.213-.668-.122-1.374-1.925-1.415-2.167-1.143-1.943-.14.08-.674 7.254-.316.37-.729.28-.607-.461-.322-.747.322-1.476.389-1.924.315-1.53.286-1.9.17-.632-.012-.042-.14.018-1.434 1.967-2.18 2.945-1.726 1.845-.414.164-.717-.37.067-.662.401-.589 2.388-3.036 1.44-1.882.93-1.086-.006-.158h-.055L4.132 18.56l-1.13.146-.487-.456.061-.746.231-.243 1.908-1.312-.006.006z" fill="#f472b6" fill-rule="nonzero"/></svg>"##
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

    fn deletion_plan(&self, meta: &SessionMeta, children: &[SessionMeta]) -> DeletionPlan {
        crate::provider::jsonl_subagents_deletion_plan(meta, children)
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
