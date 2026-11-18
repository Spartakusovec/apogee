//! Stránka Updates: aplikace a systém (dnf + Flatpak, stejná data jako okénko v panelu),
//! upravený Sway (sway-patch: jestli má nainstalovaná verze aktuální úpravu, znovu sestavit)
//! a firmware (fwupd: zařízení a dostupné aktualizace, instalace přes polkit).

use gtk::prelude::*;
use serde_json::Value;

use super::ui::*;
use crate::spolecne::{na_pozadi, prikaz};

fn kdy(cas: u64) -> String {
    let ted = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    match ted.saturating_sub(cas) {
        0 => "never".into(),
        s if s < 120 => "just now".into(),
        s if s < 7200 => format!("{} minutes ago", s / 60),
        s if s < 172800 => format!("{} hours ago", s / 3600),
        s => format!("{} days ago", s / 86400),
    }
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Updates");

    // aplikace a systém
    let v = crate::aktualizace::nacist();
    let dnf = v["dnf"].as_array().cloned().unwrap_or_default();
    let flatpak = v["flatpak"].as_array().cloned().unwrap_or_default();
    let karta = skupina(&s, "Apps and system", "Checked every 6 hours by itself.");
    let pocet = dnf.len() + flatpak.len();
    let zkontrolovat = tlacitko("Check now", false, || {
        super::oznamit("Checking for updates…");
        crate::aktualizace::zkontrolovat(|_| {
            super::oznamit("Checked");
            super::obnovit("aktualizace");
        });
    });
    zkontrolovat.set_sensitive(!crate::aktualizace::kontroluje());
    let stav = if pocet == 0 { "Everything is up to date".to_string() } else { format!("{pocet} updates ready") };
    radek(&karta, "\u{f019}", &stav, &format!("Last checked {}", kdy(v["cas"].as_u64().unwrap_or(0))), Some(zkontrolovat.upcast_ref()));
    if pocet > 0 {
        let seznam: Vec<String> = dnf.iter().chain(flatpak.iter()).filter_map(|x| x[0].as_str().map(String::from)).collect();
        let ukazka = if seznam.len() > 12 { format!("{} and {} more", seznam[..12].join(", "), seznam.len() - 12) } else { seznam.join(", ") };
        radek(&karta, "", &format!("{} system, {} Flatpak", dnf.len(), flatpak.len()), &ukazka, None);
        radek(&karta, "\u{f120}", "Install updates", "Opens a window that shows the progress and asks for your password", Some(tlacitko("Install", true, crate::aktualizace::nainstalovat).upcast_ref()));
    }

    // sway-krizek
    let karta = skupina(&s, "Sway with title bar buttons", "Rebuilt by itself after every Sway update from Fedora.");
    // nové umístění (sway-patch), dokud neproběhne install.sh, ještě staré (sway-krizek)
    let stav = std::fs::read_to_string("/var/lib/sway-patch/state").or_else(|_| std::fs::read_to_string("/var/lib/sway-krizek/stav")).unwrap_or_default();
    let mut casti = stav.split_whitespace();
    let (verze, otisk) = (casti.next().unwrap_or("").to_string(), casti.next().unwrap_or("").to_string());
    let uprava = ["/usr/local/share/sway-patch/sway-buttons.patch", "/usr/local/share/sway-krizek/sway-krizek.patch"]
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|p| prikaz("sha256sum", &[p]).chars().take(16).collect::<String>())
        .unwrap_or_default();
    let nainstalovana = prikaz("rpm", &["-q", "--qf", "%{VERSION}-%{RELEASE}", "sway"]);
    let aktualni = !verze.is_empty() && verze == nainstalovana && otisk == uprava;
    let popis = if aktualni {
        format!("Sway {nainstalovana} has the latest buttons patch")
    } else if verze.is_empty() {
        "Not built yet".to_string()
    } else {
        format!("Sway {nainstalovana} needs to be rebuilt with the latest patch")
    };
    let sestavit = tlacitko("Rebuild", !aktualni, || {
        super::oznamit("Building Sway – takes a few minutes, a notification says when it's done");
        na_pozadi(
            || std::process::Command::new("pkexec").arg(if std::path::Path::new("/usr/local/libexec/sway-patch").exists() { "/usr/local/libexec/sway-patch" } else { "/usr/local/libexec/sway-krizek" }).output().map(|v| v.status.success()).unwrap_or(false),
            |ok| {
                super::oznamit(if ok { "Sway rebuilt – log out and back in to use it" } else { "Rebuild failed or was cancelled" });
                super::obnovit("aktualizace");
            },
        );
    });
    radek(&karta, if aktualni { "\u{f00c}" } else { "\u{f071}" }, if aktualni { "Up to date" } else { "Rebuild needed" }, &popis, Some(sestavit.upcast_ref()));

    // firmware
    let karta = skupina(&s, "Firmware", "Updates for the BIOS, disk and other parts, from the Linux Vendor Firmware Service. Needs the charger plugged in.");
    nacitani(&karta, "Looking for firmware updates…");
    let k2 = karta.clone();
    na_pozadi(
        || {
            let zarizeni = prikaz("fwupdmgr", &["get-devices", "--json"]);
            let aktualizace = prikaz("fwupdmgr", &["get-updates", "--json"]);
            (serde_json::from_str::<Value>(&zarizeni).unwrap_or_default(), serde_json::from_str::<Value>(&aktualizace).unwrap_or_default())
        },
        move |(zarizeni, aktualizace)| {
            if k2.root().is_none() {
                return;
            }
            vycistit(&k2);
            let aktualizace = aktualizace["Devices"].as_array().cloned().unwrap_or_default();
            for z in aktualizace.iter() {
                let nazev = z["Name"].as_str().unwrap_or("Device").to_string();
                let nova = z["Releases"][0]["Version"].as_str().unwrap_or("").to_string();
                let id = z["DeviceId"].as_str().unwrap_or("").to_string();
                let instalovat = tlacitko("Update", false, move || {
                    super::oznamit("Updating firmware – keep the charger plugged in…");
                    let id = id.clone();
                    na_pozadi(
                        move || std::process::Command::new("fwupdmgr").args(["update", &id, "-y", "--no-reboot-check"]).output().map(|v| v.status.success()).unwrap_or(false),
                        |ok| super::oznamit(if ok { "Firmware updated – restart to finish" } else { "Firmware update failed" }),
                    );
                });
                radek(&k2, "\u{f2db}", &nazev, &format!("{} → {nova}", z["Version"].as_str().unwrap_or("")), Some(instalovat.upcast_ref()));
            }
            if aktualizace.is_empty() {
                radek(&k2, "\u{f00c}", "No firmware updates", "", None);
            }
            // aktualizovatelná zařízení (bez duplicit)
            let mut nazvy: Vec<String> = zarizeni["Devices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["Flags"].as_array().is_some_and(|f| f.iter().any(|x| x == "updatable")))
                .filter_map(|d| Some(format!("{} {}", d["Name"].as_str()?, d["Version"].as_str().unwrap_or(""))))
                .collect();
            nazvy.dedup();
            if !nazvy.is_empty() {
                radek(&k2, "", "Devices that can get updates", &nazvy.join("\n"), None);
            }
        },
    );
    s.upcast()
}
