use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
    Gemini,
    Cursor,
    #[serde(rename = "opencode")]
    OpenCode,
    Kimi,
    #[serde(rename = "cc-mirror")]
    CcMirror,
}

impl Provider {
    pub fn label(&self) -> &'static str {
        match self {
            Provider::Claude => "Claude Code",
            Provider::Codex => "Codex",
            Provider::Gemini => "Gemini",
            Provider::Cursor => "Cursor",
            Provider::OpenCode => "OpenCode",
            Provider::Kimi => "Kimi CLI",
            Provider::CcMirror => "CC-Mirror",
        }
    }

    pub fn key(&self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::Gemini => "gemini",
            Provider::Cursor => "cursor",
            Provider::OpenCode => "opencode",
            Provider::Kimi => "kimi",
            Provider::CcMirror => "cc-mirror",
        }
    }

    pub fn parse(s: &str) -> Option<Provider> {
        match s {
            "claude" => Some(Provider::Claude),
            "codex" => Some(Provider::Codex),
            "gemini" => Some(Provider::Gemini),
            "cursor" => Some(Provider::Cursor),
            "opencode" => Some(Provider::OpenCode),
            "kimi" => Some(Provider::Kimi),
            "cc-mirror" => Some(Provider::CcMirror),
            _ => None,
        }
    }

    /// All known providers in display order.
    pub fn all() -> &'static [Provider] {
        &[
            Provider::Claude,
            Provider::Codex,
            Provider::Gemini,
            Provider::Cursor,
            Provider::OpenCode,
            Provider::Kimi,
            Provider::CcMirror,
        ]
    }

    /// Get the descriptor for this provider (static metadata).
    pub fn descriptor(&self) -> &'static dyn crate::provider::ProviderDescriptor {
        match self {
            Provider::Claude => &crate::providers::claude::Descriptor,
            Provider::Codex => &crate::providers::codex::Descriptor,
            Provider::Gemini => &crate::providers::gemini::Descriptor,
            Provider::Cursor => &crate::providers::cursor::Descriptor,
            Provider::OpenCode => &crate::providers::opencode::Descriptor,
            Provider::Kimi => &crate::providers::kimi::Descriptor,
            Provider::CcMirror => &crate::providers::cc_mirror::Descriptor,
        }
    }

    /// Identify which provider owns a source path.
    pub fn from_source_path(source_path: &str) -> Option<Provider> {
        Provider::all()
            .iter()
            .find(|p| p.descriptor().owns_source_path(source_path))
            .cloned()
    }

    /// Parse a display key (as produced by `descriptor().display_key()`) back to a provider and label.
    /// Handles cc-mirror variants like "cc-mirror:cczai" → (CcMirror, "cczai").
    pub fn parse_display_key(display_key: &str) -> Option<(Provider, String)> {
        // Direct match: covers most providers
        if let Some(p) = Provider::parse(display_key) {
            let label = p.label().to_string();
            return Some((p, label));
        }
        // Custom formats: e.g. "cc-mirror:variant"
        for p in Provider::all() {
            if let Some(label) = p.descriptor().try_parse_display_key(display_key) {
                return Some((p.clone(), label));
            }
        }
        None
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub provider: Provider,
    pub title: String,
    pub project_path: String,
    pub project_name: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: u32,
    pub file_size_bytes: u64,
    pub source_path: String,
    pub is_sidechain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cc_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<String>,
    pub token_usage: Option<TokenUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDetail {
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub node_type: TreeNodeType,
    pub children: Vec<TreeNode>,
    pub count: u32,
    pub provider: Option<Provider>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub is_sidechain: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeNodeType {
    Provider,
    Project,
    Session,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub session: SessionMeta,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub session_count: u64,
    pub db_size_bytes: u64,
    pub last_index_time: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub key: String,
    pub label: String,
    pub path: String,
    pub exists: bool,
    pub session_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilters {
    pub query: String,
    pub provider: Option<String>,
    pub project: Option<String>,
    pub after: Option<i64>,
    pub before: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashMeta {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub original_path: String,
    pub trashed_at: i64,
    pub trash_file: String,
    #[serde(default)]
    pub project_name: String,
}
