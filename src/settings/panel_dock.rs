//! Panel & Dock page: which modules the bar shows, clock format, when the dock hides
//! and the order of the pinned apps in the dock.

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::json;

use super::ui::*;
use crate::common::{settings, save_settings};

/// modules that can be hidden: (id in the bar, name, description)
const MODULES: &[(&str, &str, &str)] = &[
    ("tiling", "Tiling button", "Switches the workspace between floating and tiled windows"),
    ("pomodoro", "Pomodoro timer", "Only while a timer runs"),
    ("music", "Music", "What is playing, with previous / next"),
    ("weather", "Weather", ""),
    ("sound", "Volume", ""),
    ("brightness", "Brightness", ""),
    ("network", "Wi-Fi", ""),
    ("battery", "Battery", ""),
    ("updates", "Updates", "Only when updates are waiting"),
    ("notifications", "Notifications", ""),
];

fn hidden_modules() -> Vec<String> {
    settings("panel_hidden").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}

fn clock_switcher(card: &gtk::ListBox, key: &'static str, default: bool, name: &str, description: &str) {
    let enabled = settings("clock")[key].as_bool().unwrap_or(default);
    switch_row(card, name, description, enabled, move |z| {
        let mut h = settings("clock");
        if !h.is_object() {
            h = json!({});
        }
        h[key] = json!(z);
        save_settings("clock", h);
        crate::bar::sources::change_format();
    });
}

pub fn create() -> gtk::Widget {
    let s = page("Panel & Dock");

    let card = group(&s, "Clock", "");
    clock_switcher(&card, "date", true, "Show the date", "");
    clock_switcher(&card, "seconds", true, "Show seconds", "");
    clock_switcher(&card, "h12", false, "12-hour time", "3:45 PM instead of 15:45");

    let card = group(&s, "Panel", "The app name, workspaces, clock and power button are always there.");
    let hidden = hidden_modules();
    for (id, name, description) in MODULES {
        switch_row(&card, name, description, !hidden.iter().any(|x| x == id), move |z| {
            let mut v = hidden_modules();
            v.retain(|x| x != id);
            if !z {
                v.push(id.to_string());
            }
            save_settings("panel_hidden", json!(v));
            crate::bar::refresh_modules();
        });
    }

    let card = group(&s, "Dock", "");
    let modes = [
        ("smart", "When a window is in the way"),
        ("never", "Never – always visible"),
        ("always", "Always – show with Super + X or the bottom edge"),
    ];
    let now = crate::dock::hiding();
    let d = gtk::DropDown::from_strings(&modes.iter().map(|r| r.1).collect::<Vec<_>>());
    d.set_selected(modes.iter().position(|r| r.0 == now).unwrap_or(0) as u32);
    d.connect_selected_notify(move |d| {
        crate::dock::set_hiding(modes[d.selected() as usize].0);
        super::notify("Saved");
    });
    add_row(&card, "\u{f150}", "Hide the dock", "", Some(d.upcast_ref()));

    let card = group(&s, "Pinned apps", "Pin more apps by right-clicking them in the dock or in the app grid.");
    let pinned = crate::dock::pinned();
    if pinned.is_empty() {
        add_row(&card, "", "No pinned apps", "", None);
    }
    for (i, key) in pinned.iter().enumerate() {
        let info = crate::dock::info_apps(key);
        let name = info.as_ref().map(|a| a.display_name().to_string()).unwrap_or_else(|| key.clone());
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        r.add_css_class("row");
        let icon = gtk::Image::new();
        icon.set_pixel_size(28);
        if let Some(i) = info.as_ref().and_then(|a| a.icon()) {
            icon.set_from_gicon(&i);
        }
        r.append(&icon);
        let l = gtk::Label::new(Some(&name));
        l.set_hexpand(true);
        l.set_xalign(0.0);
        r.append(&l);
        let offset = |direction: i32| {
            let list = pinned.clone();
            move || {
                let mut s = list.clone();
                let j = i as i32 + direction;
                if j >= 0 && (j as usize) < s.len() {
                    s.swap(i, j as usize);
                    crate::dock::set_pinned(&s);
                    glib::timeout_add_local_once(Duration::from_millis(100), || super::refresh("panel_dock"));
                }
            }
        };
        let up = button("\u{2191}", false, offset(-1));
        up.set_sensitive(i > 0);
        up.set_tooltip_text(Some("Move left"));
        let down = button("\u{2193}", false, offset(1));
        down.set_sensitive(i + 1 < pinned.len());
        down.set_tooltip_text(Some("Move right"));
        let list = pinned.clone();
        let unpin = button("Unpin", false, move || {
            let mut s = list.clone();
            s.remove(i);
            crate::dock::set_pinned(&s);
            glib::timeout_add_local_once(Duration::from_millis(100), || super::refresh("panel_dock"));
        });
        r.append(&up);
        r.append(&down);
        r.append(&unpin);
        let row = gtk::ListBoxRow::new();
        row.set_activatable(false);
        row.set_child(Some(&r));
        card.append(&row);
    }
    s.upcast()
}
