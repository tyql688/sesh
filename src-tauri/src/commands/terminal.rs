use anyhow::anyhow;
use tauri::State;

use crate::db::Database;
use crate::error::{CommandError, CommandResult};
use crate::models::Provider;
use crate::services::load_session_meta;
use crate::terminal;

use super::AppState;

struct ResumeTarget {
    command: String,
    cwd: Option<String>,
}

#[tauri::command]
pub fn get_resume_command(session_id: String, state: State<AppState>) -> CommandResult<String> {
    Ok(get_resume_command_for_db(&state.db, &session_id)?)
}

/// Sanitize session ID to prevent shell injection — only allow alnum, hyphens, underscores
fn sanitize_session_id(id: &str) -> anyhow::Result<String> {
    if id.is_empty() {
        return Err(anyhow!("session id is empty"));
    }

    if id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Ok(id.to_string());
    }

    Err(anyhow!("session id contains invalid characters: '{id}'"))
}

fn resolve_resume_target(db: &Database, session_id: &str) -> anyhow::Result<ResumeTarget> {
    let safe_id = sanitize_session_id(session_id)?;
    let session = load_session_meta(db, session_id).map_err(anyhow::Error::msg)?;
    let variant_name = session
        .variant_name
        .as_deref()
        .map(sanitize_session_id)
        .transpose()?;

    let command = session
        .provider
        .descriptor()
        .resume_command(&safe_id, variant_name.as_deref())
        .ok_or_else(|| anyhow!("{} session missing variant name", session.provider.key()))?;

    let cwd = (!session.project_path.is_empty()).then_some(session.project_path);

    Ok(ResumeTarget { command, cwd })
}

pub(crate) fn get_resume_command_for_db(db: &Database, session_id: &str) -> anyhow::Result<String> {
    Ok(resolve_resume_target(db, session_id)?.command)
}

/// Shell metacharacters that must never appear in a terminal command.
const SHELL_META: &[char] = &[
    '&', ';', '|', '`', '$', '(', ')', '{', '}', '<', '>', '\n', '\r',
];

#[tauri::command]
pub fn open_in_terminal(
    command: String,
    cwd: Option<String>,
    terminal_app: String,
) -> CommandResult<()> {
    // Reject any shell metacharacters to prevent command injection
    if command.chars().any(|c| SHELL_META.contains(&c)) {
        return Err(anyhow!("command rejected: contains shell metacharacters").into());
    }

    // Must match: <provider> <flag> <id> or <provider> --flag=<id> (e.g. agent --resume=xxx)
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(anyhow!("command rejected: expected '<provider> <flag> <session_id>'").into());
    }

    let cmd_name = parts[0];
    // Security: only allow known CLI commands or discovered cc-mirror variants.
    let is_allowed = Provider::all().iter().any(|p| {
        !p.descriptor().cli_command().is_empty() && p.descriptor().cli_command() == cmd_name
    }) || is_known_cc_mirror_variant(cmd_name);

    if !is_allowed {
        return Err(anyhow!("command rejected: unknown provider '{cmd_name}'").into());
    }

    terminal::launch_terminal(&terminal_app, &command, cwd.as_deref()).map_err(CommandError::from)
}

/// Check if a command name matches a discovered cc-mirror variant.
fn is_known_cc_mirror_variant(name: &str) -> bool {
    crate::providers::cc_mirror::is_known_variant_command(name)
}

/// Resume a session: looks up cwd from DB, builds command, launches terminal
#[tauri::command]
pub fn resume_session(
    session_id: String,
    terminal_app: String,
    state: State<AppState>,
) -> CommandResult<()> {
    let target = resolve_resume_target(&state.db, &session_id)?;
    terminal::launch_terminal(&terminal_app, &target.command, target.cwd.as_deref())
        .map_err(CommandError::from)?;
    Ok(())
}

#[tauri::command]
pub fn detect_terminal() -> String {
    // Check $TERM_PROGRAM (set by macOS terminals and some Linux terminals)
    if let Ok(term) = std::env::var("TERM_PROGRAM") {
        match term.to_lowercase().as_str() {
            "iterm.app" => return "iterm2".to_string(),
            "apple_terminal" => return "terminal".to_string(),
            "ghostty" => return "ghostty".to_string(),
            "wezterm-gui" | "wezterm" => return "wezterm".to_string(),
            "warpterm" | "warp" => return "warp".to_string(),
            "kitty" => return "kitty".to_string(),
            "alacritty" => return "alacritty".to_string(),
            _ => {}
        }
    }

    // Windows: check for Windows Terminal
    #[cfg(target_os = "windows")]
    {
        if std::env::var("WT_SESSION").is_ok() {
            return "windows-terminal".to_string();
        }
        "powershell".to_string()
    }

    // Linux: check common terminal indicators
    #[cfg(target_os = "linux")]
    {
        if std::env::var("GNOME_TERMINAL_SERVICE").is_ok()
            || std::env::var("GNOME_TERMINAL_SCREEN").is_ok()
        {
            return "gnome-terminal".to_string();
        }
        if std::env::var("KONSOLE_VERSION").is_ok() {
            return "konsole".to_string();
        }
        // Fallback: probe common terminals in order
        let candidates = [
            "gnome-terminal",
            "konsole",
            "alacritty",
            "kitty",
            "wezterm",
            "xfce4-terminal",
            "xterm",
        ];
        for term in &candidates {
            if std::process::Command::new("which")
                .arg(term)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return term.to_string();
            }
        }
        "xterm".to_string()
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    "terminal".to_string()
}

#[cfg(test)]
mod tests {
    use super::sanitize_session_id;

    #[test]
    fn sanitize_session_id_accepts_safe_ids() {
        assert_eq!(sanitize_session_id("abc-123_DEF").unwrap(), "abc-123_DEF");
    }

    #[test]
    fn sanitize_session_id_accepts_unicode_ids() {
        assert_eq!(
            sanitize_session_id("会话-123_变体").unwrap(),
            "会话-123_变体"
        );
    }

    #[test]
    fn sanitize_session_id_rejects_invalid_ids() {
        let err = sanitize_session_id("abc;rm").unwrap_err().to_string();
        assert!(err.contains("invalid characters"));
    }
}
