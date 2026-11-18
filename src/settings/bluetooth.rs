//! Bluetooth page: on/off, paired devices (connect, disconnect, forget, device
//! battery) and finding and pairing new ones. Everything through bluetoothctl in the background.

use std::rc::Rc;

use gtk::prelude::*;

use super::ui::*;
use crate::common::{bluetooth_enabled, in_background, command};

#[derive(Clone)]
struct Device {
    mac: String,
    name: String,
    connected: bool,
    battery: String,
    icon: &'static str,
    kind: &'static str,
}

fn info(mac: &str, name: &str) -> Device {
    let info = command("bluetoothctl", &["info", mac]);
    let value = |key: &str| info.lines().find_map(|r| r.trim().strip_prefix(key).map(|v| v.trim().to_string()));
    // "Battery Percentage: 0x50 (80)" → 80
    let battery = value("Battery Percentage:").and_then(|v| v.split('(').nth(1).map(|x| x.trim_end_matches(')').to_string())).unwrap_or_default();
    let (icon, kind) = match value("Icon:").as_deref() {
        Some("audio-headset") | Some("audio-headphones") => ("\u{f025}", "Headphones"),
        Some("audio-card") => ("\u{f028}", "Speaker"),
        Some("input-keyboard") => ("\u{f11c}", "Keyboard"),
        Some("input-mouse") => ("\u{f8cc}", "Mouse"),
        Some("input-gaming") => ("\u{f11b}", "Game controller"),
        Some("phone") => ("\u{f3cd}", "Phone"),
        Some("computer") => ("\u{f109}", "Computer"),
        _ => ("\u{f293}", "Device"),
    };
    Device { mac: mac.into(), name: name.into(), connected: info.contains("Connected: yes"), battery, icon, kind }
}

/// A device from the bluetoothctl list ("Device AA:BB:… Name").
fn list(args: &[&str]) -> Vec<(String, String)> {
    command("bluetoothctl", args)
        .lines()
        .filter_map(|r| {
            let c: Vec<&str> = r.splitn(3, ' ').collect();
            (c.len() == 3 && c[0] == "Device").then(|| (c[1].to_string(), c[2].to_string()))
        })
        .collect()
}

fn paired() -> Vec<Device> {
    let mut v: Vec<Device> = list(&["devices", "Paired"]).iter().map(|(m, j)| info(m, j)).collect();
    v.sort_by_key(|z| (!z.connected, z.name.to_lowercase()));
    v
}

/// bluetoothctl in the background, success judged by its output; then a toast and a page rebuild.
fn bt(args: Vec<String>, done: String, error: String) {
    in_background(
        move || {
            let mut a = vec!["30".to_string(), "bluetoothctl".to_string()];
            a.extend(args);
            let a: Vec<&str> = a.iter().map(String::as_str).collect();
            let v = command("timeout", &a).to_lowercase();
            v.contains("successful") || v.contains("succeeded")
        },
        move |ok| {
            super::notify(if ok { &done } else { &error });
            super::refresh("bluetooth");
        },
    );
}

pub fn create() -> gtk::Widget {
    let s = page("Bluetooth");
    let enabled = bluetooth_enabled();
    let main = group(&s, "", "");
    switch_row(&main, "Bluetooth", "Headphones, speakers, mice and keyboards", enabled, |state| {
        crate::common::background("bluetoothctl", &["power", if state { "on" } else { "off" }]);
        super::notify(if state { "Bluetooth on" } else { "Bluetooth off" });
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(800), || super::refresh("bluetooth"));
    });
    if !enabled {
        return s.upcast();
    }
    let mine = group(&s, "My devices", "");
    loading(&mine, "Loading devices…");
    let new = group(&s, "Add a device", "Put the device into pairing mode (usually by holding its power or Bluetooth button), then look for it.");
    let new2 = new.clone();
    action_row(&new, "\u{f002}", "Look for devices", "Takes about 15 seconds", "", move || search(&new2));
    action_row(&new, "\u{f013}", "More Bluetooth options", "Blueman: sending files, devices that need a PIN", "", || {
        crate::common::background("blueman-manager", &[]);
    });

    let mine = Rc::new(mine);
    in_background(paired, move |device| {
        if mine.root().is_none() {
            return;
        }
        clear(&mine);
        if device.is_empty() {
            add_row(&mine, "", "No paired devices yet", "Add one below", None);
        }
        for z in device {
            let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let z2 = z.clone();
            buttons.append(&button(if z.connected { "Disconnect" } else { "Connect" }, !z.connected, move || {
                super::notify(&format!("{} {}…", if z2.connected { "Disconnecting" } else { "Connecting" }, z2.name));
                bt(
                    vec![if z2.connected { "disconnect" } else { "connect" }.into(), z2.mac.clone()],
                    if z2.connected { format!("{} disconnected", z2.name) } else { format!("{} connected", z2.name) },
                    format!("Couldn't reach {} – is it on and nearby?", z2.name),
                );
            }));
            let z2 = z.clone();
            buttons.append(&button("Forget", false, move || {
                bt(vec!["remove".into(), z2.mac.clone()], format!("{} forgotten", z2.name), "Couldn't forget the device".into());
            }));
            let mut description = vec![z.kind.to_string(), if z.connected { "Connected".into() } else { "Not connected".into() }];
            if !z.battery.is_empty() {
                description.push(format!("Battery {} %", z.battery));
            }
            add_row(&mine, z.icon, &z.name, &description.join("  ·  "), Some(buttons.upcast_ref()));
        }
    });
    s.upcast()
}

/// Search for new devices (15 s), then the list of found unpaired ones; a click pairs and connects.
fn search(card: &gtk::ListBox) {
    super::notify("Looking for devices…");
    let card = card.clone();
    in_background(
        || {
            command("bluetoothctl", &["--timeout", "15", "scan", "on"]);
            let paired: Vec<String> = list(&["devices", "Paired"]).into_iter().map(|(m, _)| m).collect();
            list(&["devices"])
                .into_iter()
                // only devices with a real name (otherwise bluetoothctl shows the address)
                .filter(|(m, j)| !paired.contains(m) && j.replace('-', ":") != *m)
                .map(|(m, j)| info(&m, &j))
                .collect::<Vec<_>>()
        },
        move |found| {
            if card.root().is_none() {
                return;
            }
            // remove the old results (the first two rows are buttons)
            while let Some(r) = card.row_at_index(2) {
                card.remove(&r);
            }
            if found.is_empty() {
                add_row(&card, "", "No new devices found", "Is the device in pairing mode?", None);
                return;
            }
            for z in found {
                let z2 = z.clone();
                action_row(&card, z.icon, &z.name, z.kind, "Pair", move || {
                    super::notify(&format!("Pairing {}…", z2.name));
                    let (mac, name) = (z2.mac.clone(), z2.name.clone());
                    in_background(
                        move || {
                            let t = |a: &[&str]| {
                                let mut v = vec!["30", "bluetoothctl"];
                                v.extend_from_slice(a);
                                command("timeout", &v).to_lowercase()
                            };
                            let ok = t(&["pair", &mac]).contains("successful");
                            if ok {
                                t(&["trust", &mac]);
                                t(&["connect", &mac]);
                            }
                            ok
                        },
                        move |ok| {
                            super::notify(&if ok { format!("{name} paired") } else { format!("Couldn't pair {name} – try More Bluetooth options") });
                            super::refresh("bluetooth");
                        },
                    );
                });
            }
        },
    );
}
