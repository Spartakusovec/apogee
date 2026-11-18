//! Keyboard & Touchpad page: keyboard layouts and switching between them, key repeat,
//! Num Lock; touchpad (tap, drag, drag lock, scrolling, speed, clicking, off while
//! typing and with a mouse connected), mouse and a list of gestures. Everything goes to settings.conf (`input type:…`).

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::Value;

use super::sway_conf;
use super::ui::*;
use crate::common::{settings, command, save_settings};

/// layout: (xkb_layout, xkb_variant, name)
const LAYOUTS: &[(&str, &str, &str)] = &[
    ("cz", "", "Czech"),
    ("cz", "qwerty", "Czech (QWERTY)"),
    ("us", "", "English (US)"),
    ("gb", "", "English (UK)"),
    ("sk", "", "Slovak"),
    ("de", "", "German"),
    ("pl", "", "Polish"),
    ("fr", "", "French"),
    ("es", "", "Spanish"),
    ("it", "", "Italian"),
    ("ua", "", "Ukrainian"),
    ("ru", "", "Russian"),
];

const SWITCHING: &[(&str, &str)] = &[("Alt + Shift", "grp:alt_shift_toggle"), ("Ctrl + Shift", "grp:ctrl_shift_toggle"), ("Caps Lock", "grp:caps_toggle")];

fn name(l: &str, v: &str) -> String {
    LAYOUTS.iter().find(|r| r.0 == l && r.1 == v).map(|r| r.2.to_string()).unwrap_or_else(|| if v.is_empty() { l.to_string() } else { format!("{l} ({v})") })
}

fn layouts() -> Vec<(String, String)> {
    match settings("layouts").as_array() {
        Some(a) if !a.is_empty() => a.iter().map(|r| (r[0].as_str().unwrap_or("cz").to_string(), r[1].as_str().unwrap_or("").to_string())).collect(),
        _ => vec![("cz".into(), String::new())],
    }
}

fn save_layouts(r: &[(String, String)]) {
    save_settings("layouts", serde_json::json!(r.iter().map(|(l, v)| [l, v]).collect::<Vec<_>>()));
    let k = "input type:keyboard";
    sway_conf::set(&format!("{k} xkb_layout"), &r.iter().map(|x| x.0.as_str()).collect::<Vec<_>>().join(","));
    sway_conf::set(&format!("{k} xkb_variant"), &format!("\"{}\"", r.iter().map(|x| x.1.as_str()).collect::<Vec<_>>().join(",")));
    if r.len() > 1 && sway_conf::value(&format!("{k} xkb_options")).is_none() {
        sway_conf::set(&format!("{k} xkb_options"), SWITCHING[0].1);
    }
    super::notify("Saved");
    glib::timeout_add_local_once(Duration::from_millis(200), || super::refresh("input"));
}

/// Current libinput settings of a device type (touchpad / pointer), if one is connected.
fn libinput(typ: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(&command("swaymsg", &["-r", "-t", "get_inputs"])).ok()?;
    v.as_array()?.iter().find(|i| i["type"].as_str() == Some(typ)).map(|i| i["libinput"].clone())
}

/// Value: from settings.conf first, then from the device, otherwise the default.
fn value(typ: &str, key: &str, li: &Option<Value>, li_key: &str, default: &str) -> String {
    sway_conf::value(&format!("input type:{typ} {key}"))
        .or_else(|| li.as_ref().and_then(|l| match &l[li_key] {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }))
        .unwrap_or_else(|| default.to_string())
}

fn switcher(card: &gtk::ListBox, typ: &'static str, key: &'static str, name: &str, description: &str, enabled: bool) {
    switch_row(card, name, description, enabled, move |z| {
        sway_conf::set(&format!("input type:{typ} {key}"), if z { "enabled" } else { "disabled" });
    });
}

fn slider(from: f64, to_: f64, step: f64, value: f64, set: impl Fn(f64) + 'static) -> gtk::Scale {
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, from, to_, step);
    s.set_value(value);
    s.set_size_request(220, -1);
    s.set_draw_value(false);
    let timer: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>> = Default::default();
    let n = std::rc::Rc::new(set);
    s.connect_value_changed(move |s| {
        if let Some(id) = timer.take() {
            id.remove();
        }
        let (v, c, n) = (s.value(), timer.clone(), n.clone());
        timer.replace(Some(glib::timeout_add_local_once(Duration::from_millis(250), move || {
            c.replace(None);
            n(v);
        })));
    });
    s
}

