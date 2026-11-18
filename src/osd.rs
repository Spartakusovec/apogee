//! Volume and brightness indicator (keys) and short messages (tiling) at the bottom above the dock.
//! Shows the value at once; volume is set by wpctl in the background (from quick key presses
//! only the last value), brightness by logind over D-Bus. It replaced osd.sh with a mako notification (~120 ms).

use std::cell::{Cell, RefCell};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::common::command;

const STEP: f64 = 5.0;

struct Indicator {
    window: gtk::Window,
    icon: gtk::Label,
    text: gtk::Label,
    level: gtk::LevelBar,
    hide: Option<glib::SourceId>,
}

thread_local! {
    static INDICATOR: RefCell<Option<Indicator>> = const { RefCell::new(None) };
    /// volume in % and mute, when they were last touched
    static VOLUME: Cell<Option<(f64, bool, Instant)>> = const { Cell::new(None) };
    static SETTER: RefCell<Option<mpsc::Sender<(f64, bool)>>> = const { RefCell::new(None) };
    /// brightness in %, when it was set (sysfs changes only once logind really sets it)
    static BRIGHTNESS: Cell<Option<(f64, Instant)>> = const { Cell::new(None) };
}

/// Shows the indicator (value in %, None = text only) and hides it after 1.5 s.
pub fn show(icon: &str, text: &str, value: Option<f64>) {
    INDICATOR.with(|u| {
        let mut u = u.borrow_mut();
        let u = u.get_or_insert_with(create);
        u.icon.set_text(icon);
        u.text.set_text(text);
        u.level.set_visible(value.is_some());
        u.level.set_value(value.unwrap_or(0.0));
        if let Some(id) = u.hide.take() {
            id.remove();
        }
        u.window.set_default_size(1, 1);
        u.window.set_visible(true);
        u.hide = Some(glib::timeout_add_local_once(Duration::from_millis(1500), || {
            INDICATOR.with(|u| {
                if let Some(u) = u.borrow_mut().as_mut() {
                    u.hide = None;
                    u.window.set_visible(false);
                }
            });
            crate::common::return_memory();
        }));
    });
}

fn create() -> Indicator {
    crate::style::apply();
    let window = gtk::Window::new();
    window.add_css_class("apogee");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_namespace(Some("osd"));
    window.set_anchor(Edge::Bottom, true);
    window.set_margin(Edge::Bottom, 110); // nad dokem
    let panel = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    panel.set_widget_name("panel");
    panel.add_css_class("osd");
    let icon = crate::common::icon("", "icon");
    let level = gtk::LevelBar::for_interval(0.0, 100.0);
    level.set_mode(gtk::LevelBarMode::Continuous);
    for name in ["low", "high", "full"] {
        level.remove_offset_value(Some(name));
    }
    level.set_size_request(180, -1);
    level.set_valign(gtk::Align::Center);
    let text = crate::common::label("", &[]);
    panel.append(&icon);
    panel.append(&level);
    panel.append(&text);
    window.set_child(Some(&panel));
    Indicator { window, icon, text, level, hide: None }
}

/// Volume: the vol-up, vol-down, vol-mute keys.
pub fn volume(action: &str) {
    // if nothing happened with the volume for a while, it may have changed elsewhere: read it again
    let (mut volume, mut muted) = match VOLUME.with(|h| h.get()) {
        Some((h, z, when)) if when.elapsed() < Duration::from_secs(3) => (h, z),
        _ => {
            let v = command("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]);
            let h = v.split_whitespace().nth(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) * 100.0;
            (h.round(), v.contains("MUTED"))
        }
    };
    match action {
        "vol-up" => {
            volume = (volume + STEP).min(100.0);
            muted = false;
        }
        "vol-down" => volume = (volume - STEP).max(0.0),
        "vol-mute" => muted = !muted,
        _ => return,
    }
    VOLUME.with(|h| h.set(Some((volume, muted, Instant::now()))));
    set_volume(volume, muted);
    if muted {
        show("\u{f6a9}", "Muted", Some(0.0));
    } else {
        let icon = if volume == 0.0 { "\u{f026}" } else if volume < 50.0 { "\u{f027}" } else { "\u{f028}" };
        show(icon, &format!("{volume:.0} %"), Some(volume));
    }
}

/// wpctl in the background; from quick key presses only the last value is set.
fn set_volume(volume: f64, muted: bool) {
    SETTER.with(|n| {
        let mut n = n.borrow_mut();
        let sender = n.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<(f64, bool)>();
            std::thread::spawn(move || {
                while let Ok(mut last) = rx.recv() {
                    while let Ok(newer) = rx.try_recv() {
                        last = newer;
                    }
                    let (h, z) = last;
                    command("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", h / 100.0)]);
                    command("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", if z { "1" } else { "0" }]);
                }
            });
            tx
        });
        let _ = sender.send((volume, muted));
    });
}

fn read(file: &str) -> f64 {
    crate::common::read_backlight(file).unwrap_or(0.0)
}

/// Brightness: the brightness-up, brightness-down keys (5 % steps, at least 1 %, so the screen never goes black).
pub fn brightness(action: &str) {
    let max = read("max_brightness").max(1.0);
    let now = match BRIGHTNESS.with(|j| j.get()) {
        Some((j, when)) if when.elapsed() < Duration::from_secs(2) => j,
        _ => (read("brightness") / max * 100.0).round(),
    };
    let new = match action {
        "brightness-up" => (now + STEP).min(100.0),
        "brightness-down" => (now - STEP).max(1.0),
        _ => return,
    };
    set_brightness(new);
    show("\u{f185}", &format!("{:.0} %", new.round()), Some(new));
}

/// Brightness in % (1–100) through logind, without the indicator (slider in the Settings app).
pub fn set_brightness(new: f64) {
    let new = new.clamp(1.0, 100.0);
    let max = read("max_brightness").max(1.0);
    BRIGHTNESS.with(|j| j.set(Some((new, Instant::now()))));
    let value = (new / 100.0 * max).round() as u32;
    if let Ok(s) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
        s.call(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            "SetBrightness",
            Some(&("backlight", crate::common::backlight().unwrap_or_default(), value).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
            |_| {},
        );
    }
    crate::panel::publish("brightness", serde_json::json!({"text": format!("\u{f185}  {:.0} %", new.round())}));
}
