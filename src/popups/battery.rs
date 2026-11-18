//! Battery: popup (charge, time left, power draw, battery health and power mode through
//! power-mode.sh / tuned) and background watching.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;

use crate::common::*;

/// Battery watching (checked once a minute):
/// - warns at 15 % and 5 %, once per discharge
/// - at 15 % turns on power saver and restores the previous mode when the charger is plugged in
/// Milder saving on battery is done by tuned itself (balanced → balanced-battery).
pub fn watch() {
    let notified = Cell::new(100);
    let before: RefCell<Option<String>> = RefCell::new(None); // the mode before the automatic power saver
    let power = scripts().join("power-mode.sh").to_string_lossy().to_string();
    gtk::glib::timeout_add_seconds_local(60, move || {
        let Some(b) = battery() else { return gtk::glib::ControlFlow::Continue };
        let chapter = b.percent.round() as i32;
        if b.state != 2 {
            notified.set(100);
            if let Some(mode) = before.take() {
                background(&power, &[&mode]);
            }
        } else if chapter <= 5 && notified.get() > 5 {
            background("notify-send", &["-u", "critical", "-i", "battery-empty", &format!("Battery almost empty: {chapter} %"), "Plug in the charger, the laptop will shut down soon."]);
            notified.set(5);
        } else if chapter <= 15 && notified.get() > 15 {
            background("notify-send", &["-i", "battery-low", &format!("Low battery: {chapter} %"), "Power saver is on until you plug in the charger."]);
            notified.set(15);
            let now = mode_power();
            if now != "power-saver" {
                before.replace(Some(now));
                background(&power, &["power-saver"]);
            }
        }
        gtk::glib::ControlFlow::Continue
    });
}

pub fn fill(o: &Rc<Popup>) {
    let b = battery().unwrap_or(Battery { percent: 0.0, state: 0, to_empty: 0, to_full: 0, power_draw: 0.0, health: 0.0 });
    let percent = b.percent.round();

    let main = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    main.append(&label(&format!("{percent} %"), &["large"]));
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texts.set_valign(gtk::Align::Center);
    texts.set_hexpand(true);
    let description = match b.state {
        1 => "Charging",
        2 | 6 => "On battery",
        4 => "Fully charged",
        5 => "Plugged in, not charging",
        _ => "",
    };
    texts.append(&label(description, &[]));
    let (remaining, prefix) = if b.state == 2 { (b.to_empty, "Until empty: ") } else { (b.to_full, "Until full: ") };
    if remaining > 0 {
        let duration = if remaining >= 3600 { format!("{} h {} min", remaining / 3600, remaining / 60 % 60) } else { format!("{} min", remaining / 60) };
        texts.append(&label(&format!("{prefix}{duration}"), &["dim"]));
    }
    main.append(&texts);
    o.panel.append(&main);

    let level = gtk::LevelBar::for_interval(0.0, 100.0);
    level.set_mode(gtk::LevelBarMode::Continuous);
    for name in ["low", "high", "full"] {
        level.remove_offset_value(Some(name));
    }
    level.set_value(percent);
    if percent <= 15.0 {
        level.add_css_class("low");
    }
    o.panel.append(&level);

    let mut details = Vec::new();
    if b.power_draw > 0.0 {
        details.push(format!("Power draw {:.1} W", b.power_draw));
    }
    if b.health > 0.0 {
        details.push(format!("Battery health {:.0} %", b.health));
    }
    if !details.is_empty() {
        o.panel.append(&label(&details.join("   ·   "), &["dim"]));
    }

    o.panel.append(&separator());
    o.panel.append(&label("Power mode", &["dim"]));
    let power = scripts().join("power-mode.sh").to_string_lossy().to_string();
    let current = mode_power();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_homogeneous(true);
    let buttons: Rc<Vec<(&'static str, gtk::Button)>> = Rc::new(
        [("power-saver", "\u{f06c}  Saver"), ("balanced", "\u{f24e}  Balanced"), ("performance", "\u{f0e7}  Performance")]
            .iter()
            .map(|(r, t)| (*r, gtk::Button::with_label(t)))
            .collect(),
    );
    let mark = {
        let t = buttons.clone();
        move |active: &str| {
            for (r, b) in t.iter() {
                if *r == active {
                    b.add_css_class("enabled");
                } else {
                    b.remove_css_class("enabled");
                }
            }
        }
    };
    for (mode, b) in buttons.iter() {
        let (mode, mark, power) = (*mode, mark.clone(), power.clone());
        b.connect_clicked(move |_| {
            mark(mode);
            background(&power, &[mode]);
        });
        row.append(b);
    }
    mark(&current);
    o.panel.append(&row);
    if b.state == 2 && percent > 15.0 {
        o.panel.append(&label("Saver turns on by itself at 15 %", &["dim"]));
    }
    o.panel.append(&separator());
    o.panel.append(&link_settings(o, "Power settings…", "power"));
}
