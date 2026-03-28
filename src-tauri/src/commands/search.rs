use tauri::State;

use crate::models::{SearchFilters, SearchResult};

use super::AppState;

#[tauri::command]
pub fn search_sessions(
    filters: SearchFilters,
    state: State<AppState>,
) -> Result<Vec<SearchResult>, String> {
    state
        .db
        .search_filtered(&filters)
        .map_err(|e| format!("failed to search: {e}"))
}
