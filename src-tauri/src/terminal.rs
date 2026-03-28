use std::process::Command;

pub fn launch_terminal(target: &str, command: &str, cwd: Option<&str>) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("Command is empty".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        match target {
            "terminal" => launch_macos_terminal(command, cwd),
            "iterm2" => launch_iterm(command, cwd),
            "ghostty" => launch_ghostty(command, cwd),
            "kitty" => launch_kitty(command, cwd),
            "warp" => launch_warp(command, cwd),
            "wezterm" => launch_wezterm(command, cwd),
            "alacritty" => launch_alacritty(command, cwd),
            _ => launch_macos_terminal(command, cwd),
        }
    }

    #[cfg(target_os = "windows")]
    {
        match target {
            "powershell" => launch_windows_powershell(command, cwd),
            "cmd" => launch_windows_cmd(command, cwd),
            _ => launch_windows_terminal(command, cwd),
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Terminal launch is not supported on this platform".to_string())
    }
}

fn launch_macos_terminal(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, cwd);
    let escaped = escape_osascript(&full_command);
    // Use "do script" without "in window" to always create a new tab
    // in the frontmost window (or a new window if none exists).
    // Activate first to avoid racing with Terminal's own startup window.
    let script = format!(
        r#"tell application "Terminal"
    activate
    do script "{escaped}"
end tell"#
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|e| format!("Failed to launch Terminal: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Terminal command execution failed".to_string())
    }
}

fn launch_iterm(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, cwd);
    let escaped = escape_osascript(&full_command);
    let script = format!(
        r#"tell application "iTerm"
    activate
    if (count of windows) > 0 then
        tell current window
            create tab with default profile
            tell current session
                write text "{escaped}"
            end tell
        end tell
    else
        create window with default profile
        tell current session of current window
            write text "{escaped}"
        end tell
    end if
end tell"#
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|e| format!("Failed to launch iTerm: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("iTerm command execution failed".to_string())
    }
}

fn launch_ghostty(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let args = build_ghostty_args(command, cwd);

    let status = Command::new("open")
        .args(args.iter().map(String::as_str))
        .status()
        .map_err(|e| format!("Failed to launch Ghostty: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch Ghostty. Make sure it is installed.".to_string())
    }
}

fn build_ghostty_args(command: &str, cwd: Option<&str>) -> Vec<String> {
    let input = ghostty_raw_input(command);

    let mut args = vec![
        "-na".to_string(),
        "Ghostty".to_string(),
        "--args".to_string(),
        "--quit-after-last-window-closed=true".to_string(),
    ];

    if let Some(dir) = cwd {
        if !dir.trim().is_empty() {
            args.push(format!("--working-directory={dir}"));
        }
    }

    args.push(format!("--input={input}"));
    args
}

fn ghostty_raw_input(command: &str) -> String {
    let mut escaped = String::from("raw:");
    for ch in command.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            _ => escaped.push(ch),
        }
    }
    escaped.push_str("\\n");
    escaped
}

fn launch_warp(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, cwd);
    let escaped = escape_osascript(&full_command);
    let script = format!(
        r#"tell application "Warp"
    activate
    delay 0.5
    tell application "System Events"
        keystroke "{escaped}"
        key code 36
    end tell
end tell"#
    );

    let status = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .status()
        .map_err(|e| format!("Failed to launch Warp: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch Warp. Make sure it is installed.".to_string())
    }
}

fn launch_kitty(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let mut cmd = Command::new("open");
    cmd.arg("-na").arg("kitty").arg("--args");

    if let Some(dir) = cwd {
        if !dir.trim().is_empty() {
            cmd.arg("--directory").arg(dir);
        }
    }

    cmd.arg("-e").arg(&shell).arg("-c").arg(command);

    let status = cmd
        .status()
        .map_err(|e| format!("Failed to launch Kitty: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch Kitty. Make sure it is installed.".to_string())
    }
}

fn launch_wezterm(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, None);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let mut args = vec!["-na", "WezTerm", "--args", "start"];

    if let Some(dir) = cwd {
        args.push("--cwd");
        args.push(dir);
    }

    args.push("--");
    args.push(&shell);
    args.push("-c");
    args.push(&full_command);

    let status = Command::new("open")
        .args(&args)
        .status()
        .map_err(|e| format!("Failed to launch WezTerm: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch WezTerm.".to_string())
    }
}

fn launch_alacritty(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let full_command = build_shell_command(command, None);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

    let mut args = vec!["-na", "Alacritty", "--args"];

    if let Some(dir) = cwd {
        args.push("--working-directory");
        args.push(dir);
    }

    args.push("-e");
    args.push(&shell);
    args.push("-c");
    args.push(&full_command);

    let status = Command::new("open")
        .args(&args)
        .status()
        .map_err(|e| format!("Failed to launch Alacritty: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("Failed to launch Alacritty.".to_string())
    }
}

fn build_shell_command(command: &str, cwd: Option<&str>) -> String {
    match cwd {
        Some(dir) if !dir.trim().is_empty() => {
            format!("cd {} && {}", shell_escape(dir), command)
        }
        _ => command.to_string(),
    }
}

fn shell_escape(value: &str) -> String {
    // Single-quote wrapping is the POSIX-safe approach: only ' needs escaping
    let escaped = value.replace('\'', "'\\''");
    format!("'{escaped}'")
}

fn escape_osascript(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

// --- Windows terminal launchers ---

#[cfg(target_os = "windows")]
fn launch_windows_terminal(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("wt");
    cmd.args(["new-tab", "--profile", "Windows PowerShell", "--"]);
    cmd.arg("powershell");
    cmd.args(["-NoExit", "-Command"]);
    let full = build_windows_command(command, cwd);
    cmd.arg(&full);
    cmd.spawn().map_err(|e| format!("failed to launch Windows Terminal: {e}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn launch_windows_powershell(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoExit", "-Command"]);
    let full = build_windows_command(command, cwd);
    cmd.arg(&full);
    cmd.spawn().map_err(|e| format!("failed to launch PowerShell: {e}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn launch_windows_cmd(command: &str, cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = Command::new("cmd");
    cmd.args(["/k"]);
    let full = build_windows_command(command, cwd);
    cmd.arg(&full);
    cmd.spawn().map_err(|e| format!("failed to launch cmd: {e}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn build_windows_command(command: &str, cwd: Option<&str>) -> String {
    match cwd {
        Some(dir) if !dir.is_empty() => {
            format!("cd '{}'; {}", dir.replace('\'', "''"), command)
        }
        _ => command.to_string(),
    }
}
