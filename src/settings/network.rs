//! Network page: wired (state, details), VPN (connect / disconnect, import a .ovpn
//! or WireGuard .conf file, remove), sharing the internet over Wi-Fi (hotspot) and airplane mode.

use gtk::prelude::*;

use super::ui::*;
use super::wifi_details;
use crate::common::{in_background, command};

struct State {
    /// (device, state, connection)
    kabel: Vec<(String, String, String)>,
    /// (name, type, active)
    vpn: Vec<(String, String, bool)>,
    hotspot: bool,
    airplane: bool,
}

fn load() -> State {
    let kabel = command("nmcli", &["-t", "-e", "no", "-f", "DEVICE,TYPE,STATE,CONNECTION", "dev"])
        .lines()
        .filter_map(|r| {
            let c: Vec<&str> = r.splitn(4, ':').collect();
            (c.len() == 4 && c[1] == "ethernet").then(|| (c[0].to_string(), c[2].to_string(), c[3].to_string()))
        })
        .collect();
    let active = command("nmcli", &["-t", "-e", "no", "-f", "NAME", "connection", "show", "--active"]);
    let active: Vec<&str> = active.lines().collect();
    let mut hotspot = false;
    let vpn = command("nmcli", &["-t", "-e", "no", "-f", "NAME,TYPE", "connection", "show"])
        .lines()
        .filter_map(|r| {
            let (n, t) = r.rsplit_once(':')?;
            if n == "Hotspot" && active.contains(&n) {
                hotspot = true;
            }
            matches!(t, "vpn" | "wireguard").then(|| (n.to_string(), t.to_string(), active.contains(&n)))
        })
        .collect();
    let airplane = command("nmcli", &["radio", "all"]).lines().nth(1).is_some_and(|r| r.split_whitespace().nth(1) == Some("disabled"));
    State { kabel, vpn, hotspot, airplane }
}

/// nmcli in the background, then a toast and a page rebuild.
fn nmcli(args: Vec<String>, done: &'static str) {
    in_background(
        move || std::process::Command::new("nmcli").args(&args).output().map(|v| (v.status.success(), String::from_utf8_lossy(&v.stderr).lines().next().unwrap_or("").replace("Error: ", ""))).unwrap_or((false, String::new())),
        move |(ok, error)| {
            super::notify(&if ok { done.to_string() } else { format!("Didn't work: {error}") });
            super::refresh("network");
        },
    );
}

fn t(s: &str) -> String {
    s.to_string()
}

pub fn create() -> gtk::Widget {
    let s = page("Network");
    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.append(&wrapper);
    let k = group(&wrapper, "", "");
    loading(&k, "Loading…");
    let o2 = wrapper.clone();
    in_background(load, move |st| {
        if o2.root().is_none() {
            return;
        }
        while let Some(d) = o2.first_child() {
            o2.remove(&d);
        }
        show(&o2, &st);
    });
    s.upcast()
}

fn show(s: &gtk::Box, st: &State) {
    // letadlo
    let card = group(s, "", "");
    switch_row(&card, "Airplane mode", "Turns off Wi-Fi and Bluetooth", st.airplane, |z| {
        crate::common::background("rfkill", &[if z { "block" } else { "unblock" }, "bluetooth"]);
        nmcli(vec![t("radio"), t("all"), t(if z { "off" } else { "on" })], if z { "Airplane mode on" } else { "Airplane mode off" });
    });

    // kabel
    let card = group(s, "Wired", "");
    if st.kabel.is_empty() {
        add_row(&card, "", "No network port", "", None);
    }
    for (device, state, connection) in &st.kabel {
        match state.as_str() {
            "connected" => {
                let sp = connection.clone();
                let details = button("Details", false, move || wifi_details::details(&sp));
                add_row(&card, "\u{f6ff}", "Connected", connection, Some(details.upcast_ref()));
            }
            "unavailable" => {
                add_row(&card, "\u{f6ff}", "Cable unplugged", device, None);
            }
            _ => {
                let z = device.clone();
                let connect = button("Connect", true, move || nmcli(vec![t("dev"), t("connect"), z.clone()], "Connected"));
                add_row(&card, "\u{f6ff}", "Not connected", device, Some(connect.upcast_ref()));
            }
        }
    }

    // VPN
    let card = group(s, "VPN", "Your provider gives you a file (.ovpn for OpenVPN, .conf for WireGuard).");
    for (name, typ, active) in &st.vpn {
        let n = name.clone();
        let switcher = gtk::Switch::new();
        switcher.set_active(*active);
        switcher.connect_state_set(move |_, z| {
            super::notify(if z { "Connecting…" } else { "Disconnecting…" });
            nmcli(vec![t("connection"), t(if z { "up" } else { "down" }), t("id"), n.clone()], if z { "VPN connected" } else { "VPN disconnected" });
            gtk::glib::Propagation::Proceed
        });
        let n = name.clone();
        let remove = button("Remove", false, move || nmcli(vec![t("connection"), t("delete"), t("id"), n.clone()], "VPN removed"));
        let right = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        right.append(&remove);
        right.append(&switcher);
        add_row(&card, "\u{f3ed}", name, if typ == "wireguard" { "WireGuard" } else { "VPN" }, Some(right.upcast_ref()));
    }
    action_row(&card, "\u{f067}", "Add a VPN from a file…", "", "", || {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("VPN file from your provider");
        dialog.open(super::window().as_ref(), gtk::gio::Cancellable::NONE, |v| {
            let Some(p) = v.ok().and_then(|f| f.path()) else { return };
            let typ = if p.extension().is_some_and(|e| e == "conf") { "wireguard" } else { "openvpn" };
            nmcli(vec![t("connection"), t("import"), t("type"), t(typ), t("file"), p.to_string_lossy().to_string()], "VPN added – switch it on to connect");
        });
    });

    // hotspot
    let card = group(s, "Share internet", "Lets a phone or another computer use this laptop's internet over Wi-Fi. While it's on, this laptop needs the internet over a cable (it has only one Wi-Fi card).");
    let name = gtk::Entry::new();
    name.set_text("Legion");
    name.set_width_chars(16);
    let password = gtk::PasswordEntry::new();
    password.set_show_peek_icon(true);
    password.set_width_chars(16);
    password.set_property("placeholder-text", "At least 8 characters");
    if !st.hotspot {
        add_row(&card, "", "Network name", "", Some(name.upcast_ref()));
        add_row(&card, "", "Password", "", Some(password.upcast_ref()));
    }
    switch_row(&card, "Hotspot", if st.hotspot { "On – others can join now" } else { "" }, st.hotspot, move |z| {
        if z {
            let h = password.text().to_string();
            if h.chars().count() < 8 {
                super::notify("The password needs at least 8 characters");
                super::refresh("network");
                return;
            }
            nmcli(vec![t("dev"), t("wifi"), t("hotspot"), t("ssid"), name.text().to_string(), t("password"), h], "Hotspot on");
        } else {
            nmcli(vec![t("connection"), t("down"), t("id"), t("Hotspot")], "Hotspot off");
        }
    });
}
