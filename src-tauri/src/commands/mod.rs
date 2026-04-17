mod search;
mod sessions;
mod settings;
mod terminal;
pub mod trash;
mod usage;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::db::Database;
use crate::indexer::Indexer;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub indexer: Indexer,
    pub maintenance_running: Arc<AtomicBool>,
}

pub use search::*;
pub use sessions::*;
pub use settings::*;
pub use terminal::*;
pub use trash::*;
pub use usage::*;

pub(crate) fn load_session_detail_for_tests(
    db: &crate::db::Database,
    session_id: &str,
) -> Result<crate::models::SessionDetail, String> {
    sessions::load_detail(session_id, db).map_err(|e| format!("{e:#}"))
}

pub(crate) fn get_resume_command_for_tests(
    db: &crate::db::Database,
    session_id: &str,
) -> Result<String, String> {
    terminal::get_resume_command_for_db(db, session_id).map_err(|e| format!("{e:#}"))
}
