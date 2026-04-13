use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::provider::SessionProvider;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use tauri::AppHandle;
use tauri::Emitter;

/// How long to wait after the last file-change event before emitting a
/// batched `sessions-changed` event to the frontend.
const DEBOUNCE_MS: u64 = 500;

pub fn start_watcher(
    app: AppHandle,
    providers: &[Box<dyn SessionProvider>],
) -> Result<RecommendedWatcher, String> {
    let watch_paths: Vec<PathBuf> = providers
        .iter()
        .flat_map(|p| p.watch_paths())
        .filter(|p| p.exists())
        .collect();

    // Channel for forwarding changed paths from the notify callback to the
    // debounce thread. The notify callback must be non-blocking, so we just
    // send paths and let the background thread accumulate them.
    let (tx, rx) = mpsc::channel::<Vec<String>>();

    // Background thread: collect changed paths and flush them as a single
    // batched event once no new changes arrive within the debounce window.
    std::thread::Builder::new()
        .name("watcher-debounce".into())
        .spawn(move || {
            let debounce = Duration::from_millis(DEBOUNCE_MS);
            let mut pending = HashSet::<String>::new();

            loop {
                // If nothing is pending, block until the first change arrives.
                // If something IS pending, wait up to `debounce` for more.
                let recv_result = if pending.is_empty() {
                    rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                } else {
                    rx.recv_timeout(debounce)
                };

                match recv_result {
                    Ok(paths) => {
                        pending.extend(paths);
                        // Drain any other paths that arrived in the meantime.
                        while let Ok(more) = rx.try_recv() {
                            pending.extend(more);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        // Debounce window elapsed — flush the batch.
                        if !pending.is_empty() {
                            let batch: Vec<String> = pending.drain().collect();
                            let _ = app.emit("sessions-changed", batch);
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        // Watcher was dropped; flush remaining and exit.
                        if !pending.is_empty() {
                            let batch: Vec<String> = pending.drain().collect();
                            let _ = app.emit("sessions-changed", batch);
                        }
                        break;
                    }
                }
            }
        })
        .map_err(|e| format!("failed to spawn debounce thread: {e}"))?;

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
                    let _ = tx.send(changed_paths);
                }
            }
        },
        Config::default(),
    )
    .map_err(|e| format!("failed to create file watcher: {e}"))?;

    let mut watched_count = 0usize;
    for path in &watch_paths {
        match watcher.watch(path, RecursiveMode::Recursive) {
            Ok(()) => watched_count += 1,
            Err(e) => {
                log::warn!("failed to watch {}: {}", path.display(), e);
            }
        }
    }

    if !watch_paths.is_empty() && watched_count == 0 {
        return Err("failed to watch any provider directory".to_string());
    }

    log::info!(
        "Watching {}/{} directories for changes",
        watched_count,
        watch_paths.len()
    );
    Ok(watcher)
}
