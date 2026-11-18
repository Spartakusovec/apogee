//! Stránka Bluetooth: zapnutí, spárovaná zařízení (připojit, odpojit, zapomenout, baterie
//! zařízení) a hledání a spárování nových. Vše přes bluetoothctl na pozadí.

use std::rc::Rc;

use gtk::prelude::*;

use super::ui::*;
use crate::spolecne::{bluetooth_zapnuty, na_pozadi, prikaz};

#[derive(Clone)]
struct Zarizeni {
    mac: String,
    jmeno: String,
    pripojeno: bool,
    baterie: String,
    ikona: &'static str,
    druh: &'static str,
}

fn info(mac: &str, jmeno: &str) -> Zarizeni {
    let info = prikaz("bluetoothctl", &["info", mac]);
    let hodnota = |klic: &str| info.lines().find_map(|r| r.trim().strip_prefix(klic).map(|v| v.trim().to_string()));
    // "Battery Percentage: 0x50 (80)" → 80
    let baterie = hodnota("Battery Percentage:").and_then(|v| v.split('(').nth(1).map(|x| x.trim_end_matches(')').to_string())).unwrap_or_default();
    let (ikona, druh) = match hodnota("Icon:").as_deref() {
        Some("audio-headset") | Some("audio-headphones") => ("\u{f025}", "Headphones"),
        Some("audio-card") => ("\u{f028}", "Speaker"),
        Some("input-keyboard") => ("\u{f11c}", "Keyboard"),
        Some("input-mouse") => ("\u{f8cc}", "Mouse"),
        Some("input-gaming") => ("\u{f11b}", "Game controller"),
        Some("phone") => ("\u{f3cd}", "Phone"),
        Some("computer") => ("\u{f109}", "Computer"),
        _ => ("\u{f293}", "Device"),
    };
    Zarizeni { mac: mac.into(), jmeno: jmeno.into(), pripojeno: info.contains("Connected: yes"), baterie, ikona, druh }
}

/// Zařízení ze seznamu bluetoothctl ("Device AA:BB:… Jméno").
fn seznam(args: &[&str]) -> Vec<(String, String)> {
    prikaz("bluetoothctl", args)
        .lines()
        .filter_map(|r| {
            let c: Vec<&str> = r.splitn(3, ' ').collect();
            (c.len() == 3 && c[0] == "Device").then(|| (c[1].to_string(), c[2].to_string()))
        })
        .collect()
}

fn sparovana() -> Vec<Zarizeni> {
    let mut v: Vec<Zarizeni> = seznam(&["devices", "Paired"]).iter().map(|(m, j)| info(m, j)).collect();
    v.sort_by_key(|z| (!z.pripojeno, z.jmeno.to_lowercase()));
    v
}

