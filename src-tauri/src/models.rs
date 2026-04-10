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
    Qwen,
    Copilot,
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
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
pub struct ProviderSnapshot {
    pub key: Provider,
    pub label: String,
    pub color: String,
    pub sort_order: u32,
    pub watch_strategy: crate::provider::WatchStrategy,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_name: Option<String>,
    /// Parent session ID (set on child entries so restore can group them).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStats {
    pub total_sessions: u64,
    pub total_turns: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_write_tokens: u64,
    pub total_cost: f64,
    pub cache_hit_rate: f64,
    pub daily_usage: Vec<DailyUsage>,
    pub model_costs: Vec<ModelCost>,
    pub project_costs: Vec<ProjectCost>,
    pub recent_sessions: Vec<SessionCostRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyUsage {
    pub date: String,
    pub provider: String,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    pub model: String,
    pub turns: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCost {
    pub project: String,
    pub project_path: String,
    pub provider: String,
    pub sessions: u64,
    pub turns: u64,
    pub tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCostRow {
    pub id: String,
    pub project: String,
    pub project_path: String,
    pub provider: String,
    pub model: String,
    pub updated_at: i64,
    pub turns: u64,
    pub tokens: u64,
    pub cost: f64,
}
