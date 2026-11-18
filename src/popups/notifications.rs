//! Popup under the bell: quick buttons, Do Not Disturb, pomodoro, updates and the notification
//! history (kept by the daemon, see notifier/); the bell in the bar.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::glib;
use gtk::prelude::*;
use serde_json::{json, Value};

use crate::common::*;

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// The bell in the bar: number of unread, Do Not Disturb. Called on every new notification.
pub fn refresh_bell() {
    let unread = crate::notifier::unread();
    let count = if unread > 0 { format!(" {unread}") } else { String::new() };
    let (icon, class, description) = if dnd_enabled() {
        ("\u{f1f6}", "dnd", "Do Not Disturb is on")
    } else if unread > 0 {
        ("\u{f0f3}", "unread", "")
    } else {
        ("\u{f0f3}", "none", "")
    };
    crate::panel::publish("notifications", json!({"text": format!("{icon}{count}"), "class": class, "tooltip": description}));
}

/// Do Not Disturb: toggle (None), on or off.
pub fn set_dnd(enable: Option<bool>) {
    crate::notifier::set_dnd(enable.unwrap_or_else(|| !dnd_enabled()));
    refresh_bell();
}

/// Notifications from the daemon's history, newest first.
fn load_list() -> Vec<Value> {
    crate::notifier::history().iter().map(crate::notifier::to_json).collect()
}

fn dnd_enabled() -> bool {
    crate::notifier::dnd()
}

struct Notification {
    o: Rc<Popup>,
    list: gtk::Box,
    clear_vse: gtk::Button,
}

pub fn fill(o: &Rc<Popup>) {
    // quick buttons, two rows of four
    let rada = gtk::Grid::new();
    rada.set_column_homogeneous(true);
    rada.set_column_spacing(8);
    rada.set_row_spacing(8);
    // what's on is lit: Wi-Fi, Bluetooth, night light, power saver
    let noc_enabled = night_mode();
    let wifi_enabled = wifi_enabled();
    let bt_enabled = bluetooth_enabled();
    let saver = mode_power() == "power-saver";
    let open = |kind: &'static str| move || {
        glib::idle_add_local_once(move || crate::popups::toggle(kind));
    };
    let buttons: Vec<(&str, &str, Box<dyn Fn()>, bool)> = vec![
        ("\u{f1eb}", "Wi-Fi", Box::new(open("wifi")), wifi_enabled),
        ("\u{f293}", "Bluetooth", Box::new(open("bluetooth")), bt_enabled),
        ("\u{f186}", "Night light", Box::new(|| background(&scripts().join("night-light.sh").to_string_lossy(), &["toggle"])), noc_enabled),
        ("\u{f06c}", "Battery and power mode", Box::new(open("battery")), saver),
        ("\u{f030}", "Screenshot", Box::new(open("screenshot")), false),
        ("\u{f03d}", "Screen recording", Box::new(open("recording")), crate::recording::recording()),
        ("\u{f03e}", "Wallpaper", Box::new(open("wallpapers")), false),
        ("\u{f023}", "Lock", Box::new(|| {
            crate::popups::close();
            background("gtklock", &["-d"]);
        }), false),
    ];
    for (i, (char, description, action, enabled)) in buttons.into_iter().enumerate() {
        let b = gtk::Button::with_label(char);
        b.set_tooltip_text(Some(description));
        b.add_css_class("quick");
        if enabled {
            b.add_css_class("enabled");
        }
        let switch = description == "Night light";
        b.connect_clicked(move |b| {
            action();
            if switch {
                if b.has_css_class("enabled") {
                    b.remove_css_class("enabled");
                } else {
                    b.add_css_class("enabled");
                }
            }
        });
        rada.attach(&b, (i % 4) as i32, (i / 4) as i32, 1, 1);
    }
    o.panel.append(&rada);

    // Do Not Disturb
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.append(&icon("\u{f1f6}", "icon"));
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texts.set_hexpand(true);
    texts.append(&label("To Not Disturb", &[]));
    texts.append(&label("Notifications stay quiet, critical ones still show", &["dim"]));
    row.append(&texts);
    let sw = gtk::Switch::new();
    sw.set_active(dnd_enabled());
    sw.set_valign(gtk::Align::Center);
    sw.connect_active_notify(|s| set_dnd(Some(s.is_active())));
    row.append(&sw);
    o.panel.append(&row);
    o.panel.append(&super::pomodoro::section(o));
    // updates: the bar icon is there only when there are some, here they are always visible
    let a = crate::updates::load();
    let count = a["dnf"].as_array().map_or(0, |x| x.len()) + a["flatpak"].as_array().map_or(0, |x| x.len());
    let state_updates = if crate::updates::checking() {
        "Checking…".to_string()
    } else if a["time"].as_u64().is_none() {
        "Not checked yet".to_string()
    } else if count == 0 {
        "Up to date".to_string()
    } else {
        format!("{count} available")
    };
    o.panel.append(&crate::common::row("\u{f019}", "Updates", &state_updates, count > 0, || {
        glib::idle_add_local_once(|| crate::popups::toggle("updates"));
    }));
    o.panel.append(&separator());

    // historie
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let n = label("Notifications", &["title"]);
    n.set_hexpand(true);
    header.append(&n);
    let clear_vse = gtk::Button::with_label("Clear all");
    clear_vse.add_css_class("link");
    header.append(&clear_vse);
    o.panel.append(&header);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    o.panel.append(&list);

    let nt = Rc::new(Notification { o: o.clone(), list, clear_vse: clear_vse.clone() });
    // weak: the Notification struct holds the button, a strong reference would never be freed
    let nt2 = Rc::downgrade(&nt);
    clear_vse.connect_clicked(move |_| {
        let Some(nt2) = nt2.upgrade() else { return };
        crate::notifier::clear_vse();
        refresh_bell();
        nt2.draw();
    });
    nt.draw();

    // opening marks everything read
    crate::notifier::mark_read();
    refresh_bell();
}

