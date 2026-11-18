//! Sound page: output and input (device choice, volume, mute) and the volume of the apps
//! that are playing right now. Replaces pavucontrol. Data from `pactl -f json`, set through wpctl / pactl.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::Value;

use super::ui::*;
use crate::popups::sound::name_output;
use crate::common::{in_background, background, command};

struct Data {
    sinks: Vec<Value>,
    sources: Vec<Value>,
    apps: Vec<Value>,
    default_output: String,
    default_input: String,
}

fn load() -> Data {
    let json = |a: &[&str]| serde_json::from_str::<Value>(&command("pactl", a)).unwrap_or_default();
    let field = |v: Value| v.as_array().cloned().unwrap_or_default();
    let info = json(&["-f", "json", "info"]);
    Data {
        sinks: field(json(&["-f", "json", "list", "sinks"])),
        // output monitors aren't microphones
        sources: field(json(&["-f", "json", "list", "sources"])).into_iter().filter(|s| s["properties"]["device.class"].as_str() != Some("monitor")).collect(),
        apps: field(json(&["-f", "json", "list", "sink-inputs"])),
        default_output: info["default_sink_name"].as_str().unwrap_or("").into(),
        default_input: info["default_source_name"].as_str().unwrap_or("").into(),
    }
}

fn volume_of(z: &Value) -> f64 {
    z["volume"]
        .as_object()
        .and_then(|k| k.values().next())
        .and_then(|k| k["value_percent"].as_str())
        .and_then(|p| p.trim_end_matches('%').parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Volume slider 0–100 % with a number; changes are sent at most ~15 times a second.
fn slider(value: f64, set: impl Fn(f64) + 'static) -> gtk::Box {
    let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    s.set_value(value.min(100.0));
    s.set_size_request(220, -1);
    s.set_draw_value(false);
    let number = gtk::Label::new(Some(&format!("{value:.0} %")));
    number.add_css_class("description");
    number.set_width_chars(5);
    let pending: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    let timer: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let set = Rc::new(set);
    let c = number.clone();
    s.connect_value_changed(move |s| {
        let v = s.value().round();
        c.set_text(&format!("{v:.0} %"));
        pending.set(Some(v));
        if timer.borrow().is_none() {
            let (pending, time, nast) = (pending.clone(), timer.clone(), set.clone());
            let id = glib::timeout_add_local_once(Duration::from_millis(60), move || {
                time.replace(None);
                if let Some(v) = pending.take() {
                    nast(v);
                }
            });
            timer.replace(Some(id));
        }
    });
    wrapper.append(&s);
    wrapper.append(&number);
    wrapper
}

pub fn create() -> gtk::Widget {
    let s = page("Sound");
    let output = group(&s, "Output", "");
    let input = group(&s, "Input", "");
    let apps = group(&s, "Apps", "Volume of each app that is playing sound right now.");
    loading(&output, "Loading…");
    let cards = (output, input, apps);
    in_background(load, move |d| {
        if cards.0.root().is_none() {
            return;
        }
        show(&cards, &d);
    });
    s.upcast()
}

fn show((output, input, apps): &(gtk::ListBox, gtk::ListBox, gtk::ListBox), d: &Data) {
    // output: volume of the default one, mute, device choice
    clear(output);
    if let Some(z) = d.sinks.iter().find(|z| z["name"].as_str() == Some(d.default_output.as_str())) {
        add_row(output, "\u{f028}", "Volume", "", Some(slider(volume_of(z), |v| {
            background("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", v / 100.0)]);
        }).upcast_ref()));
        switch_row(output, "Mute", "", z["mute"].as_bool().unwrap_or(false), |m| {
            background("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", if m { "1" } else { "0" }]);
        });
    }
    if d.sinks.is_empty() {
        add_row(output, "", "No speakers or headphones found", "", None);
    }
    for z in &d.sinks {
        let name = z["name"].as_str().unwrap_or("").to_string();
        let description = name_output(&name, z["description"].as_str().unwrap_or(""));
        let selected = name == d.default_output;
        let p = description.clone();
        action_row(output, if selected { "\u{f00c}" } else { "" }, &description, if selected { "Playing here" } else { "" }, "", move || {
            background("pactl", &["set-default-sink", &name]);
            super::notify(&format!("Sound now plays on {p}"));
            glib::timeout_add_local_once(Duration::from_millis(400), || super::refresh("sound"));
        });
    }

    // vstup: mikrofon
    clear(input);
    if let Some(z) = d.sources.iter().find(|z| z["name"].as_str() == Some(d.default_input.as_str())) {
        let recording = crate::microphone::who_recording();
        let description = if recording.is_empty() { String::new() } else { format!("In use by {}", recording.join(", ")) };
        add_row(input, "\u{f130}", "Microphone volume", &description, Some(slider(volume_of(z), |v| {
            background("wpctl", &["set-volume", "@DEFAULT_AUDIO_SOURCE@", &format!("{:.2}", v / 100.0)]);
        }).upcast_ref()));
        switch_row(input, "Mute microphone", "", z["mute"].as_bool().unwrap_or(false), |m| {
            background("wpctl", &["set-mute", "@DEFAULT_AUDIO_SOURCE@", if m { "1" } else { "0" }]);
        });
    }
    if d.sources.len() > 1 {
        for z in &d.sources {
            let name = z["name"].as_str().unwrap_or("").to_string();
            let description = z["description"].as_str().unwrap_or(&name).replace("Built-in Audio Analog Stereo", "Built-in microphone");
            let selected = name == d.default_input;
            action_row(input, if selected { "\u{f00c}" } else { "" }, &description, if selected { "In use" } else { "" }, "", move || {
                background("pactl", &["set-default-source", &name]);
                glib::timeout_add_local_once(Duration::from_millis(400), || super::refresh("sound"));
            });
        }
    } else if d.sources.is_empty() {
        add_row(input, "", "No microphone found", "", None);
    }

    // aplikace
    clear(apps);
    if d.apps.is_empty() {
        add_row(apps, "", "No app is playing sound", "", None);
    }
    for a in &d.apps {
        let index = a["index"].as_i64().unwrap_or(-1).to_string();
        let name = a["properties"]["application.name"].as_str().unwrap_or("App").to_string();
        let co = a["properties"]["media.name"].as_str().unwrap_or("").to_string();
        let i = index.clone();
        let row = add_row(apps, "\u{f001}", &name, &co, Some(slider(volume_of(a), move |v| {
            background("pactl", &["set-sink-input-volume", &i, &format!("{v:.0}%")]);
        }).upcast_ref()));
        let _ = row;
    }
}
