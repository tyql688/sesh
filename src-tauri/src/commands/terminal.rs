use tauri::State;

use crate::models::Provider;
use crate::terminal;

use super::AppState;

#[tauri::command]
pub fn get_resume_command(session_id: String, provider: String) -> Result<String, String> {
    let safe_id = sanitize_session_id(&session_id);
    let p = Provider::from_str(&provider).unwrap_or(Provider::Claude);
    Ok(p.resume_command(&safe_id))
}

/// Sanitize session ID to prevent shell injection — only allow alnum, hyphens, underscores
fn sanitize_session_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

#[tauri::command]
pub fn open_in_terminal(
    command: String,
    cwd: Option<String>,
    terminal_app: String,
) -> Result<(), String> {
    terminal::launch_terminal(&terminal_app, &command, cwd.as_deref())
}

/// Resume a session: looks up cwd from DB, builds command, launches terminal
#[tauri::command]
pub fn resume_session(
    session_id: String,
    provider: String,
    terminal_app: String,
    state: State<AppState>,
) -> Result<(), String> {
    let safe_id = sanitize_session_id(&session_id);
    let p = Provider::from_str(&provider).unwrap_or(Provider::Claude);
    let cmd = p.resume_command(&safe_id);

    let cwd = state
        .db
        .get_session(&session_id)
        .ok()
        .flatten()
        .and_then(|s| {
            if s.project_path.is_empty() {
                None
            } else {
                Some(s.project_path)
            }
        });

    terminal::launch_terminal(&terminal_app, &cmd, cwd.as_deref())
}

#[tauri::command]
pub fn detect_terminal() -> String {
    // macOS: check $TERM_PROGRAM
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
        return "powershell".to_string();
    }

    #[cfg(not(target_os = "windows"))]
    "terminal".to_string()
}
