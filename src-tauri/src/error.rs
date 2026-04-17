use serde::{Serialize, Serializer};

/// Tauri-facing error wrapper.
///
/// Wraps `anyhow::Error` so command handlers can use `.context(..)` /
/// `.with_context(..)` freely, and serializes to the full `{:#}` source chain
/// (e.g. `"failed to rename session: database error: UNIQUE constraint..."`)
/// so the frontend toast preserves the root cause instead of only the
/// outermost context.
///
/// Commands should use `CommandResult<T>` as their return type and let `?`
/// propagate. Raw `String` / `&str` convert via the `From` impls for the rare
/// case where the error has no underlying source.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
pub struct CommandError(#[from] pub anyhow::Error);

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:#}", self.0))
    }
}

impl From<String> for CommandError {
    fn from(s: String) -> Self {
        Self(anyhow::Error::msg(s))
    }
}

impl From<&str> for CommandError {
    fn from(s: &str) -> Self {
        Self(anyhow::Error::msg(s.to_owned()))
    }
}

pub type CommandResult<T> = std::result::Result<T, CommandError>;
