//! Top bar modules: every source (bell, weather, workspaces…) sends its state here as JSON
//! {text, class, tooltip}. The daemon's own bar (bar/) gets it directly; for waybar as a fallback
//! `apogee modul <name>` can print the same as JSON lines (return-type json).

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;

static SUBSCRIBERS: Mutex<Option<HashMap<String, Vec<UnixStream>>>> = Mutex::new(None);
/// the daemon's bar (GTK main thread): (module, JSON)
static BAR: std::sync::OnceLock<async_channel::Sender<(String, String)>> = std::sync::OnceLock::new();
static VALUES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// A new module value: saves it and sends it to all connected (drops disconnected ones).
pub fn publish(module: &str, json: serde_json::Value) {
    let row = json.to_string() + "\n";
    {
        let mut h = VALUES.lock().unwrap();
        let h = h.get_or_insert_with(HashMap::new);
        if h.get(module) == Some(&row) {
            return; // nothing changed, don't redraw the bar
        }
        h.insert(module.to_string(), row.clone());
    }
    if let Some(tx) = BAR.get() {
        let _ = tx.try_send((module.to_string(), row.trim_end().to_string()));
    }
    let mut o = SUBSCRIBERS.lock().unwrap();
    if let Some(list) = o.get_or_insert_with(HashMap::new).get_mut(module) {
        list.retain_mut(|s| s.write_all(row.as_bytes()).is_ok());
    }
}

/// A bar module connected: send it the last value right away.
pub fn add_subscriber(module: &str, mut connection: UnixStream) {
    if let Some(row) = VALUES.lock().unwrap().get_or_insert_with(HashMap::new).get(module) {
        if connection.write_all(row.as_bytes()).is_err() {
            return;
        }
    }
    SUBSCRIBERS.lock().unwrap().get_or_insert_with(HashMap::new).entry(module.to_string()).or_default().push(connection);
}

/// HTML escaping for text in waybar (Pango markup).
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The daemon's bar: changes of all modules (all current values first).
pub fn subscribe() -> async_channel::Receiver<(String, String)> {
    let (tx, rx) = async_channel::unbounded();
    // under the lock: whatever is published meanwhile arrives after these values
    let mut h = VALUES.lock().unwrap();
    for (m, r) in h.get_or_insert_with(HashMap::new).iter() {
        let _ = tx.try_send((m.clone(), r.trim_end().to_string()));
    }
    let _ = BAR.set(tx);
    rx
}
