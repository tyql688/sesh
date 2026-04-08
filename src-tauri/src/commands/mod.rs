mod search;
mod sessions;
mod settings;
mod terminal;
pub mod trash;

use std::sync::Arc;

use crate::db::Database;
use crate::indexer::Indexer;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub indexer: Indexer,
}

pub use search::*;
pub use sessions::*;
pub use settings::*;
pub use terminal::*;
pub use trash::*;
