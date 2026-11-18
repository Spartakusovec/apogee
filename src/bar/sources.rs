//! Module data that waybar used to fetch itself: clock, volume, brightness, network and battery.
//! Everything goes to panel::publish like the other modules. Nothing is polled:
//! volume is reported by `pactl subscribe` (thread in microphone.rs), network by NetworkManager and battery by UPower
//! over D-Bus; only brightness is read from /sys every 5 s (mostly the keys change it, and they report it at once).

use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use gtk::gio;
use gtk::glib;

use serde_json::{json, Value};

use crate::panel::{escape, publish};
use crate::common::*;

pub fn start() {
    clock();
    volume_thread();
    brightness_to_panel();
    glib::timeout_add_seconds_local(5, || {
        brightness_to_panel();
        glib::ControlFlow::Continue
    });
    network();
    battery_watch();
    publish("power", json!({"text": "\u{f011}"}));
}

/// Clock format from Settings (`clock` in settings.json: date, seconds, h12).
pub fn format_clock() -> String {
    let h = crate::common::settings("clock");
    let datum = h["date"].as_bool().unwrap_or(true);
    let seconds = h["seconds"].as_bool().unwrap_or(true);
    let time = match (h["h12"].as_bool().unwrap_or(false), seconds) {
        (false, true) => "%H:%M:%S",
        (false, false) => "%H:%M",
        (true, true) => "%-I:%M:%S %p",
        (true, false) => "%-I:%M %p",
    };
    if datum { format!("%a %e %B   {time}") } else { time.to_string() }
}

thread_local! {
    static FORMAT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Shows the time right away (also after the format changes in Settings).
pub fn clock_now() -> chrono::DateTime<chrono::Local> {
    let now = chrono::Local::now();
    let f = FORMAT.with(|f| f.borrow_mut().get_or_insert_with(format_clock).clone());
    publish("clock", json!({"text": now.format(&f).to_string()}));
    now
}

pub fn change_format() {
    FORMAT.with(|f| f.replace(None));
    clock_now();
}

/// Clock every second, always at the start of the second (a timer would drift otherwise).
fn clock() {
    let now = clock_now();
    let to_next = 1000 - now.timestamp_subsec_millis().min(999) as u64;
    glib::timeout_add_local_once(Duration::from_millis(to_next + 2), clock);
}

static VOLUME: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// Called by the `pactl subscribe` thread when the sound output or the default device changed.
pub fn volume_changed() {
    if let Some(tx) = VOLUME.get() {
        let _ = tx.send(());
    }
}

/// Volume of the default output; several quick changes (a slider) are read once.
fn volume_thread() {
    let (tx, rx) = mpsc::channel::<()>();
    let _ = VOLUME.set(tx);
    std::thread::spawn(move || {
        let mut first = true;
        loop {
            if !first && rx.recv().is_err() {
                return;
            }
            first = false;
            std::thread::sleep(Duration::from_millis(30));
            while rx.try_recv().is_ok() {}
            let v = current(&[("pactl", &["-f", "json", "info"]), ("pactl", &["-f", "json", "list", "sinks"])]);
            let info: Value = serde_json::from_str(&v[0]).unwrap_or_default();
            let default = info["default_sink_name"].as_str().unwrap_or("");
            let sinks: Vec<Value> = serde_json::from_str(&v[1]).unwrap_or_default();
            let Some(s) = sinks.iter().find(|s| s["name"].as_str() == Some(default)) else { continue };
            let percent = s["volume"]
                .as_object()
                .and_then(|k| k.values().next())
                .and_then(|k| k["value_percent"].as_str())
                .and_then(|p| p.trim_end_matches('%').parse::<f64>().ok())
                .unwrap_or(0.0);
            if s["mute"].as_bool().unwrap_or(false) {
                publish("sound", json!({"text": "\u{f6a9}  muted", "class": "muted"}));
            } else {
                let icon = if percent < 33.4 { "\u{f026}" } else if percent < 66.7 { "\u{f027}" } else { "\u{f028}" };
                publish("sound", json!({"text": format!("{icon}  {percent:.0} %")}));
            }
        }
    });
}

/// Brightness from /sys (the backlight reads fast, unlike the battery).
pub fn brightness_to_panel() {
    let read = crate::common::read_backlight;
    if let (Some(j), Some(max)) = (read("brightness"), read("max_brightness")) {
        publish("brightness", json!({"text": format!("\u{f185}  {:.0} %", j / max.max(1.0) * 100.0)}));
    }
}

fn system() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok()
}

/// Network: Wi-Fi name, “wired” or “offline” (NetworkManager, on every change).
fn network() {
    network_to_panel();
    let Some(s) = system() else { return };
    let subscription = s.subscribe_to_signal(
        Some("org.freedesktop.NetworkManager"),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some("/org/freedesktop/NetworkManager"),
        None,
        gio::DBusSignalFlags::NONE,
        |_| network_to_panel(),
    );
    std::mem::forget(subscription); // for as long as the daemon runs
}

fn network_to_panel() {
    let primary = dbus_property(true, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager", "PrimaryConnection")
        .and_then(|v| v.str().map(String::from))
        .unwrap_or_else(|| "/".into());
    if primary == "/" {
        publish("network", json!({"text": "\u{f127}  offline", "class": "disconnected"}));
        return;
    }
    let m = dbus_vse(true, "org.freedesktop.NetworkManager", &primary, "org.freedesktop.NetworkManager.Connection.Active");
    let text = |k: &str| m.get(k).and_then(|v| v.str().map(String::from)).unwrap_or_default();
    let typ = text("Type");
    if typ.contains("wireless") {
        publish("network", json!({"text": format!("\u{f1eb}  {}", escape(&text("Id")))}));
    } else if typ.contains("ethernet") {
        publish("network", json!({"text": "\u{f6ff}  wired"}));
    } else {
        publish("network", json!({"text": format!("\u{f0c1}  {}", escape(&text("Id")))}));
    }
}

/// Battery: UPower reports every change of charge and state.
fn battery_watch() {
    battery_to_panel();
    let Some(s) = system() else { return };
    let subscription = s.subscribe_to_signal(
        Some("org.freedesktop.UPower"),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some("/org/freedesktop/UPower/devices/battery_BAT0"),
        None,
        gio::DBusSignalFlags::NONE,
        |_| battery_to_panel(),
    );
    std::mem::forget(subscription);
}

fn battery_to_panel() {
    let Some(b) = battery() else { return };
    let p = b.percent.round();
    let (icon, mut class) = match b.state {
        1 => ("\u{f0e7}", "charging"),
        4 | 5 => ("\u{f1e6}", "plugged"),
        _ => (["\u{f244}", "\u{f243}", "\u{f242}", "\u{f241}", "\u{f240}"][((p / 20.0) as usize).min(4)], ""),
    };
    if b.state != 1 {
        if p <= 10.0 {
            class = if class.is_empty() { "critical" } else { class };
        } else if p <= 25.0 && class.is_empty() {
            class = "warning";
        }
    }
    publish("battery", json!({"text": format!("{icon}  {p:.0} %"), "class": class}));
}
