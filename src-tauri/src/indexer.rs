use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::db::sync::TokenStatRow;
use crate::db::Database;
use crate::models::{Provider, SessionMeta, TreeNode, TreeNodeType};
use crate::pricing::{self, PricingCatalog, PRICING_CATALOG_JSON_KEY};
use crate::provider::{ParsedSession, SessionProvider};
use crate::providers::codex::parser::extract_usage_events_from_file;

#[derive(Clone)]
pub struct Indexer {
    db: Arc<Database>,
    providers: Arc<Vec<Box<dyn SessionProvider>>>,
}

impl Indexer {
    pub fn new(db: Arc<Database>, providers: Vec<Box<dyn SessionProvider>>) -> Self {
        Self {
            db,
            providers: Arc::new(providers),
        }
    }

    pub fn reindex(&self) -> Result<usize, String> {
        self.reindex_filtered(None, true)
    }

    pub fn reindex_providers(&self, filter: Option<&[Provider]>) -> Result<usize, String> {
        // Background/polling reindex uses protective sync (aggressive=false)
        // to avoid deleting sessions on transient scan failures.
        self.reindex_filtered(filter, false)
    }

    fn reindex_filtered(
        &self,
        filter: Option<&[Provider]>,
        aggressive: bool,
    ) -> Result<usize, String> {
        let start = Instant::now();
        let mut total = 0usize;
        let pricing_catalog = self
            .db
            .get_meta(PRICING_CATALOG_JSON_KEY)
            .ok()
            .flatten()
            .and_then(|json| pricing::parse_catalog(&json));

        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let excluded = crate::trash_state::shared_deleted_ids();

        for provider in self.providers.iter() {
            let provider_kind = provider.provider();

            if let Some(allowed) = filter {
                if !allowed.contains(&provider_kind) {
                    continue;
                }
            }

            let mut sessions = provider
                .scan_all()
                .map_err(|e| format!("failed to scan {} provider: {}", provider_kind.key(), e))?;

            if !excluded.is_empty() {
                sessions.retain(|s| !excluded.contains(&s.meta.id));
            }

            let count = sessions.len();
            self.db
                .sync_provider_snapshot(&provider_kind, &sessions, aggressive)
                .map_err(|e| format!("failed to sync {} provider: {}", provider_kind.key(), e))?;

            // Process parent sessions before subagents so that cross-file
            // dedup attributes overlapping usage entries to the parent.
            let mut parents: Vec<&ParsedSession> = Vec::new();
            let mut children: Vec<&ParsedSession> = Vec::new();
            for parsed in &sessions {
                if parsed.meta.parent_id.is_none() {
                    parents.push(parsed);
                } else {
                    children.push(parsed);
                }
            }

            let mut seen_hashes: HashSet<String> = HashSet::new();
            for parsed in parents.iter().chain(children.iter()) {
                let stat_rows = compute_token_stats_dedup(
                    parsed,
                    pricing_catalog.as_ref(),
                    Some(&mut seen_hashes),
                );
                if let Err(e) = self.db.replace_token_stats(&parsed.meta.id, &stat_rows) {
                    log::warn!("failed to write token stats for {}: {e}", parsed.meta.id);
                }
            }

            total += count;
        }

        if filter.is_none() {
            self.db
                .set_meta("last_index_time", &now_millis.to_string())
                .map_err(|e| format!("failed to store last_index_time: {e}"))?;
        }
        self.db
            .set_meta("usage_last_refreshed_at", &chrono::Utc::now().to_rfc3339())
            .map_err(|e| format!("failed to store usage_last_refreshed_at: {e}"))?;

        let elapsed = start.elapsed();
        log::info!(
            "Reindex complete: {} sessions indexed in {:.2}s",
            total,
            elapsed.as_secs_f64(),
        );

        Ok(total)
    }

