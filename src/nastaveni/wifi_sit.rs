//! Podstránky Wi-Fi: podrobnosti uložené sítě (stav, heslo, automatické připojení, měřené
//! připojení, ruční IP adresa a DNS, zapomenout) a připojení k podnikové síti (eduroam,
//! WPA2/3-Enterprise: PEAP / TTLS, jméno a heslo, kontrola certifikátu serveru).

use std::collections::HashMap;
use std::rc::Rc;

use gtk::prelude::*;

use super::ui::*;
use crate::spolecne::{na_pozadi, pozadi, prikaz};

/// Spustí nmcli s více příkazy po sobě; vrátí chybu prvního, který selže (první řádek).
fn nmcli_postupne(prikazy: Vec<Vec<String>>) -> Result<(), String> {
    for a in prikazy {
        let v = std::process::Command::new("nmcli").args(&a).output().map_err(|e| e.to_string())?;
        if !v.status.success() {
            let chyba = String::from_utf8_lossy(&v.stderr).lines().next().unwrap_or("").trim_start_matches("Error: ").to_string();
            return Err(chyba);
        }
    }
    Ok(())
}

fn t(x: &str) -> String {
    x.to_string()
}

/// Řádky „klíč:hodnota“ z `nmcli -t -f … connection show id X`.
fn vlastnosti(spojeni: &str, pole: &str) -> HashMap<String, String> {
    prikaz("nmcli", &["-t", "-f", pole, "connection", "show", "id", spojeni])
        .lines()
        .filter_map(|r| r.split_once(':').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

fn pole(placeholder: &str, text: &str) -> gtk::Entry {
    let e = gtk::Entry::new();
    e.set_placeholder_text(Some(placeholder));
    e.set_text(text);
    e.set_width_chars(24);
    e
}

fn vyber(volby: &[&str], vybrana: u32) -> gtk::DropDown {
    let d = gtk::DropDown::from_strings(volby);
    d.set_selected(vybrana);
    d
}

// ---------------------------------------------------------------- podrobnosti sítě

pub fn podrobnosti(spojeni: &str) {
    let s = stranka(spojeni);
    let karta = skupina(&s, "", "");
    nacitani(&karta, "Loading…");
    super::podstranka("Wi-Fi", s.clone().upcast());
    let sp = spojeni.to_string();
    na_pozadi(
        move || {
            let nastaveni = vlastnosti(&sp, "connection.autoconnect,connection.metered,ipv4.method,ipv4.addresses,ipv4.gateway,ipv4.dns,ipv4.ignore-auto-dns,802-11-wireless-security.key-mgmt,802-1x.identity,802-1x.eap");
            let aktivni = vlastnosti(&sp, "IP4,GENERAL.STATE");
            (sp, nastaveni, aktivni)
        },
        move |(sp, n, a)| {
            if s.root().is_none() {
                return;
            }
            // místo „Loading…“ skutečný obsah
            if let Some(rodic) = karta.parent() {
                s.remove(&rodic);
            }
            ukazat_podrobnosti(&s, &sp, &n, &a);
        },
    );
}

fn ukazat_podrobnosti(s: &gtk::Box, spojeni: &str, n: &HashMap<String, String>, a: &HashMap<String, String>) {
    let hodnota = |k: &str| n.get(k).cloned().unwrap_or_default();
    let pripojeno = a.get("GENERAL.STATE").is_some_and(|v| v == "activated");

    // stav
    let karta = skupina(s, "", "");
    if pripojeno {
        let ip = a.get("IP4.ADDRESS[1]").cloned().unwrap_or_default();
        let dns: Vec<String> = (1..5).filter_map(|i| a.get(&format!("IP4.DNS[{i}]")).cloned()).collect();
        let sp = spojeni.to_string();
        let odpojit = tlacitko("Disconnect", false, move || {
            super::oznamit("Disconnecting…");
            let sp = sp.clone();
            na_pozadi(move || nmcli_postupne(vec![vec![t("connection"), t("down"), t("id"), sp]]), |_| super::obnovit_aktivni());
        });
        radek(&karta, "\u{f1eb}", "Connected", "", Some(odpojit.upcast_ref()));
        radek(&karta, "", "IP address", "", Some(popisek_vpravo(ip.split('/').next().unwrap_or("")).upcast_ref()));
        radek(&karta, "", "Router", "", Some(popisek_vpravo(&a.get("IP4.GATEWAY").cloned().unwrap_or_default()).upcast_ref()));
        radek(&karta, "", "DNS", "", Some(popisek_vpravo(&dns.join(", ")).upcast_ref()));
    } else {
        let sp = spojeni.to_string();
        let pripojit = tlacitko("Connect", true, move || {
            super::oznamit("Connecting…");
            let sp = sp.clone();
            na_pozadi(
                move || nmcli_postupne(vec![vec![t("connection"), t("up"), t("id"), sp]]),
                |v| {
                    super::oznamit(if v.is_ok() { "Connected" } else { "Couldn't connect – is the network in range?" });
                    super::obnovit_aktivni();
                },
            );
        });
        radek(&karta, "\u{f1eb}", "Not connected", "", Some(pripojit.upcast_ref()));
    }
    let zabezpeceni = match hodnota("802-11-wireless-security.key-mgmt").as_str() {
        "wpa-psk" => "WPA2 password",
        "sae" => "WPA3 password",
        "wpa-eap" | "wpa-eap-suite-b-192" => "Username and password (Enterprise)",
        "none" | "" => "Open, not secured",
        x => x,
    }
    .to_string();
    if hodnota("802-11-wireless-security.key-mgmt").starts_with("wpa-eap") {
        radek(&karta, "\u{f023}", "Security", &zabezpeceni, None);
        radek(&karta, "\u{f007}", "Username", &hodnota("802-1x.identity"), None);
    } else if !matches!(hodnota("802-11-wireless-security.key-mgmt").as_str(), "none" | "") {
        // heslo se načte až na požádání (z NetworkManageru, může se zeptat na heslo účtu)
        let heslo = gtk::Label::new(Some("••••••••"));
        heslo.add_css_class("popis");
        heslo.set_selectable(true);
        let obal = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        obal.append(&heslo);
        let (h, sp) = (heslo.clone(), spojeni.to_string());
        let ukazat = tlacitko("Show", false, || {});
        let u2 = ukazat.clone();
        ukazat.connect_clicked(move |_| {
            let (h, u, sp) = (h.clone(), u2.clone(), sp.clone());
            na_pozadi(
                move || prikaz("nmcli", &["-s", "-g", "802-11-wireless-security.psk", "connection", "show", "id", &sp]),
                move |heslo| {
                    if heslo.trim().is_empty() {
                        super::oznamit("Couldn't read the password");
                    } else {
                        h.set_text(heslo.trim());
                        u.set_visible(false);
                    }
                },
            );
        });
        obal.append(&ukazat);
        radek(&karta, "\u{f084}", "Password", &zabezpeceni, Some(obal.upcast_ref()));
    } else {
        radek(&karta, "\u{f13e}", "Security", &zabezpeceni, None);
    }

    // chování
    let karta = skupina(s, "", "");
    let sp = spojeni.to_string();
    radek_prepinac(&karta, "Connect automatically", "Join this network whenever it is in range", hodnota("connection.autoconnect") == "yes", move |v| {
        upravit(&sp, vec![t("connection.autoconnect"), t(if v { "yes" } else { "no" })], false);
    });
    let sp = spojeni.to_string();
    radek_prepinac(
        &karta,
        "Metered connection",
        "For a phone hotspot or limited data: updates and other background downloads wait",
        hodnota("connection.metered") == "yes",
        move |v| upravit(&sp, vec![t("connection.metered"), t(if v { "yes" } else { "no" })], false),
    );

    // IP adresa a DNS
    let karta = skupina(s, "IP settings", "Leave on Automatic unless the network needs a fixed address.");
    let rucne = hodnota("ipv4.method") == "manual";
    let metoda = vyber(&["Automatic (DHCP)", "Manual"], rucne as u32);
    radek(&karta, "", "IPv4 address", "", Some(metoda.upcast_ref()));
    let adresa = pole("192.168.0.50/24", &hodnota("ipv4.addresses"));
    let brana = pole("192.168.0.1", &hodnota("ipv4.gateway"));
    let r_adresa = radek(&karta, "", "Address", "With /24 at the end for a normal home network", Some(adresa.upcast_ref()));
    let r_brana = radek(&karta, "", "Router (gateway)", "", Some(brana.upcast_ref()));
    let vlastni_dns = !hodnota("ipv4.dns").is_empty();
    let dns_vyber = vyber(&["Automatic", "Custom"], vlastni_dns as u32);
    radek(&karta, "", "DNS", "Custom DNS, e.g. 1.1.1.1 (Cloudflare) or 9.9.9.9 (Quad9)", Some(dns_vyber.upcast_ref()));
    let dns = pole("1.1.1.1, 1.0.0.1", &hodnota("ipv4.dns"));
    let r_dns = radek(&karta, "", "DNS servers", "Separated by commas", Some(dns.upcast_ref()));
    let viditelnost = {
        let (m, d, ra, rb, rd) = (metoda.clone(), dns_vyber.clone(), r_adresa.clone(), r_brana.clone(), r_dns.clone());
        move || {
            let rucne = m.selected() == 1;
            ra.set_visible(rucne);
            rb.set_visible(rucne);
            rd.set_visible(d.selected() == 1);
        }
    };
    viditelnost();
    let v = Rc::new(viditelnost);
    let v2 = v.clone();
    metoda.connect_selected_notify(move |_| v2());
    dns_vyber.connect_selected_notify(move |_| v());
    let sp = spojeni.to_string();
    let ulozit = tlacitko("Save", true, move || {
        let mut z: Vec<String> = Vec::new();
        if metoda.selected() == 1 {
            let mut a = adresa.text().trim().to_string();
            if a.is_empty() {
                super::oznamit("Fill in the address");
                return;
            }
            if !a.contains('/') {
                a.push_str("/24");
            }
            z.extend([t("ipv4.method"), t("manual"), t("ipv4.addresses"), a, t("ipv4.gateway"), brana.text().trim().to_string()]);
        } else {
            z.extend([t("ipv4.method"), t("auto"), t("ipv4.addresses"), t(""), t("ipv4.gateway"), t("")]);
        }
        if dns_vyber.selected() == 1 && !dns.text().trim().is_empty() {
            z.extend([t("ipv4.dns"), dns.text().replace(' ', ""), t("ipv4.ignore-auto-dns"), t("yes")]);
        } else {
            z.extend([t("ipv4.dns"), t(""), t("ipv4.ignore-auto-dns"), t("no")]);
        }
        upravit(&sp, z, pripojeno);
    });
    ulozit.set_halign(gtk::Align::End);
    s.append(&ulozit);

    // zapomenout
    let karta = skupina(s, "", "");
    let sp = spojeni.to_string();
    let zapomenout = tlacitko("Forget", false, move || {
        let sp = sp.clone();
        na_pozadi(
            move || nmcli_postupne(vec![vec![t("connection"), t("delete"), t("id"), sp]]),
            |v| {
                super::oznamit(if v.is_ok() { "Network forgotten" } else { "Couldn't forget the network" });
                super::obnovit_aktivni();
            },
        );
    });
    radek(&karta, "\u{f1f8}", "Forget this network", "Removes the saved password; you'll need it to connect again", Some(zapomenout.upcast_ref()));
}

fn popisek_vpravo(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(if text.is_empty() { "–" } else { text }));
    l.add_css_class("popis");
    l.set_selectable(true);
    l
}

/// `nmcli connection modify` (a u připojené sítě znovu připojit, ať se změna projeví).
fn upravit(spojeni: &str, zmeny: Vec<String>, znovu_pripojit: bool) {
    let mut prikazy = vec![[vec![t("connection"), t("modify"), t("id"), spojeni.to_string()], zmeny].concat()];
    if znovu_pripojit {
        prikazy.push(vec![t("connection"), t("up"), t("id"), spojeni.to_string()]);
    }
    let sp = spojeni.to_string();
    na_pozadi(
        move || nmcli_postupne(prikazy),
        move |v| match v {
            Ok(()) => {
                super::oznamit(if znovu_pripojit { "Saved and reconnected" } else { "Saved" });
                if znovu_pripojit {
                    podrobnosti(&sp);
                }
            }
            Err(e) => super::oznamit(&format!("Not saved: {e}")),
        },
    );
}

// ---------------------------------------------------------------- podniková síť (eduroam)

pub fn podnikova(ssid: &str) {
    let ssid = if ssid.is_empty() { "eduroam" } else { ssid };
    let s = stranka("Connect with a username");
    let karta = skupina(
        &s,
        "",
        "Schools and companies use these networks. If something below is unclear, your school's IT page (or cat.eduroam.org) lists the right settings.",
    );
    let nazev = pole("Network name", ssid);
    radek(&karta, "\u{f1eb}", "Network", "", Some(nazev.upcast_ref()));
    let jmeno = pole("you@school.edu", "");
    radek(&karta, "\u{f007}", "Username", "For eduroam your full school email", Some(jmeno.upcast_ref()));
    let heslo = gtk::PasswordEntry::new();
    heslo.set_show_peek_icon(true);
    heslo.set_width_chars(24);
    radek(&karta, "\u{f084}", "Password", "", Some(heslo.upcast_ref()));

    let karta = skupina(&s, "Advanced", "The defaults work for most eduroam networks.");
    let eap = vyber(&["PEAP", "TTLS"], 0);
    radek(&karta, "", "Method", "", Some(eap.upcast_ref()));
    let vnitrni = vyber(&["MSCHAPv2", "PAP", "GTC"], 0);
    radek(&karta, "", "Inner authentication", "", Some(vnitrni.upcast_ref()));
    let anonymni = pole("anonymous@school.edu", "");
    radek(&karta, "", "Anonymous identity", "Optional; filled in from your username if empty", Some(anonymni.upcast_ref()));
    let domena = pole("e.g. radius.school.edu", "");
    radek(&karta, "", "Server domain", "Optional; makes sure you talk to your school's server", Some(domena.upcast_ref()));
    let certifikat = vyber(&["System certificates", "Certificate file…", "Don't check (not secure)"], 0);
    radek(&karta, "\u{f132}", "Server certificate", "", Some(certifikat.upcast_ref()));
    let soubor: Rc<std::cell::RefCell<Option<String>>> = Rc::new(std::cell::RefCell::new(None));
    let (soubor2, cert2) = (soubor.clone(), certifikat.clone());
    certifikat.connect_selected_notify(move |d| {
        if d.selected() != 1 {
            return;
        }
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Certificate from your school or company");
        let (soubor, cert) = (soubor2.clone(), cert2.clone());
        let okno = d.root().and_downcast::<gtk::Window>();
        dialog.open(okno.as_ref(), gtk::gio::Cancellable::NONE, move |v| match v.ok().and_then(|f| f.path()) {
            Some(p) => {
                super::oznamit(&format!("Certificate: {}", p.file_name().unwrap_or_default().to_string_lossy()));
                soubor.replace(Some(p.to_string_lossy().to_string()));
            }
            None => cert.set_selected(0),
        });
    });

    let pripojit = tlacitko("Connect", true, move || {
        let (n, j, h) = (nazev.text().trim().to_string(), jmeno.text().trim().to_string(), heslo.text().to_string());
        if n.is_empty() || j.is_empty() || h.is_empty() {
            super::oznamit("Fill in the network, username and password");
            return;
        }
        let mut anon = anonymni.text().trim().to_string();
        if anon.is_empty() {
            if let Some((_, realm)) = j.split_once('@') {
                anon = format!("anonymous@{realm}");
            }
        }
        let mut a = vec![
            t("connection"), t("add"), t("type"), t("wifi"), t("con-name"), n.clone(), t("ssid"), n.clone(),
            t("wifi-sec.key-mgmt"), t("wpa-eap"),
            t("802-1x.eap"), t(["peap", "ttls"][eap.selected() as usize]),
            t("802-1x.phase2-auth"), t(["mschapv2", "pap", "gtc"][vnitrni.selected() as usize]),
            t("802-1x.identity"), j, t("802-1x.password"), h,
        ];
        let zarizeni = prikaz("nmcli", &["-t", "-f", "DEVICE,TYPE", "dev"]).lines().find_map(|r| r.strip_suffix(":wifi").map(String::from));
        if let Some(z) = zarizeni {
            a.extend([t("ifname"), z]);
        }
        if !anon.is_empty() {
            a.extend([t("802-1x.anonymous-identity"), anon]);
        }
        let d = domena.text().trim().to_string();
        if !d.is_empty() {
            a.extend([t("802-1x.domain-suffix-match"), d]);
        }
        match (certifikat.selected(), soubor.borrow().clone()) {
            (0, _) => a.extend([t("802-1x.system-ca-certs"), t("yes")]),
            (1, Some(f)) => a.extend([t("802-1x.ca-cert"), f]),
            (1, None) => {
                super::oznamit("Choose the certificate file first");
                return;
            }
            _ => {}
        }
        // stejně pojmenované staré připojení nahradit (jinak by nmcli udělal druhé se stejným jménem)
        let mut prikazy = Vec::new();
        let existuje = prikaz("nmcli", &["-t", "-e", "no", "-f", "NAME", "connection", "show"]).lines().any(|r| r == n);
        if existuje {
            prikazy.push(vec![t("connection"), t("delete"), t("id"), n.clone()]);
        }
        prikazy.push(a);
        prikazy.push(vec![t("connection"), t("up"), t("id"), n.clone()]);
        super::oznamit(&format!("Connecting to {n}…"));
        na_pozadi(
            move || nmcli_postupne(prikazy),
            |v| match v {
                Ok(()) => {
                    super::oznamit("Connected");
                    super::obnovit_aktivni();
                }
                Err(e) => super::oznamit(&format!("Couldn't connect: {e}")),
            },
        );
    });
    pripojit.set_halign(gtk::Align::End);
    s.append(&pripojit);
    let navod = tlacitko("Find your school's settings on cat.eduroam.org", false, || pozadi("xdg-open", &["https://cat.eduroam.org"]));
    navod.set_halign(gtk::Align::Start);
    s.append(&navod);
    super::podstranka("Wi-Fi", s.upcast());
}
