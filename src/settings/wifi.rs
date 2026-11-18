//! Wi-Fi page: on/off, connected network (IP, auto-connect, disconnect, forget),
//! available networks (password right in the row), saved networks and a hidden network. Everything through nmcli
//! in the background; networks from the last scan first, then fresh ones. Only a few available networks show, the rest
//! after a click. Network details (IP, DNS, password…) and enterprise networks (eduroam) are in wifi_details.rs.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use super::ui::*;
use super::wifi_details;
use crate::common::{in_background, command, wifi_enabled};

/// how many available networks to show before the list collapses
const SHOW: usize = 8;

thread_local! {
    /// the user expanded the whole list (valid until the page is opened again)
    static EXPANDED: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Default)]
struct Network {
    signal: i64,
    connected: bool,
    lock: bool,
    /// WPA2/3-Enterprise (eduroam, company networks): a username and password instead of one password
    enterprise: bool,
}

#[derive(Default)]
struct State {
    site: Vec<(String, Network)>,
    /// saved Wi-Fi connections (name = SSID)
    saved: Vec<String>,
    /// connected network: (connection name, device)
    connected: Option<(String, String)>,
    ip: String,
    automatic: bool,
}

fn load(again: bool) -> State {
    if again {
        // during a scan nmcli returns only some networks: start the scan, wait, then read
        command("nmcli", &["dev", "wifi", "rescan"]);
        std::thread::sleep(Duration::from_secs(3));
    }
    let mut site: BTreeMap<String, Network> = BTreeMap::new();
    for r in command("nmcli", &["-t", "-e", "no", "-f", "IN-USE,SIGNAL,SECURITY,SSID", "dev", "wifi", "list", "--rescan", "no"]).lines() {
        let c: Vec<&str> = r.splitn(4, ':').collect();
        if c.len() == 4 && !c[3].is_empty() {
            let s = site.entry(c[3].to_string()).or_default();
            s.signal = s.signal.max(c[1].parse().unwrap_or(0));
            s.connected |= c[0] == "*";
            s.lock |= !c[2].is_empty() && c[2] != "--";
            s.enterprise |= c[2].contains("802.1X");
        }
    }
    let mut site: Vec<(String, Network)> = site.into_iter().collect();
    site.sort_by_key(|(_, s)| (!s.connected, -s.signal));
    let saved = command("nmcli", &["-t", "-e", "no", "-f", "NAME,TYPE", "connection", "show"])
        .lines()
        .filter_map(|r| r.strip_suffix(":802-11-wireless").map(String::from))
        .collect();
    let connected = command("nmcli", &["-t", "-e", "no", "-f", "DEVICE,TYPE,STATE,CONNECTION", "dev"])
        .lines()
        .find_map(|r| {
            let c: Vec<&str> = r.splitn(4, ':').collect();
            (c.len() == 4 && c[1] == "wifi" && c[2] == "connected").then(|| (c[3].to_string(), c[0].to_string()))
        });
    let (ip, automatic) = match &connected {
        Some((connection, device)) => (
            command("nmcli", &["-g", "IP4.ADDRESS", "dev", "show", device]).lines().next().unwrap_or("").split('/').next().unwrap_or("").to_string(),
            command("nmcli", &["-g", "connection.autoconnect", "connection", "show", connection]).trim() == "yes",
        ),
        None => (String::new(), false),
    };
    State { site, saved, connected, ip, automatic }
}

/// nmcli in the background; when done, a toast at the bottom of the window and a page rebuild.
pub fn nmcli(args: Vec<String>, done: &'static str, error: &'static str) {
    in_background(
        move || {
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            std::process::Command::new("nmcli").args(&a).output().map(|v| v.status.success()).unwrap_or(false)
        },
        move |ok| {
            super::notify(if ok { done } else { error });
            super::refresh("wifi");
        },
    );
}

fn icon_signal(signal: i64) -> &'static str {
    // Font Awesome has only one Wi-Fi icon; the strength is the number on the right
    let _ = signal;
    "\u{f1eb}"
}

