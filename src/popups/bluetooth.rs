//! Bluetooth: on/off and connecting / disconnecting paired devices (with the battery of headphones).

use std::rc::Rc;

use gtk::prelude::*;

use crate::common::*;

#[derive(Clone)]
struct Device {
    mac: String,
    name: String,
    connected: bool,
    battery: String,
    icon: &'static str,
}

fn load() -> Vec<Device> {
    let mut result = Vec::new();
    for row in command("bluetoothctl", &["devices", "Paired"]).lines() {
        let parts: Vec<&str> = row.splitn(3, ' ').collect();
        if parts.len() < 3 {
            continue;
        }
        let info = command("bluetoothctl", &["info", parts[1]]);
        let value = |key: &str| info.lines().find_map(|r| r.trim().strip_prefix(key).map(|v| v.trim().to_string()));
        // "Battery Percentage: 0x50 (80)" → 80
        let battery = value("Battery Percentage:")
            .and_then(|v| v.split('(').nth(1).map(|x| x.trim_end_matches(')').to_string()))
            .unwrap_or_default();
        let icon = match value("Icon:").as_deref() {
            Some("audio-headset") | Some("audio-headphones") => "\u{f025}",
            Some("audio-card") => "\u{f028}",
            Some("input-keyboard") => "\u{f11c}",
            Some("input-mouse") => "\u{f8cc}",
            Some("phone") => "\u{f3cd}",
            Some("computer") => "\u{f109}",
            _ => "\u{f293}",
        };
        result.push(Device {
            mac: parts[1].to_string(),
            name: parts[2].to_string(),
            connected: info.contains("Connected: yes"),
            battery,
            icon,
        });
    }
    result.sort_by_key(|z| !z.connected);
    result
}

struct Bluetooth {
    o: Rc<Popup>,
    list: gtk::Box,
}

pub fn fill(o: &Rc<Popup>) {
    let enabled = bluetooth_enabled();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let b = Rc::new(Bluetooth { o: o.clone(), list: list.clone() });
    let b2 = b.clone();
    o.panel.append(&header_s_switch("Bluetooth", enabled, move |state| {
        background("bluetoothctl", &["power", if state { "on" } else { "off" }]);
        b2.list.set_visible(state);
    }));
    o.panel.append(&list);
    list.set_visible(enabled);
    o.panel.append(&separator());
    o.panel.append(&link_settings(o, "Bluetooth settings…", "bluetooth"));
    b.load();
}

impl Bluetooth {
    fn load(self: &Rc<Self>) {
        let b = self.clone();
        in_background(load, move |device| b.show(device));
    }

    fn show(self: &Rc<Self>, device: Vec<Device>) {
        // bluetoothctl is slow, the popup may have closed meanwhile
        if self.o.closed() {
            return;
        }
        while let Some(d) = self.list.first_child() {
            self.list.remove(&d);
        }
        if device.is_empty() {
            self.list.append(&label("No paired devices", &["dim"]));
        }
        for z in device {
            let right = format!(
                "{}{}",
                if z.battery.is_empty() { String::new() } else { format!("{} %  ", z.battery) },
                if z.connected { "Connected" } else { "" }
            );
            let b = self.clone();
            let z2 = z.clone();
            self.list.append(&row(z.icon, &z.name, &right, z.connected, move || b.toggle(&z2)));
        }
        self.o.recompute();
    }

    fn toggle(self: &Rc<Self>, z: &Device) {
        self.o.state(&format!("{} {}…", if z.connected { "Disconnecting" } else { "Connecting" }, z.name));
        let (mac, connected, name) = (z.mac.clone(), z.connected, z.name.clone());
        let b = self.clone();
        in_background(
            move || {
                command("timeout", &["25", "bluetoothctl", if connected { "disconnect" } else { "connect" }, &mac])
                    .to_lowercase()
                    .contains("successful")
            },
            move |ok| {
                b.o.state(&if ok { "Done".to_string() } else { format!("Couldn't connect {name} – is it on and nearby?") });
                b.load();
            },
        );
    }
}
