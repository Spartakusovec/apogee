//! Bluetooth: zapnutí a připojení / odpojení spárovaných zařízení (u sluchátek i jejich baterka).

use std::rc::Rc;

use gtk::prelude::*;

use crate::spolecne::*;

#[derive(Clone)]
struct Zarizeni {
    mac: String,
    jmeno: String,
    pripojeno: bool,
    baterie: String,
    ikona: &'static str,
}

fn nacist() -> Vec<Zarizeni> {
    let mut vysledek = Vec::new();
    for radek in prikaz("bluetoothctl", &["devices", "Paired"]).lines() {
        let casti: Vec<&str> = radek.splitn(3, ' ').collect();
        if casti.len() < 3 {
            continue;
        }
        let info = prikaz("bluetoothctl", &["info", casti[1]]);
        let hodnota = |klic: &str| info.lines().find_map(|r| r.trim().strip_prefix(klic).map(|v| v.trim().to_string()));
        // "Battery Percentage: 0x50 (80)" → 80
        let baterie = hodnota("Battery Percentage:")
            .and_then(|v| v.split('(').nth(1).map(|x| x.trim_end_matches(')').to_string()))
            .unwrap_or_default();
        let ikona = match hodnota("Icon:").as_deref() {
            Some("audio-headset") | Some("audio-headphones") => "\u{f025}",
            Some("audio-card") => "\u{f028}",
            Some("input-keyboard") => "\u{f11c}",
            Some("input-mouse") => "\u{f8cc}",
            Some("phone") => "\u{f3cd}",
            Some("computer") => "\u{f109}",
            _ => "\u{f293}",
        };
        vysledek.push(Zarizeni {
            mac: casti[1].to_string(),
            jmeno: casti[2].to_string(),
            pripojeno: info.contains("Connected: yes"),
            baterie,
            ikona,
        });
    }
    vysledek.sort_by_key(|z| !z.pripojeno);
    vysledek
}

struct Bluetooth {
    o: Rc<Okenko>,
    seznam: gtk::Box,
}

pub fn naplnit(o: &Rc<Okenko>) {
    let zapnuto = bluetooth_zapnuty();
    let seznam = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let b = Rc::new(Bluetooth { o: o.clone(), seznam: seznam.clone() });
    let b2 = b.clone();
    o.panel.append(&hlavicka_s_prepinacem("Bluetooth", zapnuto, move |stav| {
        pozadi("bluetoothctl", &["power", if stav { "on" } else { "off" }]);
        b2.seznam.set_visible(stav);
    }));
    o.panel.append(&seznam);
    seznam.set_visible(zapnuto);
    o.panel.append(&oddelovac());
    o.panel.append(&odkaz_nastaveni(o, "Bluetooth settings…", "bluetooth"));
    b.nacist();
}

impl Bluetooth {
    fn nacist(self: &Rc<Self>) {
        let b = self.clone();
        na_pozadi(nacist, move |zarizeni| b.ukazat(zarizeni));
    }

    fn ukazat(self: &Rc<Self>, zarizeni: Vec<Zarizeni>) {
        // bluetoothctl je pomalý, okénko mezitím mohlo zavřít
        if self.o.zavreno() {
            return;
        }
        while let Some(d) = self.seznam.first_child() {
            self.seznam.remove(&d);
        }
        if zarizeni.is_empty() {
            self.seznam.append(&popisek("No paired devices", &["dim"]));
        }
        for z in zarizeni {
            let vpravo = format!(
                "{}{}",
                if z.baterie.is_empty() { String::new() } else { format!("{} %  ", z.baterie) },
                if z.pripojeno { "Connected" } else { "" }
            );
            let b = self.clone();
            let z2 = z.clone();
            self.seznam.append(&radek(z.ikona, &z.jmeno, &vpravo, z.pripojeno, move || b.prepnout(&z2)));
        }
        self.o.prepocitat();
    }

    fn prepnout(self: &Rc<Self>, z: &Zarizeni) {
        self.o.stav(&format!("{} {}…", if z.pripojeno { "Disconnecting" } else { "Connecting" }, z.jmeno));
        let (mac, pripojeno, jmeno) = (z.mac.clone(), z.pripojeno, z.jmeno.clone());
        let b = self.clone();
        na_pozadi(
            move || {
                prikaz("timeout", &["25", "bluetoothctl", if pripojeno { "disconnect" } else { "connect" }, &mac])
                    .to_lowercase()
                    .contains("successful")
            },
            move |ok| {
                b.o.stav(&if ok { "Done".to_string() } else { format!("Couldn't connect {jmeno} – is it on and nearby?") });
                b.nacist();
            },
        );
    }
}
