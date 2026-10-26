//! Notice external saves without repainting idle windows. File contents are read only by the
//! engine's `file.refresh` command, on the session thread and inside the automation roots.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::time::Duration;

use figcraft_engine::{Session, file_access};
use figcraft_ui_egui::FigApp;
use serde_json::json;

type Stamp = Option<file_access::Stamp>;

pub struct Watch {
    tx: Sender<Vec<String>>,
    changed: Arc<AtomicBool>,
    paths: Vec<String>,
    revisions: Vec<(u64, u64)>,
}

impl Watch {
    pub fn start(wake: impl Fn() + Send + 'static, roots: Option<Arc<file_access::Roots>>) -> Option<Self> {
        let (tx, rx) = channel::<Vec<String>>();
        let changed = Arc::new(AtomicBool::new(true));
        let flag = changed.clone();
        let thread = std::thread::Builder::new().name("figcraft-files".into()).spawn(move || {
            let mut paths = Vec::new();
            let mut stamps = HashMap::new();
            loop {
                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(updated) => {
                        paths = updated;
                        stamps.retain(|path, _| paths.contains(path));
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
                let external = file_access::confined(roots.as_ref(), || scan(&paths, &mut stamps));
                if external {
                    flag.store(true, Ordering::Release);
                    wake();
                }
            }
        });
        match thread {
            Ok(_) => Some(Self { tx, changed, paths: Vec::new(), revisions: Vec::new() }),
            Err(e) => {
                log::warn!("file watcher: {e}");
                None
            }
        }
    }

    pub fn poll(&mut self, fig: &mut FigApp, ctx: &egui::Context) {
        let paths: Vec<_> = fig.session.docs.iter().filter_map(|d| d.path.clone()).collect();
        if paths != self.paths {
            let _ = self.tx.send(paths.clone());
            self.paths = paths;
        }
        // Retry after a gesture, preview or local edit ends, including undo back to the saved
        // state. A notification that arrived while editing must not be lost forever.
        let observed = revisions(&fig.session);
        let edited = observed != self.revisions;
        self.revisions = observed;
        if !self.changed.swap(false, Ordering::AcqRel) && !edited {
            return;
        }
        if let Some(result) = fig.run("file.refresh", json!({})) {
            if result["refreshed"].as_array().is_some_and(|v| !v.is_empty()) {
                ctx.request_repaint();
                for window in &fig.windows.list {
                    ctx.request_repaint_of(figcraft_ui_egui::windows::viewport(window.id));
                }
            }
            if let Some(errors) = result["errors"].as_array() {
                for error in errors.iter().filter_map(serde_json::Value::as_str) {
                    log::warn!("file refresh: {error}");
                    fig.toast(error.to_string());
                }
            }
        }
        self.revisions = revisions(&fig.session);
    }
}

fn revisions(session: &Session) -> Vec<(u64, u64)> {
    session.docs.iter().map(|d| (d.uid, d.revision)).collect()
}

/// First observation and any later replacement wake the app; a stable file costs no UI frame.
fn scan(paths: &[String], stamps: &mut HashMap<String, Stamp>) -> bool {
    let mut changed = false;
    for path in paths {
        let now = file_access::stamp(path).ok();
        if stamps.get(path) != Some(&now) {
            stamps.insert(path.clone(), now);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_replacements_wake_once_and_stable_files_stay_idle() {
        let dir = std::env::temp_dir().join(format!("figcraft-file-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("design.figcraft");
        let paths = vec![path.to_string_lossy().into_owned()];
        let mut stamps = HashMap::new();
        assert!(scan(&paths, &mut stamps));
        assert!(!scan(&paths, &mut stamps));
        std::fs::write(&path, b"first").unwrap();
        assert!(scan(&paths, &mut stamps));
        assert!(!scan(&paths, &mut stamps));
        let staged = dir.join("replacement");
        std::fs::write(&staged, b"second save").unwrap();
        std::fs::rename(&staged, &path).unwrap();
        assert!(scan(&paths, &mut stamps));
        assert!(!scan(&paths, &mut stamps));
        std::fs::remove_file(&path).unwrap();
        assert!(scan(&paths, &mut stamps));
        assert!(!scan(&paths, &mut stamps));
        let _ = std::fs::remove_dir_all(dir);
    }
}
