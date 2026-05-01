use anyhow::Context;
use tauri::State;

use super::AppState;
use crate::error::{CommandError, CommandResult};
use crate::models::{BatchResult, TrashMeta};
use crate::services::SessionLifecycleService;

#[tauri::command]
pub async fn trash_session(session_id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        SessionLifecycleService::new(&state.db).trash_session(&session_id)
    })
    .await
    .context("task join error")?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_trash() -> CommandResult<Vec<TrashMeta>> {
    tokio::task::spawn_blocking(SessionLifecycleService::list_trash)
        .await
        .context("task join error")?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn restore_session(trash_id: String, state: State<'_, AppState>) -> CommandResult<()> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        SessionLifecycleService::new(&state.db).restore_session(&trash_id)
    })
    .await
    .context("task join error")?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn empty_trash() -> CommandResult<()> {
    tokio::task::spawn_blocking(SessionLifecycleService::empty_trash)
        .await
        .context("task join error")?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn permanent_delete_trash(
    trash_id: String,
    state: State<'_, AppState>,
) -> CommandResult<()> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        SessionLifecycleService::new(&state.db).permanent_delete_trash(&trash_id)
    })
    .await
    .context("task join error")?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn trash_sessions_batch(
    items: Vec<String>,
    state: State<'_, AppState>,
) -> CommandResult<BatchResult> {
    let state = state.inner().clone();
    let result = tokio::task::spawn_blocking(move || {
        SessionLifecycleService::new(&state.db).trash_sessions(&items)
    })
    .await
    .context("task join error")?;
    Ok(result)
}

#[tauri::command]
pub async fn restore_sessions_batch(
    items: Vec<String>,
    state: State<'_, AppState>,
) -> CommandResult<BatchResult> {
    let state = state.inner().clone();
    let result = tokio::task::spawn_blocking(move || {
        SessionLifecycleService::new(&state.db).restore_sessions(&items)
    })
    .await
    .context("task join error")?;
    Ok(result)
}

#[tauri::command]
pub async fn permanent_delete_trash_batch(
    items: Vec<String>,
    state: State<'_, AppState>,
) -> CommandResult<BatchResult> {
    let state = state.inner().clone();
    let result = tokio::task::spawn_blocking(move || {
        SessionLifecycleService::new(&state.db).permanent_delete_trash_batch(&items)
    })
    .await
    .context("task join error")?;
    Ok(result)
}
