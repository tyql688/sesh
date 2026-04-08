use tauri::State;

use super::AppState;
use crate::models::TrashMeta;
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
