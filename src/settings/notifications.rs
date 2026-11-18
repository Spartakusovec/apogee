//! Notifications page: Do Not Disturb (by hand and on a schedule), history and muting single apps
//! (their notifications only go to the history, without a bubble).

use gtk::prelude::*;
use serde_json::json;

use super::ui::*;
use crate::common::{settings, save_settings};

fn time(minutes: u64) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// A time picker in half-hour steps.
fn choice_time(minutes: u64, change: impl Fn(u64) + 'static) -> gtk::DropDown {
    let times: Vec<String> = (0..48).map(|i| time(i * 30)).collect();
    let d = gtk::DropDown::from_strings(&times.iter().map(String::as_str).collect::<Vec<_>>());
    d.set_selected((minutes / 30) as u32);
    d.connect_selected_notify(move |d| change(d.selected() as u64 * 30));
    d
}

fn save_time(key: &str, value: serde_json::Value) {
    let mut c = settings("dnd_schedule");
    if !c.is_object() {
        c = json!({"enabled": false, "from": 22 * 60, "to": 7 * 60});
    }
    c[key] = value;
    save_settings("dnd_schedule", c);
}

pub fn create() -> gtk::Widget {
    let s = page("Notifications");

    let card = group(&s, "", "");
    switch_row(&card, "To Not Disturb", "Notifications go quietly to the list (Super + N); urgent ones still show", crate::notifier::dnd(), |z| {
        crate::popups::command("dnd", if z { "on" } else { "off" });
    });
    let c = settings("dnd_schedule");
    let enabled = c["enabled"].as_bool().unwrap_or(false);
    switch_row(&card, "Turn on Do Not Disturb at night", "", enabled, |z| {
        save_time("enabled", json!(z));
        super::notify("Saved");
        super::refresh("notifications");
    });
    if enabled {
        add_row(&card, "\u{f186}", "From", "", Some(choice_time(c["from"].as_u64().unwrap_or(22 * 60), |m| save_time("from", json!(m))).upcast_ref()));
        add_row(&card, "\u{f185}", "Until", "", Some(choice_time(c["to"].as_u64().unwrap_or(7 * 60), |m| save_time("to", json!(m))).upcast_ref()));
    }

    let card = group(&s, "Apps", "Turn an app off to keep its notifications in the list without popping up.");
    let apps = crate::notifier::apps_v_history();
    let mut all = apps.clone();
    for a in settings("muted_apps").as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
        if !all.iter().any(|x| x == a) {
            all.push(a.to_string());
        }
    }
    if all.is_empty() {
        add_row(&card, "", "No app has sent a notification yet", "", None);
    }
    for a in all {
        let muted = crate::notifier::muted(&a);
        let name = a.clone();
        switch_row(&card, &a, "", !muted, move |z| {
            let mut v: Vec<String> = settings("muted_apps").as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            v.retain(|x| *x != name);
            if !z {
                v.push(name.clone());
            }
            save_settings("muted_apps", json!(v));
        });
    }

    let card = group(&s, "", "");
    let count = crate::notifier::history().len();
    let clear = button("Clear", false, || {
        crate::notifier::clear_vse();
        super::notify("Notifications cleared");
        super::refresh("notifications");
    });
    clear.set_sensitive(count > 0);
    add_row(&card, "\u{f0f3}", "Notification list", &format!("{count} saved"), Some(clear.upcast_ref()));
    s.upcast()
}
