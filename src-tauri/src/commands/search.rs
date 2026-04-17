use anyhow::Context;
use tauri::State;

use crate::error::CommandResult;
use crate::models::{SearchFilters, SearchResult};

use super::AppState;

#[tauri::command]
pub fn search_sessions(
    filters: SearchFilters,
    state: State<AppState>,
) -> CommandResult<Vec<SearchResult>> {
    let results = state
        .db
        .search_filtered(&filters)
        .context("failed to search")?;
    Ok(results)
}
