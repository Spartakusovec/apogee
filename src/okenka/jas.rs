//! Jas a noční režim (nastavení v ~/.config/sway/night-light.conf, spouští night-light.sh).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::spolecne::*;

fn soubor() -> std::path::PathBuf {
    domov().join(".config/sway/night-light.conf")
}

pub fn nacist() -> HashMap<String, String> {
    std::fs::read_to_string(soubor())
        .unwrap_or_default()
        .lines()
        .filter_map(|r| r.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

/// Uloží jednu hodnotu a noční režim podle nastavení znovu spustí.
pub fn ulozit(klic: &str, hodnota: &str) {
    let mut radky: Vec<String> = std::fs::read_to_string(soubor())
        .unwrap_or_default()
        .lines()
        .filter(|r| !r.starts_with(&format!("{klic}=")))
        .map(String::from)
        .collect();
    radky.push(format!("{klic}={hodnota}"));
    let _ = std::fs::write(soubor(), radky.join("\n") + "\n");
    pozadi(&skripty().join("night-light.sh").to_string_lossy(), &[]);
}

pub fn naplnit(o: &Rc<Okenko>) {
    let jas: f64 = prikaz("brightnessctl", &["-m"])
        .split(',')
        .nth(3)
        .and_then(|s| s.trim_end_matches('%').parse().ok())
        .unwrap_or(50.0);
    o.panel.append(&nadpis("Display", ""));
    let (radek_jas, _) = posuvnik(&ikona("\u{f185}", "ikona"), jas, 1.0, 100.0, procenta, |v| {
        pozadi("brightnessctl", &["-q", "set", &format!("{}%", v.round())]);
    });
    o.panel.append(&radek_jas);
    o.panel.append(&oddelovac());

    let conf = nacist();
    let rezim = conf.get("rezim").cloned().unwrap_or_else(|| "zapad".into());
    let teplota: f64 = conf.get("teplota").and_then(|t| t.parse().ok()).unwrap_or(4000.0);

    // vypínač
    let radek = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    radek.append(&ikona("\u{f186}", "ikona"));
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texty.append(&popisek("Night light", &[]));
    texty.append(&popisek("Warmer colors in the evening, easier on the eyes", &["dim"]));
    texty.set_hexpand(true);
    radek.append(&texty);
    let prepinac = gtk::Switch::new();
    prepinac.set_active(rezim != "vypnuto");
    prepinac.set_valign(gtk::Align::Center);
    radek.append(&prepinac);
    o.panel.append(&radek);

    let nastaveni = gtk::Box::new(gtk::Orientation::Vertical, 10);
    o.panel.append(&nastaveni);

    // teplota: posuvník ukazuje míru teplého světla, vlevo 5500 K, plný = 2500 K
    let radek_t = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    radek_t.append(&popisek("Cooler", &["dim"]));
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_value((5500.0 - teplota) / 30.0);
    let cekani: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    scale.connect_value_changed(move |s| {
        let kelviny = ((5500.0 - s.value() * 30.0) / 100.0).round() * 100.0;
        if let Some(id) = cekani.take() {
            id.remove();
        }
        let c = cekani.clone();
        cekani.replace(Some(glib::timeout_add_local_once(Duration::from_millis(350), move || {
            c.replace(None);
            ulozit("teplota", &format!("{kelviny}"));
        })));
    });
    radek_t.append(&scale);
    radek_t.append(&popisek("Warmer", &["dim"]));
    nastaveni.append(&radek_t);

    // kdy: od západu slunce / pořád
    let radek_k = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    radek_k.set_homogeneous(true);
    let tlacitka: Rc<Vec<(&'static str, gtk::Button)>> = Rc::new(
        [("zapad", "Sunset to sunrise"), ("vzdy", "Always on")]
            .iter()
            .map(|(k, t)| (*k, gtk::Button::with_label(t)))
            .collect(),
    );
    let oznacit = {
        let t = tlacitka.clone();
        move |aktivni: &str| {
            for (k, b) in t.iter() {
                if *k == aktivni {
                    b.add_css_class("zapnuto");
                } else {
                    b.remove_css_class("zapnuto");
                }
            }
        }
    };
    for (klic, b) in tlacitka.iter() {
        let klic = *klic;
        let oznacit = oznacit.clone();
        b.connect_clicked(move |_| {
            oznacit(klic);
            ulozit("rezim", klic);
        });
        radek_k.append(b);
    }
    let posledni = conf.get("posledni").cloned().unwrap_or_else(|| "zapad".into());
    oznacit(if rezim == "vypnuto" { &posledni } else { &rezim });
    nastaveni.append(&radek_k);
    nastaveni.set_sensitive(rezim != "vypnuto");

    let t = tlacitka.clone();
    prepinac.connect_active_notify(move |sw| {
        let vybrany = t.iter().find(|(_, b)| b.has_css_class("zapnuto")).map(|(k, _)| *k).unwrap_or("zapad");
        if sw.is_active() {
            ulozit("rezim", vybrany);
        } else {
            ulozit("posledni", vybrany);
            ulozit("rezim", "vypnuto");
        }
        nastaveni.set_sensitive(sw.is_active());
    });
}
