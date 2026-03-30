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

/// Create a provider instance by enum variant. Returns None if HOME is unavailable.
pub fn make_provider(provider: &Provider) -> Option<Box<dyn SessionProvider>> {
    match provider {
        Provider::Claude => crate::providers::claude::ClaudeProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::Codex => crate::providers::codex::CodexProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::Gemini => crate::providers::gemini::GeminiProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::Cursor => crate::providers::cursor::CursorProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::OpenCode => crate::providers::opencode::OpenCodeProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::Kimi => crate::providers::kimi::KimiProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
        Provider::CcMirror => crate::providers::cc_mirror::CcMirrorProvider::new()
            .map(|p| Box::new(p) as Box<dyn SessionProvider>),
    }
}

/// Create all provider instances, silently skipping any that cannot resolve HOME.
pub fn all_providers() -> Vec<Box<dyn SessionProvider>> {
    Provider::all().iter().filter_map(make_provider).collect()
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