    pub fn build_tree(&self) -> Result<Vec<TreeNode>, String> {
        let mut sessions = self
            .db
            .list_sessions()
            .map_err(|e| format!("failed to list sessions: {e}"))?;
        crate::providers::cc_mirror::hydrate_variant_names(&mut sessions);

        let mut provider_map: BTreeMap<String, BTreeMap<String, Vec<SessionMeta>>> =
            BTreeMap::new();

        for session in sessions {
            let display_key = session
                .provider
                .descriptor()
                .display_key(session.variant_name.as_deref());
            let project_key = if session.project_path.is_empty() {
                String::new()
            } else {
                session.project_path.clone()
            };

            provider_map
                .entry(display_key)
                .or_default()
                .entry(project_key)
                .or_default()
                .push(session);
        }

        let mut tree = Vec::new();

        for (display_key, projects) in &provider_map {
            let (provider_enum, label) = match Provider::parse_display_key(display_key) {
                Some(pair) => pair,
                None => continue,
            };

            let mut sorted_projects: Vec<_> = projects.iter().collect();
            sorted_projects.sort_by(|a, b| {
                let max_a = a.1.iter().map(|s| s.updated_at).max().unwrap_or(0);
                let max_b = b.1.iter().map(|s| s.updated_at).max().unwrap_or(0);
                max_b.cmp(&max_a)
            });

            let mut project_nodes = Vec::new();
            let mut provider_total = 0u32;

            for (project_path, sessions) in &sorted_projects {
                let project_label = sessions
                    .first()
                    .map(|s| {
                        if s.project_name.is_empty() {
                            "(No Project)".to_string()
                        } else {
                            s.project_name.clone()
                        }
                    })
                    .unwrap_or_else(|| "(No Project)".to_string());

                let (top_sessions, subagents): (Vec<_>, Vec<_>) =
                    sessions.iter().partition(|s| s.parent_id.is_none());

                let top_ids: std::collections::HashSet<&str> =
                    top_sessions.iter().map(|s| s.id.as_str()).collect();

                let mut session_nodes: Vec<TreeNode> = top_sessions
                    .iter()
                    .map(|s| {
                        let mut children: Vec<_> = sessions
                            .iter()
                            .filter(|c| c.parent_id.as_deref() == Some(&s.id))
                            .collect();
                        children.sort_by_key(|c| c.created_at);
                        let child_nodes: Vec<TreeNode> = children
                            .iter()
                            .map(|c| TreeNode {
                                id: c.id.clone(),
                                label: c.title.clone(),
                                node_type: TreeNodeType::Session,
                                children: Vec::new(),
                                count: 0,
                                provider: Some(provider_enum.clone()),
                                updated_at: Some(c.updated_at),
                                is_sidechain: true,
                                project_path: None,
                            })
                            .collect();

                        TreeNode {
                            id: s.id.clone(),
                            label: s.title.clone(),
                            node_type: TreeNodeType::Session,
                            children: child_nodes,
                            count: 0,
                            provider: Some(provider_enum.clone()),
                            updated_at: Some(s.updated_at),
                            is_sidechain: s.is_sidechain,
                            project_path: None,
                        }
                    })
                    .collect();

                for orphan in &subagents {
                    if let Some(ref pid) = orphan.parent_id {
                        if !top_ids.contains(pid.as_str()) {
                            session_nodes.push(TreeNode {
                                id: orphan.id.clone(),
                                label: orphan.title.clone(),
                                node_type: TreeNodeType::Session,
                                children: Vec::new(),
                                count: 0,
                                provider: Some(provider_enum.clone()),
                                updated_at: Some(orphan.updated_at),
                                is_sidechain: true,
                                project_path: None,
                            });
                        }
                    }
                }

                let count = session_nodes.len() as u32;
                if count == 0 {
                    continue;
                }
                provider_total += count;

                project_nodes.push(TreeNode {
                    id: format!("{display_key}:{project_path}"),
                    label: project_label,
                    node_type: TreeNodeType::Project,
                    children: session_nodes,
                    count,
                    provider: Some(provider_enum.clone()),
                    updated_at: None,
                    is_sidechain: false,
                    project_path: Some(project_path.to_string()),
                });
            }

            tree.push(TreeNode {
                id: display_key.to_string(),
                label,
                node_type: TreeNodeType::Provider,
                children: project_nodes,
                count: provider_total,
                provider: Some(provider_enum),
                updated_at: None,
                is_sidechain: false,
                project_path: None,
            });
        }

        tree.sort_by(|a, b| {
            let order_a = a
                .provider
                .as_ref()
                .map(|p| p.descriptor().sort_order())
                .unwrap_or(99);
            let order_b = b
                .provider
                .as_ref()
                .map(|p| p.descriptor().sort_order())
                .unwrap_or(99);
            order_a.cmp(&order_b).then(a.id.cmp(&b.id))
        });

        Ok(tree)
    }
}

/// Compute per-(date, model) token usage aggregates from a parsed session's messages.
#[cfg(test)]
pub(crate) fn compute_token_stats(parsed: &ParsedSession) -> Vec<TokenStatRow> {
    compute_token_stats_dedup(parsed, None, None)
}