fn icon_apps(name: &str) -> gtk::Image {
    if name.starts_with('/') {
        let i = gtk::Image::from_file(name);
        i.set_pixel_size(32);
        return i;
    }
    let name = if name.is_empty() { "dialog-information" } else { name };
    let i = gtk::Image::from_icon_name(name);
    i.set_pixel_size(32);
    i
}

fn pred(time: f64) -> String {
    let diff = now() - time;
    if diff < 60.0 {
        "now".into()
    } else if diff < 3600.0 {
        format!("{} min ago", (diff / 60.0) as i64)
    } else if diff < 86400.0 {
        format!("{} h ago", (diff / 3600.0) as i64)
    } else {
        chrono::DateTime::from_timestamp(time as i64, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%-d %b").to_string())
            .unwrap_or_default()
    }
}

impl Notification {
    fn draw(self: &Rc<Self>) {
        while let Some(d) = self.list.first_child() {
            self.list.remove(&d);
        }
        let notifications = load_list();
        self.clear_vse.set_visible(!notifications.is_empty());
        if notifications.is_empty() {
            self.list.append(&label("No notifications", &["dim"]));
        }
        for n in notifications.iter().take(7) {
            let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            card.add_css_class("notifications");
            let ik = icon_apps(n["app_icon"].as_str().or(n["desktop_entry"].as_str()).unwrap_or(""));
            ik.set_valign(gtk::Align::Start);
            card.append(&ik);
            let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
            texts.set_hexpand(true);
            let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let apps = label(n["app_name"].as_str().unwrap_or(""), &["dim"]);
            apps.set_hexpand(true);
            top.append(&apps);
            top.append(&label(&pred(n["time"].as_f64().unwrap_or(now())), &["dim"]));
            let close = gtk::Button::with_label("\u{f00d}");
            close.add_css_class("close");
            let (id, nt) = (n["id"].as_i64().unwrap_or(0), self.clone());
            close.connect_clicked(move |_| {
                crate::notifier::clear(id as u32);
                refresh_bell();
                nt.draw();
            });
            top.append(&close);
            texts.append(&top);
            if let Some(s) = n["summary"].as_str().filter(|s| !s.is_empty()) {
                let l = label(s, &["title-notifications"]);
                l.set_wrap(true);
                l.set_max_width_chars(34);
                texts.append(&l);
            }
            if let Some(t) = n["body"].as_str().filter(|s| !s.is_empty()) {
                let l = label("", &["body"]);
                l.set_wrap(true);
                l.set_max_width_chars(34);
                l.set_lines(3);
                l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                // the body may have markup (<b>…), broken markup is shown as plain text
                if gtk::pango::parse_markup(t, '\0').is_ok() {
                    l.set_markup(t);
                } else {
                    l.set_text(t);
                }
                texts.append(&l);
            }
            card.append(&texts);
            self.list.append(&card);
        }
        if notifications.len() > 7 {
            self.list.append(&label(&format!("and {} more", notifications.len() - 7), &["dim"]));
        }
        self.o.recompute();
    }
}
