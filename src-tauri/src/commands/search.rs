use anyhow::Context;
use tauri::State;

use crate::error::CommandResult;
use crate::models::{SearchFilters, SearchResult};

use super::AppState;

#[tauri::command]
pub async fn search_sessions(
    filters: SearchFilters,
    state: State<'_, AppState>,
) -> CommandResult<Vec<SearchResult>> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        state
            .db
            .search_filtered(&filters)
            .context("failed to search")
    })
    .await
    .context("task join error")?
    .map_err(crate::error::CommandError::from)
}