pub(crate) fn compute_token_stats_with_catalog_dedup(
    parsed: &ParsedSession,
    pricing_catalog: Option<&PricingCatalog>,
    seen_hashes: &mut HashSet<String>,
) -> Vec<TokenStatRow> {
    compute_token_stats_dedup(parsed, pricing_catalog, Some(seen_hashes))
}

fn compute_token_stats_dedup(
    parsed: &ParsedSession,
    pricing_catalog: Option<&PricingCatalog>,
    mut seen_hashes: Option<&mut HashSet<String>>,
) -> Vec<TokenStatRow> {
    if parsed.meta.provider == Provider::Codex {
        return compute_codex_token_stats(parsed, pricing_catalog);
    }

    let fallback_date = chrono::DateTime::from_timestamp(parsed.meta.created_at, 0)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .unwrap_or_else(|| "1970-01-01".to_string());
    let fallback_model = parsed.meta.model.as_deref().unwrap_or("").to_string();
    let mut last_seen_model = fallback_model.clone();

    let mut stats_map: HashMap<(String, String), TokenStatRow> = HashMap::new();
    for msg in &parsed.messages {
        if let Some(model) = msg.model.as_deref().filter(|model| !model.is_empty()) {
            last_seen_model = model.to_string();
        }
        if let Some(usage) = &msg.token_usage {
            // Dedup: skip if this usage entry was already counted (cross-file)
            if let Some(ref mut seen) = seen_hashes {
                if let Some(ref hash) = msg.usage_hash {
                    if !seen.insert(hash.clone()) {
                        continue;
                    }
                }
            }

            let date = msg
                .timestamp
                .as_deref()
                .and_then(timestamp_to_local_date)
                .unwrap_or_else(|| fallback_date.clone());
            let model = msg
                .model
                .as_deref()
                .filter(|model| !model.is_empty())
                .unwrap_or(&last_seen_model)
                .to_string();
            let entry = stats_map
                .entry((date.clone(), model.clone()))
                .or_insert_with(|| TokenStatRow {
                    date,
                    model,
                    turn_count: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    cost_usd: 0.0,
                });
            entry.turn_count += 1;
            entry.input_tokens += usage.input_tokens as u64;
            entry.output_tokens += usage.output_tokens as u64;
            entry.cache_read_tokens += usage.cache_read_input_tokens as u64;
            entry.cache_write_tokens += usage.cache_creation_input_tokens as u64;
            entry.cost_usd += pricing::estimate_cost_with_catalog(
                pricing_catalog,
                &entry.model,
                usage.input_tokens as u64,
                usage.output_tokens as u64,
                usage.cache_read_input_tokens as u64,
                usage.cache_creation_input_tokens as u64,
            );
        }
    }

    stats_map.into_values().collect()
}

fn timestamp_to_local_date(timestamp: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d")
                .to_string()
        })
        .or_else(|| timestamp.get(..10).map(ToString::to_string))
}

fn compute_codex_token_stats(
    parsed: &ParsedSession,
    pricing_catalog: Option<&PricingCatalog>,
) -> Vec<TokenStatRow> {
    let path = PathBuf::from(&parsed.meta.source_path);
    let mut stats_map: HashMap<(String, String), TokenStatRow> = HashMap::new();

    for event in extract_usage_events_from_file(&path) {
        let Some(date) = timestamp_to_local_date(&event.timestamp) else {
            continue;
        };
        let key = (date.clone(), event.model.clone());
        let entry = stats_map.entry(key).or_insert_with(|| TokenStatRow {
            date,
            model: event.model.clone(),
            turn_count: 0,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            cost_usd: 0.0,
        });
        entry.turn_count += 1;
        entry.input_tokens += event.input_tokens;
        entry.output_tokens += event.output_tokens;
        entry.cache_read_tokens += event.cache_read_input_tokens;
        entry.cost_usd += pricing::estimate_cost_with_catalog(
            pricing_catalog,
            &entry.model,
            event.input_tokens,
            event.output_tokens,
            event.cache_read_input_tokens,
            0,
        );
    }

    stats_map.into_values().collect()
}

#[cfg(test)]
mod tests {
    use super::{compute_token_stats, compute_token_stats_with_catalog_dedup};
    use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
    use crate::provider::ParsedSession;
    use std::collections::HashSet;