/// bluetoothctl na pozadí, úspěch podle výpisu; pak potvrzení a nové sestavení stránky.
fn bt(args: Vec<String>, hotovo: String, chyba: String) {
    na_pozadi(
        move || {
            let mut a = vec!["30".to_string(), "bluetoothctl".to_string()];
            a.extend(args);
            let a: Vec<&str> = a.iter().map(String::as_str).collect();
            let v = prikaz("timeout", &a).to_lowercase();
            v.contains("successful") || v.contains("succeeded")
        },
        move |ok| {
            super::oznamit(if ok { &hotovo } else { &chyba });
            super::obnovit("bluetooth");
        },
    );
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Bluetooth");
    let zapnuto = bluetooth_zapnuty();
    let hlavni = skupina(&s, "", "");
    radek_prepinac(&hlavni, "Bluetooth", "Headphones, speakers, mice and keyboards", zapnuto, |stav| {
        crate::spolecne::pozadi("bluetoothctl", &["power", if stav { "on" } else { "off" }]);
        super::oznamit(if stav { "Bluetooth on" } else { "Bluetooth off" });
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(800), || super::obnovit("bluetooth"));
    });
    if !zapnuto {
        return s.upcast();
    }
    let moje = skupina(&s, "My devices", "");
    nacitani(&moje, "Loading devices…");
    let nove = skupina(&s, "Add a device", "Put the device into pairing mode (usually by holding its power or Bluetooth button), then look for it.");
    let nove2 = nove.clone();
    radek_akce(&nove, "\u{f002}", "Look for devices", "Takes about 15 seconds", "", move || hledat(&nove2));
    radek_akce(&nove, "\u{f013}", "More Bluetooth options", "Blueman: sending files, devices that need a PIN", "", || {
        crate::spolecne::pozadi("blueman-manager", &[]);
    });

    let moje = Rc::new(moje);
    na_pozadi(sparovana, move |zarizeni| {
        if moje.root().is_none() {
            return;
        }
        vycistit(&moje);
        if zarizeni.is_empty() {
            radek(&moje, "", "No paired devices yet", "Add one below", None);
        }
        for z in zarizeni {
            let tlacitka = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let z2 = z.clone();
            tlacitka.append(&tlacitko(if z.pripojeno { "Disconnect" } else { "Connect" }, !z.pripojeno, move || {
                super::oznamit(&format!("{} {}…", if z2.pripojeno { "Disconnecting" } else { "Connecting" }, z2.jmeno));
                bt(
                    vec![if z2.pripojeno { "disconnect" } else { "connect" }.into(), z2.mac.clone()],
                    if z2.pripojeno { format!("{} disconnected", z2.jmeno) } else { format!("{} connected", z2.jmeno) },
                    format!("Couldn't reach {} – is it on and nearby?", z2.jmeno),
                );
            }));
            let z2 = z.clone();
            tlacitka.append(&tlacitko("Forget", false, move || {
                bt(vec!["remove".into(), z2.mac.clone()], format!("{} forgotten", z2.jmeno), "Couldn't forget the device".into());
            }));
            let mut popis = vec![z.druh.to_string(), if z.pripojeno { "Connected".into() } else { "Not connected".into() }];
            if !z.baterie.is_empty() {
                popis.push(format!("Battery {} %", z.baterie));
            }
            radek(&moje, z.ikona, &z.jmeno, &popis.join("  ·  "), Some(tlacitka.upcast_ref()));
        }
    });
    s.upcast()
}

/// Hledání nových zařízení (15 s), pak seznam nalezených nespárovaných; klik spáruje a připojí.
fn hledat(karta: &gtk::ListBox) {
    super::oznamit("Looking for devices…");
    let karta = karta.clone();
    na_pozadi(
        || {
            prikaz("bluetoothctl", &["--timeout", "15", "scan", "on"]);
            let sparovana: Vec<String> = seznam(&["devices", "Paired"]).into_iter().map(|(m, _)| m).collect();
            seznam(&["devices"])
                .into_iter()
                // jen zařízení s opravdovým jménem (jinak bluetoothctl ukáže adresu)
                .filter(|(m, j)| !sparovana.contains(m) && j.replace('-', ":") != *m)
                .map(|(m, j)| info(&m, &j))
                .collect::<Vec<_>>()
        },
        move |nalezena| {
            if karta.root().is_none() {
                return;
            }
            // staré výsledky pryč (první dva řádky jsou tlačítka)
            while let Some(r) = karta.row_at_index(2) {
                karta.remove(&r);
            }
            if nalezena.is_empty() {
                radek(&karta, "", "No new devices found", "Is the device in pairing mode?", None);
                return;
            }
            for z in nalezena {
                let z2 = z.clone();
                radek_akce(&karta, z.ikona, &z.jmeno, z.druh, "Pair", move || {
                    super::oznamit(&format!("Pairing {}…", z2.jmeno));
                    let (mac, jmeno) = (z2.mac.clone(), z2.jmeno.clone());
                    na_pozadi(
                        move || {
                            let t = |a: &[&str]| {
                                let mut v = vec!["30", "bluetoothctl"];
                                v.extend_from_slice(a);
                                prikaz("timeout", &v).to_lowercase()
                            };
                            let ok = t(&["pair", &mac]).contains("successful");
                            if ok {
                                t(&["trust", &mac]);
                                t(&["connect", &mac]);
                            }
                            ok
                        },
                        move |ok| {
                            super::oznamit(&if ok { format!("{jmeno} paired") } else { format!("Couldn't pair {jmeno} – try More Bluetooth options") });
                            super::obnovit("bluetooth");
                        },
                    );
                });
            }
        },
    );
}
