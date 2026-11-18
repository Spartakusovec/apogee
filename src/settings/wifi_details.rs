//! Wi-Fi subpages: details of a saved network (state, password, auto-connect, metered
//! connection, manual IP address and DNS, forget) and connecting to an enterprise network (eduroam,
//! WPA2/3-Enterprise: PEAP / TTLS, username and password, server certificate check).

use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;

use super::ui::*;
use crate::common::{in_background, background, command};

/// Runs nmcli with several commands one after another; returns the error of the first that fails (first line).
fn nmcli_sequence(commands: Vec<Vec<String>>) -> Result<(), String> {
    for a in commands {
        let v = std::process::Command::new("nmcli").args(&a).output().map_err(|e| e.to_string())?;
        if !v.status.success() {
            let error = String::from_utf8_lossy(&v.stderr).lines().next().unwrap_or("").trim_start_matches("Error: ").to_string();
            return Err(error);
        }
    }
    Ok(())
}

fn t(x: &str) -> String {
    x.to_string()
}

/// “key:value” lines from `nmcli -t -f … connection show id X`.
fn properties(connection: &str, field: &str) -> HashMap<String, String> {
    command("nmcli", &["-t", "-f", field, "connection", "show", "id", connection])
        .lines()
        .filter_map(|r| r.split_once(':').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

fn field(placeholder: &str, text: &str) -> gtk::Entry {
    let e = gtk::Entry::new();
    e.set_placeholder_text(Some(placeholder));
    e.set_text(text);
    e.set_width_chars(24);
    e
}

fn choice(choices: &[&str], selected: u32) -> gtk::DropDown {
    let d = gtk::DropDown::from_strings(choices);
    d.set_selected(selected);
    d
}

// ---------------------------------------------------------------- network details

pub fn details(connection: &str) {
    let s = page(connection);
    let card = group(&s, "", "");
    loading(&card, "Loading…");
    super::subpage("Wi-Fi", s.clone().upcast());
    let sp = connection.to_string();
    in_background(
        move || {
            let settings = properties(&sp, "connection.autoconnect,connection.metered,ipv4.method,ipv4.addresses,ipv4.gateway,ipv4.dns,ipv4.ignore-auto-dns,802-11-wireless-security.key-mgmt,802-1x.identity,802-1x.eap");
            let active = properties(&sp, "IP4,GENERAL.STATE");
            (sp, settings, active)
        },
        move |(sp, n, a)| {
            if s.root().is_none() {
                return;
            }
            // the real content instead of “Loading…”
            if let Some(parent) = card.parent() {
                s.remove(&parent);
            }
            show_details(&s, &sp, &n, &a);
        },
    );
}

fn show_details(s: &gtk::Box, connection: &str, n: &HashMap<String, String>, a: &HashMap<String, String>) {
    let value = |k: &str| n.get(k).cloned().unwrap_or_default();
    let connected = a.get("GENERAL.STATE").is_some_and(|v| v == "activated");

    // stav
    let card = group(s, "", "");
    if connected {
        let ip = a.get("IP4.ADDRESS[1]").cloned().unwrap_or_default();
        let dns: Vec<String> = (1..5).filter_map(|i| a.get(&format!("IP4.DNS[{i}]")).cloned()).collect();
        let sp = connection.to_string();
        let disconnect = button("Disconnect", false, move || {
            super::notify("Disconnecting…");
            let sp = sp.clone();
            in_background(move || nmcli_sequence(vec![vec![t("connection"), t("down"), t("id"), sp]]), |_| super::refresh_active());
        });
        add_row(&card, "\u{f1eb}", "Connected", "", Some(disconnect.upcast_ref()));
        add_row(&card, "", "IP address", "", Some(label_right(ip.split('/').next().unwrap_or("")).upcast_ref()));
        add_row(&card, "", "Router", "", Some(label_right(&a.get("IP4.GATEWAY").cloned().unwrap_or_default()).upcast_ref()));
        add_row(&card, "", "DNS", "", Some(label_right(&dns.join(", ")).upcast_ref()));
    } else {
        let sp = connection.to_string();
        let connect = button("Connect", true, move || {
            super::notify("Connecting…");
            let sp = sp.clone();
            in_background(
                move || nmcli_sequence(vec![vec![t("connection"), t("up"), t("id"), sp]]),
                |v| {
                    super::notify(if v.is_ok() { "Connected" } else { "Couldn't connect – is the network in range?" });
                    super::refresh_active();
                },
            );
        });
        add_row(&card, "\u{f1eb}", "Not connected", "", Some(connect.upcast_ref()));
    }
    let security = match value("802-11-wireless-security.key-mgmt").as_str() {
        "wpa-psk" => "WPA2 password",
        "sae" => "WPA3 password",
        "wpa-eap" | "wpa-eap-suite-b-192" => "Username and password (Enterprise)",
        "none" | "" => "Open, not secured",
        x => x,
    }
    .to_string();
    if value("802-11-wireless-security.key-mgmt").starts_with("wpa-eap") {
        add_row(&card, "\u{f023}", "Security", &security, None);
        add_row(&card, "\u{f007}", "Username", &value("802-1x.identity"), None);
    } else if !matches!(value("802-11-wireless-security.key-mgmt").as_str(), "none" | "") {
        // the password is loaded only on request (from NetworkManager, which may ask for the account password)
        let password = gtk::Label::new(Some("••••••••"));
        password.add_css_class("description");
        password.set_selectable(true);
        let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        wrapper.append(&password);
        let (h, sp) = (password.clone(), connection.to_string());
        let show = button("Show", false, || {});
        let u2 = show.clone();
        show.connect_clicked(move |_| {
            let (h, u, sp) = (h.clone(), u2.clone(), sp.clone());
            in_background(
                move || command("nmcli", &["-s", "-g", "802-11-wireless-security.psk", "connection", "show", "id", &sp]),
                move |password| {
                    if password.trim().is_empty() {
                        super::notify("Couldn't read the password");
                    } else {
                        h.set_text(password.trim());
                        u.set_visible(false);
                    }
                },
            );
        });
        wrapper.append(&show);
        add_row(&card, "\u{f084}", "Password", &security, Some(wrapper.upcast_ref()));
    } else {
        add_row(&card, "\u{f13e}", "Security", &security, None);
    }

    // behaviour
    let card = group(s, "", "");
    let sp = connection.to_string();
    switch_row(&card, "Connect automatically", "Join this network whenever it is in range", value("connection.autoconnect") == "yes", move |v| {
        modify(&sp, vec![t("connection.autoconnect"), t(if v { "yes" } else { "no" })], false);
    });
    let sp = connection.to_string();
    switch_row(
        &card,
        "Metered connection",
        "For a phone hotspot or limited data: updates and other background downloads wait",
        value("connection.metered") == "yes",
        move |v| modify(&sp, vec![t("connection.metered"), t(if v { "yes" } else { "no" })], false),
    );

    // IP address and DNS
    let card = group(s, "IP settings", "Leave on Automatic unless the network needs a fixed address.");
    let manual = value("ipv4.method") == "manual";
    let method = choice(&["Automatic (DHCP)", "Manual"], manual as u32);
    add_row(&card, "", "IPv4 address", "", Some(method.upcast_ref()));
    let address = field("192.168.0.50/24", &value("ipv4.addresses"));
    let brana = field("192.168.0.1", &value("ipv4.gateway"));
    let r_address = add_row(&card, "", "Address", "With /24 at the end for a normal home network", Some(address.upcast_ref()));
    let r_brana = add_row(&card, "", "Router (gateway)", "", Some(brana.upcast_ref()));
    let custom_dns = !value("ipv4.dns").is_empty();
    let dns_choice = choice(&["Automatic", "Custom"], custom_dns as u32);
    add_row(&card, "", "DNS", "Custom DNS, e.g. 1.1.1.1 (Cloudflare) or 9.9.9.9 (Quad9)", Some(dns_choice.upcast_ref()));
    let dns = field("1.1.1.1, 1.0.0.1", &value("ipv4.dns"));
    let r_dns = add_row(&card, "", "DNS servers", "Separated by commas", Some(dns.upcast_ref()));
    let visibility = {
        let (m, d, ra, rb, rd) = (method.clone(), dns_choice.clone(), r_address.clone(), r_brana.clone(), r_dns.clone());
        move || {
            let manual = m.selected() == 1;
            ra.set_visible(manual);
            rb.set_visible(manual);
            rd.set_visible(d.selected() == 1);
        }
    };
    visibility();
    let v = Rc::new(visibility);
    let v2 = v.clone();
    method.connect_selected_notify(move |_| v2());
    dns_choice.connect_selected_notify(move |_| v());
    let sp = connection.to_string();
    let save = button("Save", true, move || {
        let mut z: Vec<String> = Vec::new();
        if method.selected() == 1 {
            let mut a = address.text().trim().to_string();
            if a.is_empty() {
                super::notify("Fill in the address");
                return;
            }
            if !a.contains('/') {
                a.push_str("/24");
            }
            z.extend([t("ipv4.method"), t("manual"), t("ipv4.addresses"), a, t("ipv4.gateway"), brana.text().trim().to_string()]);
        } else {
            z.extend([t("ipv4.method"), t("auto"), t("ipv4.addresses"), t(""), t("ipv4.gateway"), t("")]);
        }
        if dns_choice.selected() == 1 && !dns.text().trim().is_empty() {
            z.extend([t("ipv4.dns"), dns.text().replace(' ', ""), t("ipv4.ignore-auto-dns"), t("yes")]);
        } else {
            z.extend([t("ipv4.dns"), t(""), t("ipv4.ignore-auto-dns"), t("no")]);
        }
        modify(&sp, z, connected);
    });
    save.set_halign(gtk::Align::End);
    s.append(&save);

    // zapomenout
    let card = group(s, "", "");
    let sp = connection.to_string();
    let forget = button("Forget", false, move || {
        let sp = sp.clone();
        in_background(
            move || nmcli_sequence(vec![vec![t("connection"), t("delete"), t("id"), sp]]),
            |v| {
                super::notify(if v.is_ok() { "Network forgotten" } else { "Couldn't forget the network" });
                super::refresh_active();
            },
        );
    });
    add_row(&card, "\u{f1f8}", "Forget this network", "Removes the saved password; you'll need it to connect again", Some(forget.upcast_ref()));
}

fn label_right(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(if text.is_empty() { "–" } else { text }));
    l.add_css_class("description");
    l.set_selectable(true);
    l
}

/// `nmcli connection modify` (and reconnect a connected network, so the change takes effect).
fn modify(connection: &str, changes: Vec<String>, again_connect: bool) {
    let mut commands = vec![[vec![t("connection"), t("modify"), t("id"), connection.to_string()], changes].concat()];
    if again_connect {
        commands.push(vec![t("connection"), t("up"), t("id"), connection.to_string()]);
    }
    let sp = connection.to_string();
    in_background(
        move || nmcli_sequence(commands),
        move |v| match v {
            Ok(()) => {
                super::notify(if again_connect { "Saved and reconnected" } else { "Saved" });
                if again_connect {
                    details(&sp);
                }
            }
            Err(e) => super::notify(&format!("Not saved: {e}")),
        },
    );
}

// ---------------------------------------------------------------- enterprise network (eduroam)

pub fn enterprise(ssid: &str) {
    let ssid = if ssid.is_empty() { "eduroam" } else { ssid };
    let s = page("Connect with a username");
    let card = group(
        &s,
        "",
        "Schools and companies use these networks. If something below is unclear, your school's IT page (or cat.eduroam.org) lists the right settings.",
    );
    let name = field("Network name", ssid);
    add_row(&card, "\u{f1eb}", "Network", "", Some(name.upcast_ref()));
    let name = field("you@school.edu", "");
    add_row(&card, "\u{f007}", "Username", "For eduroam your full school email", Some(name.upcast_ref()));
    let password = gtk::PasswordEntry::new();
    password.set_show_peek_icon(true);
    password.set_width_chars(24);
    add_row(&card, "\u{f084}", "Password", "", Some(password.upcast_ref()));

    let card = group(&s, "Advanced", "The defaults work for most eduroam networks.");
    let eap = choice(&["PEAP", "TTLS"], 0);
    add_row(&card, "", "Method", "", Some(eap.upcast_ref()));
    let inner = choice(&["MSCHAPv2", "PAP", "GTC"], 0);
    add_row(&card, "", "Inner authentication", "", Some(inner.upcast_ref()));
    let anonymous = field("anonymous@school.edu", "");
    add_row(&card, "", "Anonymous identity", "Optional; filled in from your username if empty", Some(anonymous.upcast_ref()));
    let domain = field("e.g. radius.school.edu", "");
    add_row(&card, "", "Server domain", "Optional; makes sure you talk to your school's server", Some(domain.upcast_ref()));
    let certificate = choice(&["System certificates", "Certificate file…", "Don't check (not secure)"], 0);
    add_row(&card, "\u{f132}", "Server certificate", "", Some(certificate.upcast_ref()));
    let file: Rc<std::cell::RefCell<Option<String>>> = Rc::new(std::cell::RefCell::new(None));
    let (file2, cert2) = (file.clone(), certificate.clone());
    certificate.connect_selected_notify(move |d| {
        if d.selected() != 1 {
            return;
        }
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Certificate from your school or company");
        let (file, cert) = (file2.clone(), cert2.clone());
        let window = d.root().and_downcast::<gtk::Window>();
        dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |v| match v.ok().and_then(|f| f.path()) {
            Some(p) => {
                super::notify(&format!("Certificate: {}", p.file_name().unwrap_or_default().to_string_lossy()));
                file.replace(Some(p.to_string_lossy().to_string()));
            }
            None => cert.set_selected(0),
        });
    });

    let connect = button("Connect", true, move || {
        let (n, j, h) = (name.text().trim().to_string(), name.text().trim().to_string(), password.text().to_string());
        if n.is_empty() || j.is_empty() || h.is_empty() {
            super::notify("Fill in the network, username and password");
            return;
        }
        let mut anon = anonymous.text().trim().to_string();
        if anon.is_empty() {
            if let Some((_, realm)) = j.split_once('@') {
                anon = format!("anonymous@{realm}");
            }
        }
        let mut a = vec![
            t("connection"), t("add"), t("type"), t("wifi"), t("con-name"), n.clone(), t("ssid"), n.clone(),
            t("wifi-sec.key-mgmt"), t("wpa-eap"),
            t("802-1x.eap"), t(["peap", "ttls"][eap.selected() as usize]),
            t("802-1x.phase2-auth"), t(["mschapv2", "pap", "gtc"][inner.selected() as usize]),
            t("802-1x.identity"), j, t("802-1x.password"), h,
        ];
        let device = command("nmcli", &["-t", "-f", "DEVICE,TYPE", "dev"]).lines().find_map(|r| r.strip_suffix(":wifi").map(String::from));
        if let Some(z) = device {
            a.extend([t("ifname"), z]);
        }
        if !anon.is_empty() {
            a.extend([t("802-1x.anonymous-identity"), anon]);
        }
        let d = domain.text().trim().to_string();
        if !d.is_empty() {
            a.extend([t("802-1x.domain-suffix-match"), d]);
        }
        match (certificate.selected(), file.borrow().clone()) {
            (0, _) => a.extend([t("802-1x.system-ca-certs"), t("yes")]),
            (1, Some(f)) => a.extend([t("802-1x.ca-cert"), f]),
            (1, None) => {
                super::notify("Choose the certificate file first");
                return;
            }
            _ => {}
        }
        // replace an old connection with the same name (otherwise nmcli would make a second one with that name)
        let mut commands = Vec::new();
        let exists = command("nmcli", &["-t", "-e", "no", "-f", "NAME", "connection", "show"]).lines().any(|r| r == n);
        if exists {
            commands.push(vec![t("connection"), t("delete"), t("id"), n.clone()]);
        }
        commands.push(a);
        commands.push(vec![t("connection"), t("up"), t("id"), n.clone()]);
        super::notify(&format!("Connecting to {n}…"));
        in_background(
            move || nmcli_sequence(commands),
            |v| match v {
                Ok(()) => {
                    super::notify("Connected");
                    super::refresh_active();
                }
                Err(e) => super::notify(&format!("Couldn't connect: {e}")),
            },
        );
    });
    connect.set_halign(gtk::Align::End);
    s.append(&connect);
    let guide = button("Find your school's settings on cat.eduroam.org", false, || background("xdg-open", &["https://cat.eduroam.org"]));
    guide.set_halign(gtk::Align::Start);
    s.append(&guide);
    super::subpage("Wi-Fi", s.upcast());
}
