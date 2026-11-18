//! Stránka Network: kabel (stav, podrobnosti), VPN (připojit / odpojit, import souboru .ovpn
//! nebo WireGuard .conf, odebrat), sdílení internetu přes Wi-Fi (hotspot) a režim letadlo.

use gtk::prelude::*;

use super::ui::*;
use super::wifi_sit;
use crate::spolecne::{na_pozadi, prikaz};

struct Stav {
    /// (zařízení, stav, připojení)
    kabel: Vec<(String, String, String)>,
    /// (název, typ, aktivní)
    vpn: Vec<(String, String, bool)>,
    hotspot: bool,
    letadlo: bool,
}

fn nacist() -> Stav {
    let kabel = prikaz("nmcli", &["-t", "-e", "no", "-f", "DEVICE,TYPE,STATE,CONNECTION", "dev"])
        .lines()
        .filter_map(|r| {
            let c: Vec<&str> = r.splitn(4, ':').collect();
            (c.len() == 4 && c[1] == "ethernet").then(|| (c[0].to_string(), c[2].to_string(), c[3].to_string()))
        })
        .collect();
    let aktivni = prikaz("nmcli", &["-t", "-e", "no", "-f", "NAME", "connection", "show", "--active"]);
    let aktivni: Vec<&str> = aktivni.lines().collect();
    let mut hotspot = false;
    let vpn = prikaz("nmcli", &["-t", "-e", "no", "-f", "NAME,TYPE", "connection", "show"])
        .lines()
        .filter_map(|r| {
            let (n, t) = r.rsplit_once(':')?;
            if n == "Hotspot" && aktivni.contains(&n) {
                hotspot = true;
            }
            matches!(t, "vpn" | "wireguard").then(|| (n.to_string(), t.to_string(), aktivni.contains(&n)))
        })
        .collect();
    let letadlo = prikaz("nmcli", &["radio", "all"]).lines().nth(1).is_some_and(|r| r.split_whitespace().nth(1) == Some("disabled"));
    Stav { kabel, vpn, hotspot, letadlo }
}

/// nmcli na pozadí, pak potvrzení a nové sestavení stránky.
fn nmcli(args: Vec<String>, hotovo: &'static str) {
    na_pozadi(
        move || std::process::Command::new("nmcli").args(&args).output().map(|v| (v.status.success(), String::from_utf8_lossy(&v.stderr).lines().next().unwrap_or("").replace("Error: ", ""))).unwrap_or((false, String::new())),
        move |(ok, chyba)| {
            super::oznamit(&if ok { hotovo.to_string() } else { format!("Didn't work: {chyba}") });
            super::obnovit("sit");
        },
    );
}

fn t(s: &str) -> String {
    s.to_string()
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Network");
    let obal = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.append(&obal);
    let k = skupina(&obal, "", "");
    nacitani(&k, "Loading…");
    let o2 = obal.clone();
    na_pozadi(nacist, move |st| {
        if o2.root().is_none() {
            return;
        }
        while let Some(d) = o2.first_child() {
            o2.remove(&d);
        }
        ukazat(&o2, &st);
    });
    s.upcast()
}

