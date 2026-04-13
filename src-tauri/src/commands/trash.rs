use tauri::State;

use super::AppState;
use crate::models::{BatchResult, TrashMeta};
use crate::services::SessionLifecycleService;

#[tauri::command]
pub fn trash_session(session_id: String, state: State<AppState>) -> Result<(), String> {
    SessionLifecycleService::new(&state.db).trash_session(&session_id)
}

#[tauri::command]
pub fn list_trash() -> Result<Vec<TrashMeta>, String> {
    SessionLifecycleService::list_trash()
}

#[tauri::command]
pub fn restore_session(trash_id: String, state: State<AppState>) -> Result<(), String> {
    SessionLifecycleService::new(&state.db).restore_session(&trash_id)
}

#[tauri::command]
pub fn empty_trash() -> Result<(), String> {
    SessionLifecycleService::empty_trash()
}

#[tauri::command]
pub fn permanent_delete_trash(trash_id: String, state: State<AppState>) -> Result<(), String> {
    SessionLifecycleService::new(&state.db).permanent_delete_trash(&trash_id)
}

#[tauri::command]
pub async fn trash_sessions_batch(
    items: Vec<String>,
    state: State<'_, AppState>,
) -> Result<BatchResult, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        Ok(SessionLifecycleService::new(&state.db).trash_sessions(&items))
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub async fn restore_sessions_batch(
    items: Vec<String>,
    state: State<'_, AppState>,
) -> Result<BatchResult, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        Ok(SessionLifecycleService::new(&state.db).restore_sessions(&items))
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}

#[tauri::command]
pub async fn permanent_delete_trash_batch(
    items: Vec<String>,
    state: State<'_, AppState>,
) -> Result<BatchResult, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        Ok(SessionLifecycleService::new(&state.db).permanent_delete_trash_batch(&items))
    })
    .await
    .map_err(|e| format!("task join error: {e}"))?
}
