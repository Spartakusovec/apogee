//! Date & Time page: automatic time, time zone, clock format (link to Panel & Dock)
//! and formats of dates, numbers and units (localectl, takes effect after logging in again).
//! System changes go through timedatectl / localectl, which ask for the password (polkit).

use std::collections::HashMap;

use gtk::prelude::*;

use super::ui::*;
use crate::common::{in_background, command};

/// formats: (name, locale)
const FORMATS: &[(&str, &str)] = &[
    ("Czech – 18. 11. 2026, 1 234,5", "cs_CZ.UTF-8"),
    ("English (US) – 11/18/2026, 1,234.5", "en_US.UTF-8"),
    ("English (UK) – 18/11/2026, 1,234.5", "en_GB.UTF-8"),
    ("Slovak – 18. 11. 2026, 1 234,5", "sk_SK.UTF-8"),
];
const CATEGORIES: &[&str] = &["LC_TIME", "LC_NUMERIC", "LC_MONETARY", "LC_MEASUREMENT", "LC_PAPER"];

fn timedatectl() -> HashMap<String, String> {
    command("timedatectl", &["show"]).lines().filter_map(|r| r.split_once('=').map(|(a, b)| (a.to_string(), b.to_string()))).collect()
}

/// A system command in the background (polkit asks for the password), then a toast and a rebuild.
fn start(program: &'static str, args: Vec<String>, done: &'static str) {
    super::notify("Waiting for your password…");
    in_background(
        move || std::process::Command::new(program).args(&args).output().map(|v| v.status.success()).unwrap_or(false),
        move |ok| {
            super::notify(if ok { done } else { "Not changed" });
            super::refresh("time");
        },
    );
}

pub fn create() -> gtk::Widget {
    let s = page("Date & Time");
    let t = timedatectl();

    let card = group(&s, "", "");
    let now = chrono::Local::now();
    add_row(&card, "\u{f017}", &now.format("%H:%M").to_string(), &now.format("%A %e %B %Y").to_string(), None);
    switch_row(&card, "Set the time automatically", "From the internet", t.get("NTP").is_some_and(|v| v == "yes"), |z| {
        start("timedatectl", vec!["set-ntp".into(), z.to_string()], "Saved");
    });
    let zones: Vec<String> = command("timedatectl", &["list-timezones"]).lines().map(String::from).collect();
    let pasmo = t.get("Timezone").cloned().unwrap_or_default();
    let d = gtk::DropDown::from_strings(&zones.iter().map(|p| p.replace('_', " ")).collect::<Vec<_>>().iter().map(String::as_str).collect::<Vec<_>>());
    d.set_enable_search(true);
    d.set_expression(Some(gtk::PropertyExpression::new(gtk::StringObject::static_type(), None::<gtk::Expression>, "string")));
    d.set_selected(zones.iter().position(|p| *p == pasmo).map(|p| p as u32).unwrap_or(gtk::INVALID_LIST_POSITION));
    d.connect_selected_notify(move |d| {
        if let Some(p) = zones.get(d.selected() as usize) {
            if *p != pasmo {
                start("timedatectl", vec!["set-timezone".into(), p.clone()], "Time zone changed");
            }
        }
    });
    add_row(&card, "\u{f57d}", "Time zone", "", Some(d.upcast_ref()));
    action_row(&card, "\u{f2f2}", "Clock in the panel", "Date, seconds, 12-hour time", "\u{f054}", || super::open("panel_dock"));

    // location: weather and night light (sunrise and sunset)
    let card = group(&s, "Location", "For the weather and for night light at sunset. Search for your town.");
    let now = crate::popups::weather::location().map(|p| p.0).unwrap_or_else(|| "Not set".into());
    add_row(&card, "\u{f3c5}", &now, "", None);
    let search = gtk::Entry::new();
    search.set_placeholder_text(Some("Town or city"));
    search.set_width_chars(22);
    let results = card.clone();
    let find = {
        let search = search.clone();
        move || {
            let query = search.text().trim().to_string();
            if query.is_empty() {
                return;
            }
            super::notify("Searching…");
            let card = results.clone();
            in_background(
                move || {
                    let url = format!(
                        "https://geocoding-api.open-meteo.com/v1/search?count=6&language=en&format=json&name={}",
                        query.replace(' ', "+")
                    );
                    serde_json::from_str::<serde_json::Value>(&command("curl", &["-sf", "--max-time", "10", &url]))
                        .ok()
                        .and_then(|v| v["results"].as_array().cloned())
                        .unwrap_or_default()
                },
                move |mista| {
                    // remove the old results (the rows after the search row)
                    while let Some(r) = card.row_at_index(2) {
                        card.remove(&r);
                    }
                    if mista.is_empty() {
                        super::notify("Nothing found – check the spelling or the internet connection");
                    }
                    for m in mista {
                        let name = m["name"].as_str().unwrap_or("").to_string();
                        let place = [m["admin1"].as_str(), m["country"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(", ");
                        let (width, length) = (m["latitude"].as_f64().unwrap_or(0.0), m["longitude"].as_f64().unwrap_or(0.0));
                        let n = name.clone();
                        action_row(&card, "", &name, &place, "Use", move || {
                            crate::common::save_settings("location", serde_json::json!({"name": n, "lat": width, "lon": length}));
                            crate::popups::weather::change_location();
                            // night light reads the location when it starts again
                            crate::popups::brightness::save("location", &format!("{width:.3},{length:.3}"));
                            super::notify(&format!("Location: {n}"));
                            super::refresh("time");
                        });
                    }
                },
            );
        }
    };
    let n2 = find.clone();
    search.connect_activate(move |_| n2());
    let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    wrapper.append(&search);
    wrapper.append(&button("Search", false, find));
    add_row(&card, "\u{f002}", "Change", "", Some(wrapper.upcast_ref()));

    // formats
    let card = group(&s, "Formats", "Dates, numbers, money and units in apps. The menus stay in English. Applies after you log in again.");
    let locale = command("localectl", &["status"]);
    let now = locale.split_whitespace().find_map(|w| w.strip_prefix("LC_TIME=")).or_else(|| locale.split_whitespace().find_map(|w| w.strip_prefix("LANG="))).unwrap_or("en_US.UTF-8").to_string();
    let d = gtk::DropDown::from_strings(&FORMATS.iter().map(|f| f.0).collect::<Vec<_>>());
    d.set_selected(FORMATS.iter().position(|f| f.1.eq_ignore_ascii_case(&now) || f.1.replace("UTF-8", "utf8") == now).unwrap_or(1) as u32);
    d.connect_selected_notify(move |d| {
        let l = FORMATS[d.selected() as usize].1;
        let mut a = vec!["set-locale".to_string(), "LANG=en_US.UTF-8".to_string()];
        a.extend(CATEGORIES.iter().map(|k| format!("{k}={l}")));
        start("localectl", a, "Saved – log out and back in to see it");
    });
    add_row(&card, "\u{f1ab}", "Region", "", Some(d.upcast_ref()));
    if t.get("LocalRTC").is_some_and(|v| v == "yes") {
        add_row(&card, "\u{f05a}", "Hardware clock keeps local time", "Usual when Windows is installed on the same computer", None);
    }
    s.upcast()
}
