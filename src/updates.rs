//! Updates: every 6 hours, checks what's new in dnf and Flatpak (in the background, at low
//! priority). The bar shows an icon with the count only when there is something; click = popup with the list.
//! The result is remembered in ~/.cache/apogee/updates.json; a daemon restart doesn't check again.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::glib;
use serde_json::{json, Value};

use crate::panel::publish;
use crate::common::*;

const INTERVAL: u64 = 6 * 3600;

thread_local! {
    static CHECKING: Cell<bool> = const { Cell::new(false) };
}

fn file() -> PathBuf {
    home().join(".cache/apogee/updates.json")
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The last result: {"time": …, "dnf": [[name, version]…], "flatpak": [[name, version]…]}
pub fn load() -> Value {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}))
}

pub fn checking() -> bool {
    CHECKING.with(|k| k.get())
}

fn count(v: &Value) -> usize {
    v["dnf"].as_array().map_or(0, |a| a.len()) + v["flatpak"].as_array().map_or(0, |a| a.len())
}

fn to_panel(v: &Value) {
    let n = count(v);
    publish("updates", if n > 0 { json!({"text": format!("\u{f019}  {n}"), "class": "available"}) } else { json!({"text": ""}) });
}

/// Finds the updates (slow: downloads the package lists, call outside GTK).
fn detect() -> Value {
    // lines “name.arch  version  repository” (dnf returns 100 when there are some)
    let dnf: Vec<Value> = command("nice", &["-n", "19", "dnf", "check-update", "-q"])
        .lines()
        .filter_map(|r| {
            let parts: Vec<&str> = r.split_whitespace().collect();
            (parts.len() == 3 && parts[0].contains('.')).then(|| json!([parts[0].rsplit_once('.').map_or(parts[0], |(n, _)| n), parts[1]]))
        })
        .collect();
    // “flatpak remote-ls --updates” also lists things flatpak update doesn't update (other runtime
    // branches), so ask flatpak update directly and answer the question with “n”:
    // lines “ 1.  org.inkscape.Inkscape  stable  u  fedora  < 52 MB”
    let flatpak: Vec<Value> = {
        use std::io::Write;
        use std::process::{Command, Stdio};
        Command::new("nice")
            .args(["-n", "19", "flatpak", "update"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut p| {
                if let Some(mut input) = p.stdin.take() {
                    let _ = input.write_all(b"n\n");
                }
                p.wait_with_output()
            })
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default()
            .lines()
            .filter_map(|r| {
                let parts: Vec<&str> = r.split_whitespace().collect();
                let order = parts.first()?.strip_suffix('.')?;
                order.parse::<u32>().ok()?;
                let id = parts.iter().skip(1).find(|c| c.contains('.'))?;
                let i = parts.iter().position(|c| c == id)?;
                Some(json!([id.rsplit('.').next().unwrap_or(id), parts.get(i + 1).copied().unwrap_or("")]))
            })
            .collect()
    };
    json!({"time": now(), "dnf": dnf, "flatpak": flatpak})
}

/// Check right now (button in the popup, after installing); `done` gets the new result.
pub fn check(done: impl FnOnce(Value) + 'static) {
    if CHECKING.with(|k| k.replace(true)) {
        return;
    }
    in_background(detect, move |v| {
        CHECKING.with(|k| k.set(false));
        if let Some(s) = file().parent() {
            let _ = std::fs::create_dir_all(s);
        }
        let _ = std::fs::write(file(), v.to_string());
        to_panel(&v);
        done(v);
    });
}

pub fn watch() {
    to_panel(&load());
    // check by the real time (also after waking from sleep), the first time only after 5 minutes,
    // so the login isn't slowed down
    glib::timeout_add_seconds_local(300, || {
        if now().saturating_sub(load()["time"].as_u64().unwrap_or(0)) >= INTERVAL {
            check(|_| {});
        }
        glib::ControlFlow::Continue
    });
}

/// Opens a terminal with the update (sudo wants a password), and checks again at the end.
pub fn install() {
    let script = "sudo dnf upgrade --refresh; echo; flatpak update; ~/.local/bin/apogee check-updates; \
                  echo; read -rp 'Done. Press Enter to close this window.'";
    background("ptyxis", &["--new-window", "-T", "System update", "--", "bash", "-c", script]);
}
