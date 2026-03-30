use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::provider::SessionProvider;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Emitter;

pub fn start_watcher(
    app: AppHandle,
    providers: &[Box<dyn SessionProvider>],
) -> Result<RecommendedWatcher, String> {
    let watch_paths: Vec<PathBuf> = providers
        .iter()
        .flat_map(|p| p.watch_paths())
        .filter(|p| p.exists())
        .collect();

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let changed_paths: Vec<String> = event
                    .paths
                    .iter()
                    .filter(|p| {
                        p.extension()
                            .is_some_and(|ext| ext == "jsonl" || ext == "json")
                    })
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();

                if !changed_paths.is_empty() {
                    let _ = app.emit("sessions-changed", changed_paths);
                }
            }
        },
        Config::default(),
    )
    .map_err(|e| format!("failed to create file watcher: {e}"))?;

    for path in &watch_paths {
        watcher
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| format!("failed to watch {}: {}", path.display(), e))?;
    }

    log::info!("Watching {} directories for changes", watch_paths.len());
    Ok(watcher)
}
