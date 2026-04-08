use crate::db::Database;
use crate::models::Provider;

pub struct SourceSyncService<'a> {
    db: &'a Database,
}

impl<'a> SourceSyncService<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn sync_source_path(&self, source_path: &str) -> Result<bool, String> {
        let Some(provider) = Provider::from_source_path(source_path) else {
            return Ok(false);
        };

        self.sync_provider_source(provider, source_path)?;
        Ok(true)
    }

    pub fn sync_provider_source(&self, provider: Provider, source_path: &str) -> Result<(), String> {
        let provider_impl = provider.require_runtime()?;

        let mut sessions = provider_impl
            .scan_source(source_path)
            .map_err(|e| format!("failed to scan source: {e}"))?;

        let excluded = crate::trash_state::shared_deleted_ids();
        if !excluded.is_empty() {
            sessions.retain(|session| !excluded.contains(&session.meta.id));
        }

        self.db
            .sync_source_snapshot(&provider, source_path, &sessions)
            .map_err(|e| format!("failed to sync source snapshot: {e}"))
    }

    pub fn sync_provider_key(&self, provider_key: &str, source_path: &str) -> Result<(), String> {
        let provider = Provider::parse_strict(provider_key)?;
        self.sync_provider_source(provider, source_path)
    }
}