fn ukazat(s: &gtk::Box, st: &Stav) {
    // letadlo
    let karta = skupina(s, "", "");
    radek_prepinac(&karta, "Airplane mode", "Turns off Wi-Fi and Bluetooth", st.letadlo, |z| {
        crate::spolecne::pozadi("rfkill", &[if z { "block" } else { "unblock" }, "bluetooth"]);
        nmcli(vec![t("radio"), t("all"), t(if z { "off" } else { "on" })], if z { "Airplane mode on" } else { "Airplane mode off" });
    });

    // kabel
    let karta = skupina(s, "Wired", "");
    if st.kabel.is_empty() {
        radek(&karta, "", "No network port", "", None);
    }
    for (zarizeni, stav, spojeni) in &st.kabel {
        match stav.as_str() {
            "connected" => {
                let sp = spojeni.clone();
                let podrobnosti = tlacitko("Details", false, move || wifi_sit::podrobnosti(&sp));
                radek(&karta, "\u{f6ff}", "Connected", spojeni, Some(podrobnosti.upcast_ref()));
            }
            "unavailable" => {
                radek(&karta, "\u{f6ff}", "Cable unplugged", zarizeni, None);
            }
            _ => {
                let z = zarizeni.clone();
                let pripojit = tlacitko("Connect", true, move || nmcli(vec![t("dev"), t("connect"), z.clone()], "Connected"));
                radek(&karta, "\u{f6ff}", "Not connected", zarizeni, Some(pripojit.upcast_ref()));
            }
        }
    }

    // VPN
    let karta = skupina(s, "VPN", "Your provider gives you a file (.ovpn for OpenVPN, .conf for WireGuard).");
    for (nazev, typ, aktivni) in &st.vpn {
        let n = nazev.clone();
        let prepinac = gtk::Switch::new();
        prepinac.set_active(*aktivni);
        prepinac.connect_state_set(move |_, z| {
            super::oznamit(if z { "Connecting…" } else { "Disconnecting…" });
            nmcli(vec![t("connection"), t(if z { "up" } else { "down" }), t("id"), n.clone()], if z { "VPN connected" } else { "VPN disconnected" });
            gtk::glib::Propagation::Proceed
        });
        let n = nazev.clone();
        let odebrat = tlacitko("Remove", false, move || nmcli(vec![t("connection"), t("delete"), t("id"), n.clone()], "VPN removed"));
        let vpravo = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        vpravo.append(&odebrat);
        vpravo.append(&prepinac);
        radek(&karta, "\u{f3ed}", nazev, if typ == "wireguard" { "WireGuard" } else { "VPN" }, Some(vpravo.upcast_ref()));
    }
    radek_akce(&karta, "\u{f067}", "Add a VPN from a file…", "", "", || {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("VPN file from your provider");
        dialog.open(super::okno().as_ref(), gtk::gio::Cancellable::NONE, |v| {
            let Some(p) = v.ok().and_then(|f| f.path()) else { return };
            let typ = if p.extension().is_some_and(|e| e == "conf") { "wireguard" } else { "openvpn" };
            nmcli(vec![t("connection"), t("import"), t("type"), t(typ), t("file"), p.to_string_lossy().to_string()], "VPN added – switch it on to connect");
        });
    });

    // hotspot
    let karta = skupina(s, "Share internet", "Lets a phone or another computer use this laptop's internet over Wi-Fi. While it's on, this laptop needs the internet over a cable (it has only one Wi-Fi card).");
    let nazev = gtk::Entry::new();
    nazev.set_text("Legion");
    nazev.set_width_chars(16);
    let heslo = gtk::PasswordEntry::new();
    heslo.set_show_peek_icon(true);
    heslo.set_width_chars(16);
    heslo.set_property("placeholder-text", "At least 8 characters");
    if !st.hotspot {
        radek(&karta, "", "Network name", "", Some(nazev.upcast_ref()));
        radek(&karta, "", "Password", "", Some(heslo.upcast_ref()));
    }
    radek_prepinac(&karta, "Hotspot", if st.hotspot { "On – others can join now" } else { "" }, st.hotspot, move |z| {
        if z {
            let h = heslo.text().to_string();
            if h.chars().count() < 8 {
                super::oznamit("The password needs at least 8 characters");
                super::obnovit("sit");
                return;
            }
            nmcli(vec![t("dev"), t("wifi"), t("hotspot"), t("ssid"), nazev.text().to_string(), t("password"), h], "Hotspot on");
        } else {
            nmcli(vec![t("connection"), t("down"), t("id"), t("Hotspot")], "Hotspot off");
        }
    });
}
