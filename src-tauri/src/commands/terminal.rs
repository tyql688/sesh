use tauri::State;

use crate::models::Provider;
use crate::terminal;

use super::AppState;

#[tauri::command]
pub fn get_resume_command(session_id: String, provider: String) -> Result<String, String> {
    let safe_id = sanitize_session_id(&session_id);
    let p = Provider::parse(&provider).unwrap_or_else(|| {
        eprintln!(
            "warning: unknown provider '{}', falling back to Claude",
            provider
        );
        Provider::Claude
    });
    Ok(p.resume_command(&safe_id))
}

/// Sanitize session ID to prevent shell injection — only allow alnum, hyphens, underscores
fn sanitize_session_id(id: &str) -> String {
    id.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
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
) -> Result<(), String> {
    // Reject any shell metacharacters to prevent command injection
    if command.chars().any(|c| SHELL_META.contains(&c)) {
        return Err("command rejected: contains shell metacharacters".to_string());
    }

    // Must match: <provider> <flag> <id> or <provider> --flag=<id> (e.g. agent --resume=xxx)
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.len() < 2 {
        return Err("command rejected: expected '<provider> <flag> <session_id>'".to_string());
    }

    const ALLOWED_PROVIDERS: &[&str] = &[
        "claude", "codex", "gemini", "cursor", "agent", "opencode", "kimi",
    ];

    if !ALLOWED_PROVIDERS.contains(&parts[0]) {
        return Err(format!(
            "command rejected: unknown provider '{}'. Allowed: {}",
            parts[0],
            ALLOWED_PROVIDERS.join(", ")
        ));
    }

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
    let p = Provider::parse(&provider).unwrap_or_else(|| {
        eprintln!(
            "warning: unknown provider '{}', falling back to Claude",
            provider
        );
        Provider::Claude
    });
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