pub fn create() -> gtk::Widget {
    let s = page("Wi-Fi");
    EXPANDED.with(|r| r.set(false));
    let enabled = wifi_enabled();
    let main = group(&s, "", "");
    switch_row(&main, "Wi-Fi", "Connect to wireless networks", enabled, |state| {
        crate::common::background("nmcli", &["radio", "wifi", if state { "on" } else { "off" }]);
        super::notify(if state { "Wi-Fi on, looking for networks…" } else { "Wi-Fi off" });
        // after turning on it takes a moment to find networks
        gtk::glib::timeout_add_local_once(Duration::from_millis(if state { 4000 } else { 400 }), || super::refresh("wifi"));
    });
    if !enabled {
        return s.upcast();
    }

    let connected = group(&s, "Connected network", "");
    let available = group(&s, "Available networks", "");
    let saved = group(&s, "Saved networks", "Networks this computer remembers and joins on its own.");
    let next = group(&s, "", "");
    loading(&available, "Looking for networks…");
    let hidden = {
        let next = next.clone();
        move || formular_hidden(&next)
    };
    action_row(&next, "\u{f067}", "Connect to a hidden network…", "A network that doesn't show its name", "", hidden);
    action_row(&next, "\u{f19d}", "Connect to eduroam or a work network…", "Networks where you sign in with a username and password", "\u{f054}", || wifi_details::enterprise(""));
    let k = next.clone();
    action_row(&k, "\u{f2f1}", "Look for networks again", "", "", || {
        super::notify("Looking for networks…");
        in_background(|| load(true), |_| super::refresh("wifi"));
    });

    let cards = Rc::new((connected, available, saved));
    // networks from the last scan first (at once), then fresh ones (a few seconds)
    let k = cards.clone();
    in_background(|| load(false), move |state| {
        show(&k, &state);
        let k = k.clone();
        in_background(|| load(true), move |state| show(&k, &state));
    });
    s.upcast()
}

fn show(cards: &Rc<(gtk::ListBox, gtk::ListBox, gtk::ListBox)>, state: &State) {
    if !super::is_open() || cards.0.root().is_none() {
        return; // the page disappeared meanwhile
    }
    let (connected, available, saved) = (&cards.0, &cards.1, &cards.2);

    clear(connected);
    match &state.connected {
        Some((connection, _)) => {
            let signal = state.site.iter().find(|(n, s)| s.connected || n == connection).map(|(_, s)| s.signal).unwrap_or(0);
            let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let sp = connection.clone();
            buttons.append(&button("Disconnect", false, move || {
                nmcli(vec!["connection".into(), "down".into(), "id".into(), sp.clone()], "Disconnected", "Couldn't disconnect");
            }));
            let sp = connection.clone();
            buttons.append(&button("Details", false, move || wifi_details::details(&sp)));
            let description = if state.ip.is_empty() { format!("Signal {signal} %") } else { format!("Signal {signal} %  ·  IP address {}", state.ip) };
            add_row(connected, icon_signal(signal), connection, &description, Some(buttons.upcast_ref()));
            let sp = connection.clone();
            switch_row(connected, "Connect automatically", "Join this network whenever it is in range", state.automatic, move |a| {
                nmcli(
                    vec!["connection".into(), "modify".into(), "id".into(), sp.clone(), "connection.autoconnect".into(), if a { "yes" } else { "no" }.into()],
                    if a { "Will connect automatically" } else { "Won't connect automatically" },
                    "Couldn't change the setting",
                );
            });
        }
        None => {
            add_row(connected, "", "Not connected", "Pick a network below", None);
        }
    }

    clear(available);
    let visible: Vec<&(String, Network)> = state.site.iter().filter(|(_, s)| !s.connected).collect();
    if visible.is_empty() {
        add_row(available, "", "No other networks found", "", None);
    }
    let expanded = EXPANDED.with(|r| r.get());
    let mut hidden = Vec::new();
    for (i, (ssid, network)) in visible.iter().map(|x| (&x.0, &x.1)).enumerate() {
        let saved = state.saved.contains(ssid);
        let description = match (network.lock, saved, network.enterprise) {
            (_, true, _) => "Saved",
            (_, false, true) => "Secured with a username (e.g. eduroam)",
            (true, false, false) => "Secured",
            (false, false, false) => "Open",
        };
        let right = format!("{}{} %", if network.lock { "\u{f023}  " } else { "" }, network.signal);
        let (ssid2, lock, enterprise) = (ssid.clone(), network.lock, network.enterprise);
        let card = available.clone();
        let row = action_row(available, icon_signal(network.signal), ssid, description, &right, || {});
        if i >= SHOW && !expanded {
            row.set_visible(false);
            hidden.push(row.clone());
        }
        // click: a saved / open network connects at once, a new secured one asks for the password under the row
        let button_rows = row.child().and_downcast::<gtk::Button>();
        if let Some(b) = button_rows {
            let row2 = row.clone();
            b.connect_clicked(move |_| {
                if saved {
                    super::notify(&format!("Connecting to {ssid2}…"));
                    nmcli(vec!["connection".into(), "up".into(), "id".into(), ssid2.clone()], "Connected", "Couldn't connect – is the network in range?");
                } else if enterprise {
                    wifi_details::enterprise(&ssid2);
                } else if !lock {
                    super::notify(&format!("Connecting to {ssid2}…"));
                    nmcli(vec!["dev".into(), "wifi".into(), "connect".into(), ssid2.clone()], "Connected", "Couldn't connect");
                } else {
                    password_pod(&card, &row2, &ssid2);
                }
            });
        }
    }
    if !hidden.is_empty() {
        let count = hidden.len();
        let expand = action_row(available, "\u{f078}", &format!("Show {count} more networks"), "", "", || {});
        let r2 = expand.clone();
        if let Some(b) = expand.child().and_downcast::<gtk::Button>() {
            b.connect_clicked(move |_| {
                EXPANDED.with(|r| r.set(true));
                for r in &hidden {
                    r.set_visible(true);
                }
                r2.set_visible(false);
            });
        }
    }

    clear(saved);
    let mut ul: Vec<&String> = state.saved.iter().filter(|n| state.connected.as_ref().map(|p| &p.0) != Some(*n)).collect();
    ul.sort();
    if ul.is_empty() {
        add_row(saved, "", "No other saved networks", "", None);
    }
    for name in ul {
        let n = name.clone();
        let in_range = state.site.iter().any(|(s, _)| s == name);
        action_row(saved, "\u{f1eb}", name, if in_range { "In range" } else { "Not in range" }, "\u{f054}", move || wifi_details::details(&n));
    }
}

