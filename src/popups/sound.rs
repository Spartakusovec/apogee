//! Sound: output volume and mute, microphone, output choice (speakers, headphones, AirPlay…).

use std::rc::Rc;

use gtk::prelude::*;

use crate::common::*;

const REPRO: &str = "\u{f028}";
const MUTED: &str = "\u{f6a9}";
const MICROPHONE: &str = "\u{f130}";
const MICROPHONE_OFF: &str = "\u{f131}";

/// Volume and mute of a device from `pactl -f json list sinks/sources` (same numbers as wpctl)
fn volume_of(list: &[serde_json::Value], name: &str) -> (f64, bool) {
    let Some(z) = list.iter().find(|z| z["name"].as_str() == Some(name)) else { return (0.0, false) };
    let percent = z["volume"]
        .as_object()
        .and_then(|k| k.values().next())
        .and_then(|k| k["value_percent"].as_str())
        .and_then(|p| p.trim_end_matches('%').parse::<f64>().ok())
        .unwrap_or(0.0);
    (percent, z["mute"].as_bool().unwrap_or(false))
}

pub fn name_output(name: &str, description: &str) -> String {
    if let Some(rest) = name.strip_prefix("raop_sink.") {
        // AirPlay: raop_sink.Jana--MacBook-Air.local.… → Jana's MacBook Air (AirPlay)
        let host = rest.split(".local").next().unwrap_or(rest);
        return format!("{} (AirPlay)", host.replace("--", "'s ").replace('-', " "));
    }
    if !description.is_empty() && description != "(null)" {
        if description.starts_with("Built-in Audio") {
            return "Built-in speakers / headphones".into();
        }
        return description.into();
    }
    name.into()
}

/// A mute button that switches its icon and look.
fn button_mute(device: &'static str, muted: bool, enabled: &'static str, off: &'static str) -> gtk::Button {
    let b = gtk::Button::with_label(if muted { off } else { enabled });
    b.add_css_class(if muted { "enabled" } else { "row" });
    b.add_css_class("quick-icon");
    b.connect_clicked(move |b| {
        background("wpctl", &["set-mute", device, "toggle"]);
        let now_muted = !b.has_css_class("enabled");
        b.set_label(if now_muted { off } else { enabled });
        if now_muted {
            b.remove_css_class("row");
            b.add_css_class("enabled");
        } else {
            b.remove_css_class("enabled");
            b.add_css_class("row");
        }
    });
    b
}

pub fn fill(o: &Rc<Popup>) {
    // all queries at once, so the popup doesn't wait for each (pactl is faster than wpctl)
    let v = current(&[
        ("pactl", &["-f", "json", "list", "sinks"]),
        ("pactl", &["-f", "json", "list", "sources"]),
        ("pactl", &["-f", "json", "info"]),
    ]);
    let json = |t: &str| serde_json::from_str::<serde_json::Value>(t).ok();
    let list = |t: &str| json(t).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let (sinks, sources, info) = (list(&v[0]), list(&v[1]), json(&v[2]).unwrap_or_default());
    let default = info["default_sink_name"].as_str().unwrap_or("").to_string();
    let (volume, muted) = volume_of(&sinks, &default);
    let outputs: Vec<(String, String)> = sinks
        .iter()
        .map(|v| (v["name"].as_str().unwrap_or("").to_string(), v["description"].as_str().unwrap_or("").to_string()))
        .collect();
    let description = outputs.iter().find(|(n, _)| *n == default).map(|(n, p)| name_output(n, p)).unwrap_or_default();
    o.panel.append(&title("Sound", &description));

    let mute = button_mute("@DEFAULT_AUDIO_SINK@", muted, REPRO, MUTED);
    let (row_volume, _) = slider(&mute, volume, 0.0, 100.0, percent, |v| {
        background("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", v / 100.0)]);
    });
    o.panel.append(&row_volume);

    // mikrofon
    let (microphone, mic_muted) = volume_of(&sources, info["default_source_name"].as_str().unwrap_or(""));
    let mic = button_mute("@DEFAULT_AUDIO_SOURCE@", mic_muted, MICROPHONE, MICROPHONE_OFF);
    let (row_mic, _) = slider(&mic, microphone, 0.0, 100.0, percent, |v| {
        background("wpctl", &["set-volume", "@DEFAULT_AUDIO_SOURCE@", &format!("{:.2}", v / 100.0)]);
    });
    // who is recording right now (that's why the microphone icon in the bar is lit)
    let recording = crate::microphone::who_recording();
    let title_mic = if recording.is_empty() { "Microphone".to_string() } else { format!("Microphone · in use by {}", recording.join(", ")) };
    o.panel.append(&label(&title_mic, &["dim"]));
    o.panel.append(&row_mic);

    if outputs.len() > 1 {
        o.panel.append(&separator());
        o.panel.append(&label("Output", &["dim"]));
        for (name, description) in &outputs {
            let n = name.clone();
            let slaby = Rc::downgrade(o);
            o.panel.append(&row(
                if *name == default { "\u{f00c}" } else { "" },
                &name_output(name, description),
                "",
                *name == default,
                move || {
                    background("pactl", &["set-default-sink", &n]);
                    if let Some(o) = slaby.upgrade() {
                        o.close();
                    }
                },
            ));
        }
    }
    o.panel.append(&separator());
    o.panel.append(&link_settings(o, "Sound settings…", "sound"));
}
