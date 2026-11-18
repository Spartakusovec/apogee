//! Display page: brightness, night light (night-light.conf, like the bar popup) and for every
//! screen the resolution, refresh rate, scale and on/off (only for a second monitor).
//! Resolution and scale ask “Keep?” for 15 s, otherwise they go back. Saved in settings.conf.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::Value;

use super::sway_conf;
use super::ui::*;
use crate::popups::brightness as night;
use crate::common::{in_background, command};

const SCALES: [f64; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 0.0];

fn brightness() -> f64 {
    let read = crate::common::read_backlight;
    match (read("brightness"), read("max_brightness")) {
        (Some(j), Some(m)) if m > 0.0 => (j / m * 100.0).round(),
        _ => 50.0,
    }
}

/// A slider that applies with a delay (so it doesn't send a hundred changes per second).
fn slider(from: f64, to_: f64, value: f64, delay: u64, set: impl Fn(f64) + 'static) -> gtk::Scale {
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, from, to_, 1.0);
    s.set_value(value);
    s.set_size_request(240, -1);
    s.set_draw_value(false);
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let set = Rc::new(set);
    s.connect_value_changed(move |s| {
        if let Some(id) = timer.take() {
            id.remove();
        }
        let (v, c, n) = (s.value(), timer.clone(), set.clone());
        timer.replace(Some(glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            c.replace(None);
            n(v);
        })));
    });
    s
}

pub fn create() -> gtk::Widget {
    let s = page("Display");

    let card = group(&s, "", "");
    add_row(&card, "\u{f185}", "Brightness", "", Some(slider(1.0, 100.0, brightness(), 30, crate::osd::set_brightness).upcast_ref()));

    // night light
    let conf = night::load();
    let mode = conf.get("mode").cloned().unwrap_or_else(|| "sunset".into());
    let temperature: f64 = conf.get("temperature").and_then(|t| t.parse().ok()).unwrap_or(4000.0);
    let card = group(&s, "Night light", "Warmer colors in the evening are easier on the eyes and help you fall asleep.");
    switch_row(&card, "Night light", "", mode != "off", |zap| {
        crate::common::background(&crate::common::scripts().join("night-light.sh").to_string_lossy(), &["toggle"]);
        super::notify(if zap { "Night light on" } else { "Night light off" });
        glib::timeout_add_local_once(Duration::from_millis(300), || super::refresh("display"));
    });
    if mode != "off" {
        let when = gtk::DropDown::from_strings(&["Sunset to sunrise", "All the time"]);
        when.set_selected((mode == "always") as u32);
        when.connect_selected_notify(|d| night::save("mode", if d.selected() == 1 { "always" } else { "sunset" }));
        let place = crate::popups::weather::location().map(|p| format!("Sunset and sunrise times for {}", p.0)).unwrap_or_else(|| "Set your location in Date & Time for sunset times".into());
        add_row(&card, "\u{f186}", "When", &place, Some(when.upcast_ref()));
        // slider: left 5500 K (almost neutral), right 2500 K (very warm)
        let t = slider(0.0, 100.0, (5500.0 - temperature) / 30.0, 350, |v| {
            let k = ((5500.0 - v * 30.0) / 100.0).round() * 100.0;
            night::save("temperature", &format!("{k}"));
        });
        add_row(&card, "\u{f2c9}", "Warmth", "", Some(t.upcast_ref()));
    }

    // obrazovky
    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.append(&wrapper);
    let o2 = wrapper.clone();
    in_background(
        || serde_json::from_str::<Value>(&command("swaymsg", &["-r", "-t", "get_outputs"])).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default(),
        move |outputs| {
            if o2.root().is_none() {
                return;
            }
            let vice = outputs.len() > 1;
            for v in &outputs {
                screen(&o2, v, vice);
            }
            if !vice {
                let k = group(&o2, "", "");
                add_row(&k, "\u{f26c}", "External monitor", "Plug in a monitor over HDMI and it shows up here.", None);
            }
        },
    );
    s.upcast()
}

