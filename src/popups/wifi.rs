//! Wi-Fi: on/off, list of networks (the 10 strongest), connecting – for a new network with the password right in the popup.

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::common::*;

#[derive(Clone, Default)]
struct Network {
    signal: i64,
    connected: bool,
    lock: bool,
}

type Site = (BTreeMap<String, Network>, HashSet<String>);

fn load(again: bool) -> Site {
    if again {
        // during a scan nmcli returns only some networks: start the scan, wait for it, then read
        command("nmcli", &["dev", "wifi", "rescan"]);
        std::thread::sleep(Duration::from_secs(3));
    }
    let mut site: BTreeMap<String, Network> = BTreeMap::new();
    for row in command("nmcli", &["-t", "-e", "no", "-f", "IN-USE,SIGNAL,SECURITY,SSID", "dev", "wifi", "list", "--rescan", "no"]).lines() {
        let parts: Vec<&str> = row.splitn(4, ':').collect();
        if parts.len() == 4 && !parts[3].is_empty() {
            let s = site.entry(parts[3].to_string()).or_default();
            s.signal = s.signal.max(parts[1].parse().unwrap_or(0));
            s.connected |= parts[0] == "*";
            s.lock |= !parts[2].is_empty() && parts[2] != "--";
        }
    }
    let saved = command("nmcli", &["-t", "-e", "no", "-f", "NAME,TYPE", "connection", "show"])
        .lines()
        .filter_map(|r| r.strip_suffix(":802-11-wireless").map(String::from))
        .collect();
    (site, saved)
}

struct Wifi {
    o: Rc<Popup>,
    list: gtk::Box,
    password_box: gtk::Box,
}

pub fn fill(o: &Rc<Popup>) {
    let enabled = wifi_enabled();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let password_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let w = Rc::new(Wifi { o: o.clone(), list: list.clone(), password_box: password_box.clone() });

    let w2 = w.clone();
    o.panel.append(&header_s_switch("Wi-Fi", enabled, move |state| {
        background("nmcli", &["radio", "wifi", if state { "on" } else { "off" }]);
        w2.list.set_visible(state);
        if state {
            // after turning on, Wi-Fi takes a moment to find networks
            let w3 = w2.clone();
            glib::timeout_add_local_once(Duration::from_millis(2500), move || w3.load(true));
        }
    }));
    o.panel.append(&list);
    o.panel.append(&password_box);
    o.panel.append(&separator());
    let dole = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let settings = link_settings(o, "Wi-Fi settings…", "wifi");
    settings.set_hexpand(true);
    dole.append(&settings);
    let refresh = gtk::Button::with_label("\u{f2f1}  Refresh");
    refresh.add_css_class("link");
    let w2 = w.clone();
    refresh.connect_clicked(move |_| w2.load(true));
    dole.append(&refresh);
    o.panel.append(&dole);

    list.set_visible(enabled);
    // networks from the last scan show at once, fresh ones after a new scan
    w.load(false);
    if enabled {
        w.load(true);
    }
}

impl Wifi {
    fn load(self: &Rc<Self>, again: bool) {
        let w = self.clone();
        in_background(move || load(again), move |data| w.show(data));
    }

    fn show(self: &Rc<Self>, (site, saved): Site) {
        // the network scan takes a few seconds, the popup may have closed meanwhile
        if self.o.closed() {
            return;
        }
        while let Some(d) = self.list.first_child() {
            self.list.remove(&d);
        }
        let mut sorted: Vec<(String, Network)> = site.into_iter().collect();
        sorted.sort_by_key(|(_, s)| (!s.connected, -s.signal));
        for (ssid, s) in sorted.into_iter().take(10) {
            let right = format!(
                "{}{}{} %",
                if s.connected { "\u{f00c}  " } else { "" },
                if s.lock { "\u{f023}  " } else { "" },
                s.signal
            );
            let w = self.clone();
            let saved = saved.contains(&ssid);
            let (ssid2, s2) = (ssid.clone(), s.clone());
            self.list.append(&row("\u{f1eb}", &ssid, &right, s.connected, move || {
                w.connect(&ssid2, &s2, saved, None)
            }));
        }
        self.o.recompute();
    }

    fn connect(self: &Rc<Self>, ssid: &str, s: &Network, saved: bool, password: Option<String>) {
        if s.connected {
            return;
        }
        if s.lock && !saved && password.is_none() {
            return self.ask_on_password(ssid, s);
        }
        self.o.state(&format!("Connecting to {ssid}…"));
        let ssid_thread = ssid.to_string();
        let ssid_done = ssid.to_string();
        let o = self.o.clone();
        in_background(
            move || {
                let mut c = std::process::Command::new("nmcli");
                if saved && password.is_none() {
                    c.args(["connection", "up", "id", &ssid_thread]);
                } else {
                    c.args(["dev", "wifi", "connect", &ssid_thread]);
                    if let Some(h) = &password {
                        c.args(["password", h]);
                    }
                }
                c.output().map(|v| v.status.success()).unwrap_or(false)
            },
            move |ok| {
                if ok {
                    o.state(&format!("Connected to {ssid_done}"));
                    let o2 = o.clone();
                    glib::timeout_add_local_once(Duration::from_millis(900), move || o2.close());
                } else {
                    o.state("Couldn't connect – wrong password or the network is out of range");
                }
            },
        );
    }

    fn ask_on_password(self: &Rc<Self>, ssid: &str, s: &Network) {
        while let Some(d) = self.password_box.first_child() {
            self.password_box.remove(&d);
        }
        self.o.keep_open.set(true);
        self.password_box.append(&label(&format!("Password for {ssid}"), &["dim"]));
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let field = gtk::Entry::new();
        field.set_visibility(false);
        field.set_placeholder_text(Some("Password"));
        field.set_hexpand(true);
        let button = gtk::Button::with_label("Connect");
        button.add_css_class("enabled");
        row.append(&field);
        row.append(&button);
        self.password_box.append(&row);
        let send = {
            let (w, ssid, s, field) = (self.clone(), ssid.to_string(), s.clone(), field.clone());
            move || w.connect(&ssid, &s, false, Some(field.text().to_string()))
        };
        let o2 = send.clone();
        field.connect_activate(move |_| o2());
        button.connect_clicked(move |_| send());
        field.grab_focus();
        self.o.recompute();
    }
}
