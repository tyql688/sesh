export type Provider =
  | "claude"
  | "codex"
  | "gemini"
  | "cursor"
  | "opencode"
  | "kimi"
  | "cc-mirror";

export interface SessionMeta {
  id: string;
  provider: Provider;
  title: string;
  project_path: string;
  project_name: string;
  created_at: number;
  updated_at: number;
  message_count: number;
  file_size_bytes: number;
  source_path: string;
  is_sidechain: boolean;
  variant_name?: string;
}

export type MessageRole = "user" | "assistant" | "tool" | "system";

export interface TokenUsage {
  input_tokens: number;
  output_tokens: number;
  cache_creation_input_tokens: number;
  cache_read_input_tokens: number;
}

export interface Message {
  role: MessageRole;
  content: string;
  timestamp: string | null;
  tool_name: string | null;
  tool_input: string | null;
  token_usage: TokenUsage | null;
}

export interface SessionDetail {
  meta: SessionMeta;
  messages: Message[];
}

export type TreeNodeType = "provider" | "project" | "session";

export interface TreeNode {
  id: string;
  label: string;
  node_type: TreeNodeType;
  children: TreeNode[];
  count: number;
  provider: Provider | null;
  updated_at?: number;
  is_sidechain?: boolean;
}

export interface SearchResult {
  session: SessionMeta;
  snippet: string;
}

export interface SearchFilters {
  query: string;
  provider?: string;
  project?: string;
  after?: number;
  before?: number;
}

export interface IndexStats {
  session_count: number;
  db_size_bytes: number;
  last_index_time: string;
}

export interface ProviderInfo {
  key: Provider;
  label: string;
  path: string;
  exists: boolean;
  session_count: number;
}

export interface TrashMeta {
  id: string;
  provider: string;
  title: string;
  original_path: string;
  trashed_at: number;
  trash_file: string;
}
