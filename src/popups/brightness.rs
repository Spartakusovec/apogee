//! Brightness and night light (settings in ~/.config/sway/night-light.conf, run by night-light.sh).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::common::*;

fn file() -> std::path::PathBuf {
    home().join(".config/sway/night-light.conf")
}

pub fn load() -> HashMap<String, String> {
    std::fs::read_to_string(file())
        .unwrap_or_default()
        .lines()
        .filter_map(|r| r.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

/// Saves one value and restarts night light with the settings.
pub fn save(key: &str, value: &str) {
    let mut rows: Vec<String> = std::fs::read_to_string(file())
        .unwrap_or_default()
        .lines()
        .filter(|r| !r.starts_with(&format!("{key}=")))
        .map(String::from)
        .collect();
    rows.push(format!("{key}={value}"));
    let _ = std::fs::write(file(), rows.join("\n") + "\n");
    background(&scripts().join("night-light.sh").to_string_lossy(), &[]);
}

pub fn fill(o: &Rc<Popup>) {
    let brightness: f64 = command("brightnessctl", &["-m"])
        .split(',')
        .nth(3)
        .and_then(|s| s.trim_end_matches('%').parse().ok())
        .unwrap_or(50.0);
    o.panel.append(&title("Display", ""));
    let (row_brightness, _) = slider(&icon("\u{f185}", "icon"), brightness, 1.0, 100.0, percent, |v| {
        background("brightnessctl", &["-q", "set", &format!("{}%", v.round())]);
    });
    o.panel.append(&row_brightness);
    o.panel.append(&separator());

    let conf = load();
    let mode = conf.get("mode").cloned().unwrap_or_else(|| "sunset".into());
    let temperature: f64 = conf.get("temperature").and_then(|t| t.parse().ok()).unwrap_or(4000.0);

    // switch
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.append(&icon("\u{f186}", "icon"));
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texts.append(&label("Night light", &[]));
    texts.append(&label("Warmer colors in the evening, easier on the eyes", &["dim"]));
    texts.set_hexpand(true);
    row.append(&texts);
    let switcher = gtk::Switch::new();
    switcher.set_active(mode != "off");
    switcher.set_valign(gtk::Align::Center);
    row.append(&switcher);
    o.panel.append(&row);

    let settings = gtk::Box::new(gtk::Orientation::Vertical, 10);
    o.panel.append(&settings);

    // temperature: the slider shows how warm the light is, left 5500 K, full = 2500 K
    let row_t = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row_t.append(&label("Cooler", &["dim"]));
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_value((5500.0 - temperature) / 30.0);
    let wait: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    scale.connect_value_changed(move |s| {
        let kelvin = ((5500.0 - s.value() * 30.0) / 100.0).round() * 100.0;
        if let Some(id) = wait.take() {
            id.remove();
        }
        let c = wait.clone();
        wait.replace(Some(glib::timeout_add_local_once(Duration::from_millis(350), move || {
            c.replace(None);
            save("temperature", &format!("{kelvin}"));
        })));
    });
    row_t.append(&scale);
    row_t.append(&label("Warmer", &["dim"]));
    settings.append(&row_t);

    // when: from sunset / all the time
    let row_k = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row_k.set_homogeneous(true);
    let buttons: Rc<Vec<(&'static str, gtk::Button)>> = Rc::new(
        [("sunset", "Sunset to sunrise"), ("always", "Always on")]
            .iter()
            .map(|(k, t)| (*k, gtk::Button::with_label(t)))
            .collect(),
    );
    let mark = {
        let t = buttons.clone();
        move |active: &str| {
            for (k, b) in t.iter() {
                if *k == active {
                    b.add_css_class("enabled");
                } else {
                    b.remove_css_class("enabled");
                }
            }
        }
    };
    for (key, b) in buttons.iter() {
        let key = *key;
        let mark = mark.clone();
        b.connect_clicked(move |_| {
            mark(key);
            save("mode", key);
        });
        row_k.append(b);
    }
    let last = conf.get("last").cloned().unwrap_or_else(|| "sunset".into());
    mark(if mode == "off" { &last } else { &mode });
    settings.append(&row_k);
    settings.set_sensitive(mode != "off");

    let t = buttons.clone();
    switcher.connect_active_notify(move |sw| {
        let selected = t.iter().find(|(_, b)| b.has_css_class("enabled")).map(|(k, _)| *k).unwrap_or("sunset");
        if sw.is_active() {
            save("mode", selected);
        } else {
            save("last", selected);
            save("mode", "off");
        }
        settings.set_sensitive(sw.is_active());
    });
}