/// A password field right under the network's row.
fn password_pod(card: &gtk::ListBox, row: &gtk::ListBoxRow, ssid: &str) {
    // only one field at a time
    let mut r = card.first_child();
    while let Some(w) = r {
        r = w.next_sibling();
        if w.widget_name() == "password" {
            card.remove(&w);
        }
    }
    let field = gtk::PasswordEntry::new();
    field.set_show_peek_icon(true);
    field.set_hexpand(true);
    field.set_property("placeholder-text", format!("Password for {ssid}"));
    let connect = {
        let (field, ssid) = (field.clone(), ssid.to_string());
        move || {
            let password = field.text().to_string();
            if password.is_empty() {
                return;
            }
            super::notify(&format!("Connecting to {ssid}…"));
            nmcli(
                vec!["dev".into(), "wifi".into(), "connect".into(), ssid.clone(), "password".into(), password],
                "Connected",
                "Couldn't connect – wrong password or the network is out of range",
            );
        }
    };
    let p2 = connect.clone();
    field.connect_activate(move |_| p2());
    let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    wrapper.add_css_class("row");
    wrapper.append(&field);
    wrapper.append(&button("Connect", true, connect));
    let new = gtk::ListBoxRow::new();
    new.set_widget_name("password");
    new.set_activatable(false);
    new.set_child(Some(&wrapper));
    card.insert(&new, row.index() + 1);
    field.grab_focus();
}

/// Hidden network: name and password.
fn formular_hidden(card: &gtk::ListBox) {
    let mut r = card.first_child();
    while let Some(w) = r {
        r = w.next_sibling();
        if w.widget_name() == "hidden" {
            return; // already open
        }
    }
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("Network name"));
    name.set_hexpand(true);
    let password = gtk::PasswordEntry::new();
    password.set_show_peek_icon(true);
    password.set_hexpand(true);
    password.set_property("placeholder-text", "Password (empty for an open network)");
    let connect = {
        let (name, password) = (name.clone(), password.clone());
        move || {
            let n = name.text().to_string();
            if n.is_empty() {
                return;
            }
            let mut a: Vec<String> = vec!["dev".into(), "wifi".into(), "connect".into(), n.clone(), "hidden".into(), "yes".into()];
            let h = password.text().to_string();
            if !h.is_empty() {
                a.extend(["password".to_string(), h]);
            }
            super::notify(&format!("Connecting to {n}…"));
            nmcli(a, "Connected", "Couldn't connect – check the name and password");
        }
    };
    let p2 = connect.clone();
    password.connect_activate(move |_| p2());
    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 8);
    wrapper.add_css_class("row");
    wrapper.append(&name);
    let dole = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    dole.append(&password);
    dole.append(&button("Connect", true, connect));
    wrapper.append(&dole);
    let row = gtk::ListBoxRow::new();
    row.set_widget_name("hidden");
    row.set_activatable(false);
    row.set_child(Some(&wrapper));
    card.insert(&row, 1);
    name.grab_focus();
}