fn choice(choices: &[&str], selected: usize, change: impl Fn(usize) + 'static) -> gtk::DropDown {
    let d = gtk::DropDown::from_strings(choices);
    d.set_selected(selected as u32);
    d.connect_selected_notify(move |d| change(d.selected() as usize));
    d
}

pub fn create() -> gtk::Widget {
    let s = page("Keyboard & Touchpad");

    // keyboard
    let card = group(&s, "Keyboard layouts", "The first one is used after you log in.");
    let list = layouts();
    for (i, (l, v)) in list.iter().enumerate() {
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        if i > 0 {
            let list = list.clone();
            buttons.append(&button("Make first", false, move || {
                let mut s = list.clone();
                let x = s.remove(i);
                s.insert(0, x);
                save_layouts(&s);
            }));
        }
        if list.len() > 1 {
            let list = list.clone();
            buttons.append(&button("Remove", false, move || {
                let mut s = list.clone();
                s.remove(i);
                save_layouts(&s);
            }));
        }
        add_row(&card, "\u{f11c}", &name(l, v), if i == 0 { "Default" } else { "" }, Some(buttons.upcast_ref()));
    }
    let free: Vec<&(&str, &str, &str)> = LAYOUTS.iter().filter(|r| !list.iter().any(|(l, v)| l == r.0 && v == r.1)).collect();
    let mut names: Vec<&str> = vec!["Choose…"];
    names.extend(free.iter().map(|r| r.2));
    let list2 = list.clone();
    let add = choice(&names, 0, move |i| {
        if i == 0 {
            return;
        }
        let r = free[i - 1];
        let mut s = list2.clone();
        s.push((r.0.to_string(), r.1.to_string()));
        save_layouts(&s);
    });
    add_row(&card, "\u{f067}", "Add a layout", "", Some(add.upcast_ref()));
    if list.len() > 1 {
        let now = sway_conf::value("input type:keyboard xkb_options").unwrap_or_default();
        let d = choice(&SWITCHING.iter().map(|p| p.0).collect::<Vec<_>>(), SWITCHING.iter().position(|p| p.1 == now).unwrap_or(0), |i| {
            sway_conf::set("input type:keyboard xkb_options", SWITCHING[i].1);
            super::notify("Saved");
        });
        add_row(&card, "\u{f362}", "Switch layouts with", "The panel shows the current layout", Some(d.upcast_ref()));
    }

    let card = group(&s, "Typing", "");
    let delay: f64 = value("keyboard", "repeat_delay", &None, "", "600").parse().unwrap_or(600.0);
    let speed: f64 = value("keyboard", "repeat_rate", &None, "", "25").parse().unwrap_or(25.0);
    add_row(&card, "\u{f017}", "Repeat delay", "How long to hold a key before it repeats", Some(slider(150.0, 1000.0, 10.0, delay, |v| {
        sway_conf::set("input type:keyboard repeat_delay", &format!("{v:.0}"));
    }).upcast_ref()));
    add_row(&card, "\u{f04e}", "Repeat speed", "", Some(slider(5.0, 60.0, 1.0, speed, |v| {
        sway_conf::set("input type:keyboard repeat_rate", &format!("{v:.0}"));
    }).upcast_ref()));
    let numlock = value("keyboard", "xkb_numlock", &None, "", "enabled") == "enabled";
    switcher(&card, "keyboard", "xkb_numlock", "Num Lock on after login", "", numlock);
    let field = gtk::Entry::new();
    field.set_placeholder_text(Some("Try typing here"));
    field.set_width_chars(22);
    add_row(&card, "", "Test", "", Some(field.upcast_ref()));

    // touchpad
    let li = libinput("touchpad");
    let yes = |key: &str, li_key: &str, default: &str| value("touchpad", key, &li, li_key, default) != "disabled";
    let card = group(&s, "Touchpad", "");
    let speed: f64 = value("touchpad", "pointer_accel", &li, "accel_speed", "0").parse().unwrap_or(0.0);
    add_row(&card, "\u{f245}", "Pointer speed", "", Some(slider(-1.0, 1.0, 0.05, speed, |v| {
        sway_conf::set("input type:touchpad pointer_accel", &format!("{v:.2}"));
    }).upcast_ref()));
    switcher(&card, "touchpad", "tap", "Tap to click", "One finger = left click, two = right click, three = middle click", yes("tap", "tap", "enabled"));
    switcher(&card, "touchpad", "drag", "Tap and drag", "Tap, then tap again and slide to drag", yes("drag", "tap_drag", "enabled"));
    let lock = value("touchpad", "drag_lock", &li, "tap_drag_lock", "disabled");
    let choices = [("Off", "disabled"), ("Short pause", "enabled"), ("Until you tap again", "enabled_sticky")];
    let d = choice(&choices.iter().map(|v| v.0).collect::<Vec<_>>(), choices.iter().position(|v| v.1 == lock).unwrap_or(0), move |i| {
        sway_conf::set("input type:touchpad drag_lock", choices[i].1);
    });
    add_row(&card, "", "Drag lock", "Lifting the finger while dragging doesn't drop the item", Some(d.upcast_ref()));
    switcher(&card, "touchpad", "natural_scroll", "Natural scrolling", "Content moves with your fingers, like on a phone", yes("natural_scroll", "natural_scroll", "enabled"));
    let offset = value("touchpad", "scroll_method", &li, "scroll_method", "two_finger");
    let choices = [("Two fingers", "two_finger"), ("Right edge", "edge")];
    let d = choice(&choices.iter().map(|v| v.0).collect::<Vec<_>>(), choices.iter().position(|v| v.1 == offset).unwrap_or(0), move |i| {
        sway_conf::set("input type:touchpad scroll_method", choices[i].1);
    });
    add_row(&card, "", "Scroll with", "", Some(d.upcast_ref()));
    let click = value("touchpad", "click_method", &li, "click_method", "button_areas");
    let choices = [("Bottom corners", "button_areas"), ("Number of fingers", "clickfinger")];
    let d = choice(&choices.iter().map(|v| v.0).collect::<Vec<_>>(), choices.iter().position(|v| v.1 == click).unwrap_or(0), move |i| {
        sway_conf::set("input type:touchpad click_method", choices[i].1);
    });
    add_row(&card, "", "Right click when pressing", "Bottom right corner, or press with two fingers", Some(d.upcast_ref()));
    switcher(&card, "touchpad", "dwt", "Off while typing", "Stops the palm from moving the pointer", yes("dwt", "dwt", "enabled"));
    let events = value("touchpad", "events", &li, "send_events", "enabled");
    switch_row(&card, "Off when a mouse is connected", "", events == "disabled_on_external_mouse", |z| {
        sway_conf::set("input type:touchpad events", if z { "disabled_on_external_mouse" } else { "enabled" });
    });

    // mouse
    let li = libinput("pointer");
    let card = group(&s, "Mouse", if li.is_some() { "" } else { "No mouse connected right now; these apply when one is." });
    let speed: f64 = value("pointer", "pointer_accel", &li, "accel_speed", "0").parse().unwrap_or(0.0);
    add_row(&card, "\u{f8cc}", "Pointer speed", "", Some(slider(-1.0, 1.0, 0.05, speed, |v| {
        sway_conf::set("input type:pointer pointer_accel", &format!("{v:.2}"));
    }).upcast_ref()));
    let profile = value("pointer", "accel_profile", &li, "accel_profile", "adaptive");
    switch_row(&card, "Mouse acceleration", "Fast movements move the pointer further. Off is better for games.", profile != "flat", |z| {
        sway_conf::set("input type:pointer accel_profile", if z { "adaptive" } else { "flat" });
    });
    switcher(&card, "pointer", "natural_scroll", "Natural scrolling", "", value("pointer", "natural_scroll", &li, "natural_scroll", "disabled") == "enabled");
    switcher(&card, "pointer", "left_handed", "Left-handed", "Swaps the left and right buttons", value("pointer", "left_handed", &li, "left_handed", "disabled") == "enabled");

    // gesta
    let card = group(&s, "Gestures", "");
    for (gesture, co) in [
        ("4 fingers left / right", "Next / previous workspace"),
        ("3 fingers up", "Show or hide the dock"),
        ("3-finger tap", "Middle click (on the volume icon: open the sound mixer)"),
        ("2 fingers up / down", "Scroll"),
    ] {
        add_row(&card, "\u{f256}", gesture, co, None);
    }
    s.upcast()
}
