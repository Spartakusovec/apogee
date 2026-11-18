//! App name and icon from its .desktop file (same as the app launcher).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone)]
pub struct Info {
    pub name: String,
    pub icon: String,
}

struct Index {
    /// file name without .desktop (lower case) → path
    by_files: HashMap<String, PathBuf>,
    /// StartupWMClass (lower case) → path
    by_classes: HashMap<String, PathBuf>,
    done: HashMap<String, Info>,
}

static INDEX: Mutex<Option<Index>> = Mutex::new(None);

fn folders() -> Vec<PathBuf> {
    vec![
        crate::common::home().join(".local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/usr/share/applications"),
    ]
}

fn build() -> Index {
    let mut r = Index { by_files: HashMap::new(), by_classes: HashMap::new(), done: HashMap::new() };
    for folder in folders() {
        let Ok(files) = std::fs::read_dir(&folder) else { continue };
        for s in files.flatten() {
            let path = s.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".desktop")) else {
                continue;
            };
            r.by_files.entry(name.to_lowercase()).or_insert_with(|| path.clone());
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Some(class) = content.lines().find_map(|l| l.strip_prefix("StartupWMClass=")) {
                    r.by_classes.entry(class.trim().to_lowercase()).or_insert(path);
                }
            }
        }
    }
    r
}

/// The index is built in the background right after start, so Alt+Tab is fast the first time too.
pub fn load_on_background() {
    std::thread::spawn(|| {
        let r = build();
        *INDEX.lock().unwrap() = Some(r);
    });
}

fn read(path: &PathBuf, id: &str) -> Info {
    let mut name = None;
    let mut icon = None;
    let mut section = false;
    for row in std::fs::read_to_string(path).unwrap_or_default().lines() {
        if row.starts_with('[') {
            section = row == "[Desktop Entry]";
        } else if section {
            if let Some(v) = row.strip_prefix("Name=") {
                name.get_or_insert_with(|| v.to_string());
            } else if let Some(v) = row.strip_prefix("Icon=") {
                icon.get_or_insert_with(|| v.to_string());
            }
        }
    }
    Info { name: name.unwrap_or_else(|| id.to_string()), icon: icon.unwrap_or_else(|| "application-x-executable".into()) }
}

fn find(r: &Index, app_id: &str) -> Option<PathBuf> {
    let small = app_id.to_lowercase();
    r.by_files
        .get(&small)
        .or_else(|| r.by_classes.get(&small))
        // app_id doesn't always match the file name (Discord, Spotify…): try the end of the name
        .or_else(|| r.by_files.iter().find(|(k, _)| k.ends_with(&format!(".{small}"))).map(|(_, v)| v))
        .cloned()
}

/// The app's .desktop file for a window's app_id (None = there is none).
pub fn path(app_id: &str) -> Option<PathBuf> {
    let mut lock = INDEX.lock().unwrap();
    let r = lock.get_or_insert_with(build);
    find(r, app_id)
}

pub fn info(app_id: &str) -> Info {
    let mut lock = INDEX.lock().unwrap();
    let r = lock.get_or_insert_with(build);
    if let Some(i) = r.done.get(app_id) {
        return i.clone();
    }
    let path = find(r, app_id);
    let i = match path {
        Some(c) => read(&c, app_id),
        None => Info { name: app_id.to_string(), icon: "application-x-executable".into() },
    };
    r.done.insert(app_id.to_string(), i.clone());
    i
}
