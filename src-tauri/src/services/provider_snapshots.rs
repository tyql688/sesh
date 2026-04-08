use crate::db::Database;
use crate::models::ProviderSnapshot;

pub struct ProviderSnapshotService<'a> {
    db: &'a Database,
}

impl<'a> ProviderSnapshotService<'a> {
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    pub fn list(&self) -> Result<Vec<ProviderSnapshot>, String> {
        let providers = crate::provider::all_runtimes();
        let counts = self
            .db
            .provider_session_counts()
            .map_err(|e| format!("failed to load provider session counts: {e}"))?;

        let mut snapshots = Vec::new();

        for runtime in &providers {
            let provider = runtime.provider();
            let paths = runtime.watch_paths();
            let path = paths
                .first()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();
            let exists = paths.first().is_some_and(|p| p.exists());

            snapshots.push(ProviderSnapshot {
                key: provider.clone(),
                label: provider.label().to_string(),
                color: provider.descriptor().color().to_string(),
                sort_order: provider.descriptor().sort_order(),
                watch_strategy: provider.descriptor().watch_strategy(),
                path,
                exists,
                session_count: counts.get(provider.key()).copied().unwrap_or(0),
            });
        }

        snapshots.sort_by_key(|snapshot| snapshot.sort_order);
        Ok(snapshots)
    }
}
