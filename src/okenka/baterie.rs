//! Baterie: okénko (nabití, kolik zbývá, odběr, zdraví baterie a režim napájení přes
//! power-mode.sh / tuned) a hlídání na pozadí.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::prelude::*;

use crate::spolecne::*;

/// Hlídání baterie (kontrola jednou za minutu):
/// - upozorní na 15 % a 5 %, jednou za každé vybíjení
/// - na 15 % zapne úsporný režim a po připojení nabíječky vrátí ten předchozí
/// Mírnější úsporu na baterii dělá sám tuned (balanced → balanced-battery).
pub fn hlidat() {
    let upozorneno = Cell::new(100);
    let predtim: RefCell<Option<String>> = RefCell::new(None); // režim před automatickým úsporným
    let napajeni = skripty().join("power-mode.sh").to_string_lossy().to_string();
    gtk::glib::timeout_add_seconds_local(60, move || {
        let Some(b) = baterie() else { return gtk::glib::ControlFlow::Continue };
        let kap = b.procenta.round() as i32;
        if b.stav != 2 {
            upozorneno.set(100);
            if let Some(rezim) = predtim.take() {
                pozadi(&napajeni, &[&rezim]);
            }
        } else if kap <= 5 && upozorneno.get() > 5 {
            pozadi("notify-send", &["-u", "critical", "-i", "battery-empty", &format!("Battery almost empty: {kap} %"), "Plug in the charger, the laptop will shut down soon."]);
            upozorneno.set(5);
        } else if kap <= 15 && upozorneno.get() > 15 {
            pozadi("notify-send", &["-i", "battery-low", &format!("Low battery: {kap} %"), "Power saver is on until you plug in the charger."]);
            upozorneno.set(15);
            let ted = rezim_napajeni();
            if ted != "power-saver" {
                predtim.replace(Some(ted));
                pozadi(&napajeni, &["power-saver"]);
            }
        }
        gtk::glib::ControlFlow::Continue
    });
}

pub fn naplnit(o: &Rc<Okenko>) {
    let b = baterie().unwrap_or(Baterie { procenta: 0.0, stav: 0, do_vybiti: 0, do_nabiti: 0, prikon: 0.0, zdravi: 0.0 });
    let procenta = b.procenta.round();

    let hlavni = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    hlavni.append(&popisek(&format!("{procenta} %"), &["velke"]));
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texty.set_valign(gtk::Align::Center);
    texty.set_hexpand(true);
    let popis = match b.stav {
        1 => "Charging",
        2 | 6 => "On battery",
        4 => "Fully charged",
        5 => "Plugged in, not charging",
        _ => "",
    };
    texty.append(&popisek(popis, &[]));
    let (zbyva, predpona) = if b.stav == 2 { (b.do_vybiti, "Until empty: ") } else { (b.do_nabiti, "Until full: ") };
    if zbyva > 0 {
        let doba = if zbyva >= 3600 { format!("{} h {} min", zbyva / 3600, zbyva / 60 % 60) } else { format!("{} min", zbyva / 60) };
        texty.append(&popisek(&format!("{predpona}{doba}"), &["dim"]));
    }
    hlavni.append(&texty);
    o.panel.append(&hlavni);

    let uroven = gtk::LevelBar::for_interval(0.0, 100.0);
    uroven.set_mode(gtk::LevelBarMode::Continuous);
    for nazev in ["low", "high", "full"] {
        uroven.remove_offset_value(Some(nazev));
    }
    uroven.set_value(procenta);
    if procenta <= 15.0 {
        uroven.add_css_class("nizka");
    }
    o.panel.append(&uroven);

    let mut detaily = Vec::new();
    if b.prikon > 0.0 {
        detaily.push(format!("Power draw {:.1} W", b.prikon));
    }
    if b.zdravi > 0.0 {
        detaily.push(format!("Battery health {:.0} %", b.zdravi));
    }
    if !detaily.is_empty() {
        o.panel.append(&popisek(&detaily.join("   ·   "), &["dim"]));
    }

    o.panel.append(&oddelovac());
    o.panel.append(&popisek("Power mode", &["dim"]));
    let napajeni = skripty().join("power-mode.sh").to_string_lossy().to_string();
    let aktualni = rezim_napajeni();
    let radek = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    radek.set_homogeneous(true);
    let tlacitka: Rc<Vec<(&'static str, gtk::Button)>> = Rc::new(
        [("power-saver", "\u{f06c}  Saver"), ("balanced", "\u{f24e}  Balanced"), ("performance", "\u{f0e7}  Performance")]
            .iter()
            .map(|(r, t)| (*r, gtk::Button::with_label(t)))
            .collect(),
    );
    let oznacit = {
        let t = tlacitka.clone();
        move |aktivni: &str| {
            for (r, b) in t.iter() {
                if *r == aktivni {
                    b.add_css_class("zapnuto");
                } else {
                    b.remove_css_class("zapnuto");
                }
            }
        }
    };
    for (rezim, b) in tlacitka.iter() {
        let (rezim, oznacit, napajeni) = (*rezim, oznacit.clone(), napajeni.clone());
        b.connect_clicked(move |_| {
            oznacit(rezim);
            pozadi(&napajeni, &[rezim]);
        });
        radek.append(b);
    }
    oznacit(&aktualni);
    o.panel.append(&radek);
    if b.stav == 2 && procenta > 15.0 {
        o.panel.append(&popisek("Saver turns on by itself at 15 %", &["dim"]));
    }
    o.panel.append(&oddelovac());
    o.panel.append(&odkaz_nastaveni(o, "Power settings…", "napajeni"));
}