    fn make_session(meta_model: Option<&str>, messages: Vec<Message>) -> ParsedSession {
        ParsedSession {
            meta: SessionMeta {
                id: "session-1".into(),
                provider: Provider::Claude,
                title: "Test".into(),
                project_path: "/tmp/project".into(),
                project_name: "project".into(),
                created_at: 1_775_635_200,
                updated_at: 1_775_635_200,
                message_count: messages.len() as u32,
                file_size_bytes: 0,
                source_path: "/tmp/source.jsonl".into(),
                is_sidechain: false,
                variant_name: None,
                model: meta_model.map(str::to_string),
                cc_version: None,
                git_branch: None,
                parent_id: None,
            },
            messages,
            content_text: String::new(),
        }
    }

    fn token_usage(input: u32, output: u32) -> Option<TokenUsage> {
        Some(TokenUsage {
            input_tokens: input,
            output_tokens: output,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        })
    }

    #[test]
    fn compute_token_stats_falls_back_to_session_model() {
        let parsed = make_session(
            Some("claude-opus-4-6"),
            vec![Message {
                role: MessageRole::Assistant,
                content: String::new(),
                timestamp: Some("2026-04-09T12:00:00Z".into()),
                tool_name: None,
                tool_input: None,
                token_usage: token_usage(100, 50),
                model: None,
                usage_hash: None,
            }],
        );

        let rows = compute_token_stats(&parsed);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "claude-opus-4-6");
    }

    #[test]
    fn compute_token_stats_falls_back_to_created_at_date() {
        let parsed = make_session(
            Some("gpt-5.4"),
            vec![Message {
                role: MessageRole::Assistant,
                content: String::new(),
                timestamp: None,
                tool_name: None,
                tool_input: None,
                token_usage: token_usage(25, 10),
                model: None,
                usage_hash: None,
            }],
        );

        let rows = compute_token_stats(&parsed);
        assert_eq!(rows.len(), 1);
        let expected_date = chrono::DateTime::from_timestamp(parsed.meta.created_at, 0)
            .expect("valid timestamp")
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(rows[0].date, expected_date);
    }

    #[test]
    fn compute_token_stats_uses_latest_seen_model_for_tool_usage() {
        let parsed = make_session(
            Some("claude-haiku-4-5-20251001"),
            vec![
                Message {
                    role: MessageRole::Assistant,
                    content: String::new(),
                    timestamp: Some("2026-04-09T12:00:00Z".into()),
                    tool_name: None,
                    tool_input: None,
                    token_usage: None,
                    model: Some("claude-opus-4-6".into()),
                    usage_hash: None,
                },
                Message {
                    role: MessageRole::Tool,
                    content: String::new(),
                    timestamp: Some("2026-04-09T12:00:01Z".into()),
                    tool_name: Some("Bash".into()),
                    tool_input: None,
                    token_usage: token_usage(100, 50),
                    model: None,
                    usage_hash: None,
                },
            ],
        );

        let rows = compute_token_stats(&parsed);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "claude-opus-4-6");
    }

    #[test]
    fn compute_token_stats_groups_dates_in_local_timezone() {
        let parsed = make_session(
            Some("claude-opus-4-6"),
            vec![Message {
                role: MessageRole::Assistant,
                content: String::new(),
                timestamp: Some("2026-04-08T16:30:00Z".into()),
                tool_name: None,
                tool_input: None,
                token_usage: token_usage(10, 5),
                model: Some("claude-opus-4-6".into()),
                usage_hash: None,
            }],
        );

        let rows = compute_token_stats(&parsed);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, "2026-04-09");
    }

    #[test]
    fn compute_token_stats_dedups_same_usage_hash_across_sessions() {
        let make_message = || Message {
            role: MessageRole::Assistant,
            content: String::new(),
            timestamp: Some("2026-04-09T12:00:00Z".into()),
            tool_name: None,
            tool_input: None,
            token_usage: token_usage(100, 50),
            model: Some("claude-opus-4-6".into()),
            usage_hash: Some("msg-1:req-1".into()),
        };

        let first = make_session(Some("claude-opus-4-6"), vec![make_message()]);
        let second = make_session(Some("claude-opus-4-6"), vec![make_message()]);
        let mut seen_hashes = HashSet::new();

        let first_rows = compute_token_stats_with_catalog_dedup(&first, None, &mut seen_hashes);
        let second_rows = compute_token_stats_with_catalog_dedup(&second, None, &mut seen_hashes);

        assert_eq!(first_rows.len(), 1);
        assert!(second_rows.is_empty());
    }
}
