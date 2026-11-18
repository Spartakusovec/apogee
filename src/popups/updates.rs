//! Updates: list of packages (dnf) and apps (Flatpak), install in a terminal, check again.

use std::rc::Rc;

use gtk::prelude::*;
use serde_json::Value;

use crate::updates;
use crate::common::*;

pub fn fill(o: &Rc<Popup>) {
    show(o, &updates::load());
}

fn when(time: u64) -> String {
    if time == 0 {
        return "Not checked yet".into();
    }
    chrono::DateTime::from_timestamp(time as i64, 0)
        .map(|d| format!("Checked {}", d.with_timezone(&chrono::Local).format("%-d %b, %H:%M")))
        .unwrap_or_default()
}

fn show(o: &Rc<Popup>, v: &Value) {
    o.clear_content();
    let list = |k: &str| v[k].as_array().cloned().unwrap_or_default();
    let (dnf, flatpak) = (list("dnf"), list("flatpak"));
    let time = v["time"].as_u64().unwrap_or(0);
    let description = match (dnf.len(), flatpak.len()) {
        (0, 0) if time == 0 => "Checked every 6 hours".to_string(),
        (0, 0) => "Everything is up to date".to_string(),
        (d, 0) => format!("{d} system packages"),
        (0, f) => format!("{f} Flatpak apps and runtimes"),
        (d, f) => format!("{d} system packages, {f} Flatpak apps and runtimes"),
    };
    o.panel.append(&title("Updates", &description));

    if !dnf.is_empty() || !flatpak.is_empty() {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
        for (name, items) in [("System", &dnf), ("Flatpak", &flatpak)] {
            if items.is_empty() {
                continue;
            }
            let h = label(name, &["dim"]);
            h.set_margin_top(4);
            content.append(&h);
            for p in items {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let n = label(p[0].as_str().unwrap_or(""), &[]);
                n.set_hexpand(true);
                n.set_ellipsize(gtk::pango::EllipsizeMode::End);
                row.append(&n);
                row.append(&label(p[1].as_str().unwrap_or(""), &["right"]));
                content.append(&row);
            }
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(320);
        scroll.set_child(Some(&content));
        o.panel.append(&scroll);

        let install = gtk::Button::with_label("Install in a terminal…");
        install.add_css_class("main-action");
        let weak = Rc::downgrade(o);
        install.connect_clicked(move |_| {
            updates::install();
            if let Some(o) = weak.upgrade() {
                o.close();
            }
        });
        o.panel.append(&install);
    }

    let dole = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let k = label(&when(time), &["dim"]);
    k.set_hexpand(true);
    dole.append(&k);
    let again = gtk::Button::with_label(if updates::checking() { "Checking…" } else { "Check now" });
    again.add_css_class("link");
    let weak = Rc::downgrade(o);
    again.connect_clicked(move |b| {
        b.set_label("Checking…");
        b.set_sensitive(false);
        let weak = weak.clone();
        updates::check(move |v| {
            if let Some(o) = weak.upgrade() {
                if o.window.is_visible() {
                    show(&o, &v);
                }
            }
        });
    });
    dole.append(&again);
    o.panel.append(&dole);
    o.recompute();
}
