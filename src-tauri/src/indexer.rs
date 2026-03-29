use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::db::Database;
use crate::models::{Provider, TreeNode, TreeNodeType};
use crate::provider::SessionProvider;

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
        let start = Instant::now();
        let mut total = 0usize;

        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        for provider in self.providers.iter() {
            let provider_kind = provider.provider();
            let sessions = provider
                .scan_all()
                .map_err(|e| format!("failed to scan {} provider: {}", provider_kind.key(), e))?;

            let count = sessions.len();
            self.db
                .sync_provider_snapshot(&provider_kind, &sessions)
                .map_err(|e| format!("failed to sync {} provider: {}", provider_kind.key(), e))?;
            total += count;
        }

        self.db
            .set_meta("last_index_time", &now_millis.to_string())
            .map_err(|e| format!("failed to store last_index_time: {e}"))?;

        let elapsed = start.elapsed();
        println!(
            "Reindex complete: {} sessions indexed in {:.2}s",
            total,
            elapsed.as_secs_f64(),
        );

        Ok(total)
    }

    pub fn build_tree(&self) -> Result<Vec<TreeNode>, String> {
        let sessions = self
            .db
            .list_sessions()
            .map_err(|e| format!("failed to list sessions: {e}"))?;

        // Group by provider -> project_path -> sessions
        let mut provider_map: BTreeMap<String, BTreeMap<String, Vec<crate::models::SessionMeta>>> =
            BTreeMap::new();

        for session in sessions {
            let provider_key = session.provider.key().to_string();
            let project_key = if session.project_path.is_empty() {
                String::new()
            } else {
                session.project_path.clone()
            };

            provider_map
                .entry(provider_key)
                .or_default()
                .entry(project_key)
                .or_default()
                .push(session);
        }

        let mut tree = Vec::new();

        for (provider_key, projects) in &provider_map {
            let provider_enum = match Provider::parse(provider_key) {
                Some(p) => p,
                None => continue,
            };

            let label = provider_enum.label().to_string();

            // Sort projects by most recently updated session
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
                let session_nodes: Vec<TreeNode> = sessions
                    .iter()
                    .map(|s| TreeNode {
                        id: s.id.clone(),
                        label: s.title.clone(),
                        node_type: TreeNodeType::Session,
                        children: Vec::new(),
                        count: 0,
                        provider: Some(provider_enum.clone()),
                        updated_at: Some(s.updated_at),
                        is_sidechain: s.is_sidechain,
                    })
                    .collect();

                let count = session_nodes.len() as u32;
                provider_total += count;

                project_nodes.push(TreeNode {
                    id: format!("{provider_key}:{project_path}"),
                    label: project_label,
                    node_type: TreeNodeType::Project,
                    children: session_nodes,
                    count,
                    provider: Some(provider_enum.clone()),
                    updated_at: None,
                    is_sidechain: false,
                });
            }

            tree.push(TreeNode {
                id: provider_key.clone(),
                label,
                node_type: TreeNodeType::Provider,
                children: project_nodes,
                count: provider_total,
                provider: Some(provider_enum),
                updated_at: None,
                is_sidechain: false,
            });
        }

        Ok(tree)
    }
}