fn screen(page: &gtk::Box, v: &Value, vice: bool) {
    let name = v["name"].as_str().unwrap_or("").to_string();
    let builtin = name.starts_with("eDP");
    let title = if builtin {
        "Built-in display".to_string()
    } else {
        let n = format!("{} {}", v["make"].as_str().unwrap_or(""), v["model"].as_str().unwrap_or("")).replace("Unknown", "");
        if n.trim().is_empty() { name.clone() } else { n.trim().to_string() }
    };
    let enabled = v["active"].as_bool().unwrap_or(true);
    let card = group(page, &title, "");
    if vice {
        let j = name.clone();
        switch_row(&card, "Use this display", "", enabled, move |z| {
            sway_conf::set(&format!("output {j}"), if z { "enable" } else { "disable" });
            glib::timeout_add_local_once(Duration::from_millis(800), || super::refresh("display"));
        });
        if !builtin && enabled {
            let j = name.clone();
            let mirror = gtk::DropDown::from_strings(&["Right of the laptop", "Left of the laptop", "Above the laptop"]);
            mirror.connect_selected_notify(move |d| place(&j, d.selected()));
            add_row(&card, "\u{f0db}", "Position", "", Some(mirror.upcast_ref()));
        }
    }
    if !enabled {
        return;
    }

    // resolution and refresh rate
    let modes: Vec<(i64, i64, i64)> = {
        let mut r: Vec<(i64, i64, i64)> = v["modes"].as_array().map(|m| m.iter().map(|m| (m["width"].as_i64().unwrap_or(0), m["height"].as_i64().unwrap_or(0), m["refresh"].as_i64().unwrap_or(0))).collect()).unwrap_or_default();
        r.sort_by(|a, b| (b.0 * b.1, b.2).cmp(&(a.0 * a.1, a.2)));
        r.dedup();
        r
    };
    let c = &v["current_mode"];
    let now = (c["width"].as_i64().unwrap_or(0), c["height"].as_i64().unwrap_or(0), c["refresh"].as_i64().unwrap_or(0));
    if modes.len() > 1 {
        let texts: Vec<String> = modes.iter().enumerate().map(|(i, m)| format!("{} × {}  ·  {:.0} Hz{}", m.0, m.1, m.2 as f64 / 1000.0, if i == 0 { "  (best)" } else { "" })).collect();
        let d = gtk::DropDown::from_strings(&texts.iter().map(String::as_str).collect::<Vec<_>>());
        d.set_selected(modes.iter().position(|m| *m == now).unwrap_or(0) as u32);
        let (j, r) = (name.clone(), modes.clone());
        let original = format!("{}x{}@{:.3}Hz", now.0, now.1, now.2 as f64 / 1000.0);
        d.connect_selected_notify(move |d| {
            let m = r[d.selected() as usize];
            let new = format!("{}x{}@{:.3}Hz", m.0, m.1, m.2 as f64 / 1000.0);
            try_out(&format!("output {j} mode"), &new, &original);
        });
        add_row(&card, "\u{f108}", "Resolution", "", Some(d.upcast_ref()));
    } else {
        add_row(&card, "\u{f108}", "Resolution", &format!("{} × {}  ·  {:.0} Hz", now.0, now.1, now.2 as f64 / 1000.0), None);
    }

    // scale
    let scale = v["scale"].as_f64().unwrap_or(1.0);
    let mut choices: Vec<String> = SCALES[..5].iter().map(|m| format!("{:.0} %", m * 100.0)).collect();
    let mut values: Vec<f64> = SCALES[..5].to_vec();
    if !values.iter().any(|m| (m - scale).abs() < 0.01) {
        choices.push(format!("{:.0} %", scale * 100.0));
        values.push(scale);
    }
    let d = gtk::DropDown::from_strings(&choices.iter().map(String::as_str).collect::<Vec<_>>());
    d.set_selected(values.iter().position(|m| (m - scale).abs() < 0.01).unwrap_or(0) as u32);
    let j = name.clone();
    d.connect_selected_notify(move |d| {
        let m = values[d.selected() as usize];
        try_out(&format!("output {j} scale"), &format!("{m}"), &format!("{scale}"));
    });
    add_row(&card, "\u{f00e}", "Scale", "Makes everything bigger. For just bigger text, see Appearance → Text size.", Some(d.upcast_ref()));
}

/// A second monitor next to the laptop (position from the built-in screen's width).
fn place(name: &str, target: u32) {
    let outputs: Vec<Value> = serde_json::from_str::<Value>(&command("swaymsg", &["-r", "-t", "get_outputs"])).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let rect = |n: &Value| (n["rect"]["width"].as_i64().unwrap_or(1920), n["rect"]["height"].as_i64().unwrap_or(1080));
    let Some(notebook) = outputs.iter().find(|v| v["name"].as_str().is_some_and(|n| n.starts_with("eDP"))) else { return };
    let Some(monitor) = outputs.iter().find(|v| v["name"].as_str() == Some(name)) else { return };
    let ((nw, _), (mw, mh)) = (rect(notebook), rect(monitor));
    let nj = notebook["name"].as_str().unwrap_or("eDP-1");
    let (n_pos, m_pos) = match target {
        1 => (format!("{mw} 0"), "0 0".to_string()),
        2 => (format!("0 {mh}"), "0 0".to_string()),
        _ => ("0 0".to_string(), format!("{nw} 0")),
    };
    sway_conf::set(&format!("output {nj} position"), &n_pos);
    sway_conf::set(&format!("output {name} position"), &m_pos);
    super::notify("Saved");
}

/// Applies the display settings at once, but saves them only if the user confirms within 15 s.
fn try_out(key: &str, new: &str, original: &str) {
    crate::sway::command(&format!("{key} {new}"));
    let cancel = gtk::gio::Cancellable::new();
    let z2 = cancel.clone();
    let timer = glib::timeout_add_local_once(Duration::from_secs(15), move || z2.cancel());
    let dialog = gtk::AlertDialog::builder()
        .message("Keep these display settings?")
        .detail("They go back in 15 seconds if you don't choose.")
        .buttons(["Go back", "Keep"])
        .default_button(1)
        .cancel_button(0)
        .modal(true)
        .build();
    let window = super::window();
    let (k, n, p) = (key.to_string(), new.to_string(), original.to_string());
    let timer = RefCell::new(Some(timer));
    dialog.choose(window.as_ref(), Some(&cancel), move |v| {
        if let Some(c) = timer.take() {
            if !cancel_expired(&v) {
                c.remove();
            }
        }
        if v == Ok(1) {
            sway_conf::set(&k, &n);
            super::notify("Saved");
        } else {
            crate::sway::command(&format!("{k} {p}"));
            super::notify("Went back to the previous settings");
        }
        glib::timeout_add_local_once(Duration::from_millis(500), || super::refresh("display"));
    });
}

/// The dialog ended by cancellation after 15 s (the timer already ran out, it must not be removed again).
fn cancel_expired(v: &Result<i32, glib::Error>) -> bool {
    matches!(v, Err(e) if e.matches(gtk::gio::IOErrorEnum::Cancelled))
}
