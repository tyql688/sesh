use std::collections::HashSet;

use crate::db::Database;
use crate::indexer::compute_token_stats_with_catalog_dedup;
use crate::models::Provider;
use crate::pricing::{self, PRICING_CATALOG_JSON_KEY};
use crate::services::image_cache::{image_cache_provider_for, ImageCacheService};

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

    pub fn sync_provider_source(
        &self,
        provider: Provider,
        source_path: &str,
    ) -> Result<(), String> {
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
            .map_err(|e| format!("failed to sync source snapshot: {e}"))?;

        let pricing_catalog = match self.db.get_meta(PRICING_CATALOG_JSON_KEY) {
            Ok(Some(json)) => match pricing::parse_catalog(&json) {
                Ok(catalog) => Some(catalog),
                Err(error) => {
                    log::warn!("failed to parse cached pricing catalog for source sync: {error}");
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                log::warn!("failed to read cached pricing catalog for source sync: {error}");
                None
            }
        };

        let mut seen_hashes = HashSet::new();
        let mut stats_batch: Vec<(String, Vec<crate::db::sync::TokenStatRow>)> = Vec::new();
        for parsed in &sessions {
            let stat_rows = compute_token_stats_with_catalog_dedup(
                parsed,
                pricing_catalog.as_ref(),
                &mut seen_hashes,
            );
            stats_batch.push((parsed.meta.id.clone(), stat_rows));
        }
        {
            let batch_refs: Vec<(&str, &[crate::db::sync::TokenStatRow])> = stats_batch
                .iter()
                .map(|(id, rows)| (id.as_str(), rows.as_slice()))
                .collect();
            if let Err(e) = self.db.replace_token_stats_batch(&batch_refs) {
                log::warn!("failed to write token stats batch for source {source_path}: {e}");
            }
        }

        // Cache images for providers that support it
        if let Some(cache_provider) = image_cache_provider_for(&provider) {
            if let Some(data_dir) = crate::services::image_cache::image_cache_data_dir() {
                let image_service = ImageCacheService::new(&data_dir);
                for parsed in &sessions {
                    image_service.cache_images(cache_provider.as_ref(), &parsed.messages);
                }
            }
        }

        self.db
            .set_meta("usage_last_refreshed_at", &chrono::Utc::now().to_rfc3339())
            .map_err(|e| format!("failed to store usage_last_refreshed_at: {e}"))?;

        Ok(())
    }

    pub fn sync_provider_key(&self, provider_key: &str, source_path: &str) -> Result<(), String> {
        let provider = Provider::parse_strict(provider_key)?;
        self.sync_provider_source(provider, source_path)
    }
}
