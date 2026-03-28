use std::path::PathBuf;

use thiserror::Error;

use crate::models::{Message, Provider, SessionMeta};

#[derive(Error, Debug)]
pub enum ProviderError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Clone)]
pub struct ParsedSession {
    pub meta: SessionMeta,
    pub messages: Vec<Message>,
    pub content_text: String,
}

/// Create a provider instance by enum variant.
pub fn make_provider(provider: &Provider) -> Box<dyn SessionProvider> {
    match provider {
        Provider::Claude => Box::new(crate::providers::claude::ClaudeProvider::new()),
        Provider::Codex => Box::new(crate::providers::codex::CodexProvider::new()),
        Provider::Gemini => Box::new(crate::providers::gemini::GeminiProvider::new()),
        Provider::Cursor => Box::new(crate::providers::cursor::CursorProvider::new()),
        Provider::OpenCode => Box::new(crate::providers::opencode::OpenCodeProvider::new()),
    }
}

/// Create all provider instances.
pub fn all_providers() -> Vec<Box<dyn SessionProvider>> {
    Provider::all().iter().map(make_provider).collect()
}

pub trait SessionProvider: Send + Sync {
    fn provider(&self) -> Provider;
    fn watch_paths(&self) -> Vec<PathBuf>;
    fn scan_all(&self) -> Result<Vec<ParsedSession>, ProviderError>;
    fn scan_source(&self, source_path: &str) -> Result<Vec<ParsedSession>, ProviderError> {
        Ok(self
            .scan_all()?
            .into_iter()
            .filter(|session| session.meta.source_path == source_path)
            .collect())
    }
    fn load_messages(
        &self,
        session_id: &str,
        source_path: &str,
    ) -> Result<Vec<Message>, ProviderError>;
}
