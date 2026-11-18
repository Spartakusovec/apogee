//! Screenshot and screen recording: what to capture (area, window, screen), delay, sound,
//! editor. Remembers the choices in ~/.config/apogee/settings.json. The capturing itself is in recording.rs.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use serde_json::{json, Value};

use crate::common::*;
use crate::recording;

/// A row of toggle buttons, the selected one is lit.
fn choice(options: &[(&str, &'static str)], selected: &str, on_change: impl Fn(&'static str) + 'static) -> gtk::Box {
    let rada = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    rada.set_homogeneous(true);
    let on_change = Rc::new(on_change);
    for (text, value) in options {
        let b = gtk::Button::with_label(text);
        if *value == selected {
            b.add_css_class("enabled");
        }
        let (on_change, value) = (on_change.clone(), *value);
        b.connect_clicked(move |b| {
            // turn off the other buttons in the row (through the parent, so the buttons don't hold themselves)
            let mut x = b.parent().and_then(|r| r.first_child());
            while let Some(w) = x {
                w.remove_css_class("enabled");
                x = w.next_sibling();
            }
            b.add_css_class("enabled");
            on_change(value);
        });
        rada.append(&b);
    }
    rada
}

fn save(choices: &RefCell<Value>, key: &str, value: Value) {
    choices.borrow_mut()[key] = value;
    save_settings("screenshot", choices.borrow().clone());
}

/// `kind`: "screenshot" or "recording" (by the button that opened the popup).
pub fn fill(o: &Rc<Popup>, kind: &str) {
    // recording right now: only the time and stop
    if let Some(s) = recording::length() {
        o.panel.append(&title("Recording the screen", &format!("{}:{:02} so far · saved to Videos → Recordings", s / 60, s % 60)));
        let stop = gtk::Button::with_label("Stop recording");
        stop.add_css_class("main-action");
        let slaby = Rc::downgrade(o);
        stop.connect_clicked(move |_| {
            recording::stop();
            if let Some(o) = slaby.upgrade() {
                o.close();
            }
        });
        o.panel.append(&stop);
        return;
    }

    let mut v = settings("screenshot");
    if !v.is_object() {
        v = json!({});
    }
    let choices = Rc::new(RefCell::new(v));
    let text = |k: &str, default: &'static str| -> String { choices.borrow()[k].as_str().unwrap_or(default).to_string() };
    let mode = text("mode", "area");
    let sound = text("sound", "off");
    let delay = choices.borrow()["delay"].as_u64().unwrap_or(0);
    let modify = choices.borrow()["modify"].as_bool().unwrap_or(false);
    let recording = Rc::new(std::cell::Cell::new(kind == "recording"));

    o.panel.append(&choice(&[("Screenshot", "screenshot"), ("Recording", "recording")], kind, {
        let recording = recording.clone();
        let o = Rc::downgrade(o);
        move |d| {
            recording.set(d == "recording");
            if let Some(o) = o.upgrade() {
                // switch the visible choices
                let mut dite = o.panel.first_child();
                while let Some(w) = dite {
                    if w.has_css_class("only-screenshot") {
                        w.set_visible(d == "screenshot");
                    }
                    if w.has_css_class("only-recording") {
                        w.set_visible(d == "recording");
                    }
                    if let Ok(b) = w.clone().downcast::<gtk::Button>() {
                        if b.has_css_class("main-action") {
                            b.set_label(if d == "screenshot" { "Take screenshot" } else { "Start recording" });
                        }
                    }
                    dite = w.next_sibling();
                }
                o.recompute();
            }
        }
    }));

    let section = |name: &str, class: Option<&str>, content: gtk::Box| {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
        b.append(&label(name, &["dim"]));
        b.append(&content);
        if let Some(t) = class {
            b.add_css_class(t);
        }
        o.panel.append(&b);
        b
    };
    let vol = choices.clone();
    section("Capture", None, choice(&[("Area", "area"), ("Window", "window"), ("Screen", "screen")], &mode, move |r| save(&vol, "mode", json!(r))));
    let vol = choices.clone();
    let selected_delay = match delay { 3 => "3", 5 => "5", _ => "0" };
    let s = section("Delay", Some("only-screenshot"), choice(&[("None", "0"), ("3 s", "3"), ("5 s", "5")], selected_delay, move |z| {
        save(&vol, "delay", json!(z.parse::<u64>().unwrap_or(0)))
    }));
    s.set_visible(!recording.get());
    let vol = choices.clone();
    let s = section("Sound", Some("only-recording"), choice(&[("Off", "off"), ("System", "system"), ("Microphone", "microphone")], &sound, move |z| {
        save(&vol, "sound", json!(z))
    }));
    s.set_visible(recording.get());

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("only-screenshot");
    let t = label("Open the editor after capture", &[]);
    t.set_hexpand(true);
    row.append(&t);
    let sw = gtk::Switch::new();
    sw.set_active(modify);
    sw.set_valign(gtk::Align::Center);
    let vol = choices.clone();
    sw.connect_active_notify(move |s| save(&vol, "modify", json!(s.is_active())));
    row.append(&sw);
    row.set_visible(!recording.get());
    o.panel.append(&row);

    let on_start_recording = recording.get();
    let action = gtk::Button::with_label(if recording.get() { "Start recording" } else { "Take screenshot" });
    action.add_css_class("main-action");
    action.set_margin_top(4);
    let vol = choices.clone();
    action.connect_clicked(move |_| {
        let v = vol.borrow().clone();
        let mode = v["mode"].as_str().unwrap_or("area").to_string();
        if recording.get() {
            recording::record(&mode, v["sound"].as_str().unwrap_or("off"));
        } else {
            recording::screenshot(&mode, v["delay"].as_u64().unwrap_or(0), v["modify"].as_bool().unwrap_or(false));
        }
    });
    o.panel.append(&action);
    for (text, class, visible) in [
        ("Screenshots are also copied to the clipboard. Print Screen takes an area right away.", "only-screenshot", !on_start_recording),
        ("Recordings are saved to Videos → Recordings. Click the red dot in the panel to stop.", "only-recording", on_start_recording),
    ] {
        let tip = label(text, &["dim"]);
        tip.set_wrap(true);
        tip.set_max_width_chars(40);
        tip.add_css_class(class);
        tip.set_visible(visible);
        o.panel.append(&tip);
    }
}
