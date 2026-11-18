//! Okénko pod zvonečkem: rychlá tlačítka, Do Not Disturb, pomodoro, aktualizace a historie
//! notifikací (vede ji démon, viz upozorneni/); zvoneček v panelu.

use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::glib;
use gtk::prelude::*;
use serde_json::{json, Value};

use crate::spolecne::*;

fn ted() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// Zvoneček v panelu: počet nepřečtených, Do Not Disturb. Volá se při každé nové notifikaci.
pub fn obnovit_zvonek() {
    let neprectene = crate::upozorneni::neprectenych();
    let pocet = if neprectene > 0 { format!(" {neprectene}") } else { String::new() };
    let (ikona, trida, popis) = if dnd_zapnuto() {
        ("\u{f1f6}", "dnd", "Do Not Disturb is on")
    } else if neprectene > 0 {
        ("\u{f0f3}", "neprectene", "")
    } else {
        ("\u{f0f3}", "zadne", "")
    };
    crate::panel::publikovat("notifikace", json!({"text": format!("{ikona}{pocet}"), "class": trida, "tooltip": popis}));
}

/// Do Not Disturb: přepnout (None), zapnout nebo vypnout.
pub fn nastavit_dnd(zapnout: Option<bool>) {
    crate::upozorneni::nastavit_dnd(zapnout.unwrap_or_else(|| !dnd_zapnuto()));
    obnovit_zvonek();
}

/// Notifikace z historie démona od nejnovější.
fn nacist_seznam() -> Vec<Value> {
    crate::upozorneni::historie().iter().map(crate::upozorneni::do_json).collect()
}

fn dnd_zapnuto() -> bool {
    crate::upozorneni::dnd()
}

struct Notifikace {
    o: Rc<Okenko>,
    seznam: gtk::Box,
    smazat_vse: gtk::Button,
}

pub fn naplnit(o: &Rc<Okenko>) {
    // rychlá tlačítka, dvě řady po čtyřech
    let rada = gtk::Grid::new();
    rada.set_column_homogeneous(true);
    rada.set_column_spacing(8);
    rada.set_row_spacing(8);
    // zapnuté věci svítí: Wi-Fi, Bluetooth, noční režim, úsporný režim
    let noc_zapnuta = nocni_rezim();
    let wifi_zapnuta = wifi_zapnuta();
    let bt_zapnuty = bluetooth_zapnuty();
    let usporny = rezim_napajeni() == "power-saver";
    let otevrit = |druh: &'static str| move || {
        glib::idle_add_local_once(move || crate::okenka::prepnout(druh));
    };
    let tlacitka: Vec<(&str, &str, Box<dyn Fn()>, bool)> = vec![
        ("\u{f1eb}", "Wi-Fi", Box::new(otevrit("wifi")), wifi_zapnuta),
        ("\u{f293}", "Bluetooth", Box::new(otevrit("bluetooth")), bt_zapnuty),
        ("\u{f186}", "Night light", Box::new(|| pozadi(&skripty().join("night-light.sh").to_string_lossy(), &["prepnout"])), noc_zapnuta),
        ("\u{f06c}", "Battery and power mode", Box::new(otevrit("baterie")), usporny),
        ("\u{f030}", "Screenshot", Box::new(otevrit("snimek")), false),
        ("\u{f03d}", "Screen recording", Box::new(otevrit("nahravani")), crate::zaznam::nahrava()),
        ("\u{f03e}", "Wallpaper", Box::new(otevrit("tapety")), false),
        ("\u{f023}", "Lock", Box::new(|| {
            crate::okenka::zavrit();
            pozadi("gtklock", &["-d"]);
        }), false),
    ];
    for (i, (znak, popis, akce, zapnuto)) in tlacitka.into_iter().enumerate() {
        let b = gtk::Button::with_label(znak);
        b.set_tooltip_text(Some(popis));
        b.add_css_class("rychle");
        if zapnuto {
            b.add_css_class("zapnuto");
        }
        let prepinaci = popis == "Night light";
        b.connect_clicked(move |b| {
            akce();
            if prepinaci {
                if b.has_css_class("zapnuto") {
                    b.remove_css_class("zapnuto");
                } else {
                    b.add_css_class("zapnuto");
                }
            }
        });
        rada.attach(&b, (i % 4) as i32, (i / 4) as i32, 1, 1);
    }
    o.panel.append(&rada);

    // Do Not Disturb
    let radek = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    radek.append(&ikona("\u{f1f6}", "ikona"));
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texty.set_hexpand(true);
    texty.append(&popisek("Do Not Disturb", &[]));
    texty.append(&popisek("Notifications stay quiet, critical ones still show", &["dim"]));
    radek.append(&texty);
    let sw = gtk::Switch::new();
    sw.set_active(dnd_zapnuto());
    sw.set_valign(gtk::Align::Center);
    sw.connect_active_notify(|s| nastavit_dnd(Some(s.is_active())));
    radek.append(&sw);
    o.panel.append(&radek);
    o.panel.append(&super::pomodoro::sekce(o));
    // aktualizace: ikona v panelu je jen, když nějaké jsou, tady jsou vidět vždycky
    let a = crate::aktualizace::nacist();
    let pocet = a["dnf"].as_array().map_or(0, |x| x.len()) + a["flatpak"].as_array().map_or(0, |x| x.len());
    let stav_aktualizaci = if crate::aktualizace::kontroluje() {
        "Checking…".to_string()
    } else if a["cas"].as_u64().is_none() {
        "Not checked yet".to_string()
    } else if pocet == 0 {
        "Up to date".to_string()
    } else {
        format!("{pocet} available")
    };
    o.panel.append(&crate::spolecne::radek("\u{f019}", "Updates", &stav_aktualizaci, pocet > 0, || {
        glib::idle_add_local_once(|| crate::okenka::prepnout("aktualizace"));
    }));
    o.panel.append(&oddelovac());

    // historie
    let hlavicka = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let n = popisek("Notifications", &["nadpis"]);
    n.set_hexpand(true);
    hlavicka.append(&n);
    let smazat_vse = gtk::Button::with_label("Clear all");
    smazat_vse.add_css_class("odkaz");
    hlavicka.append(&smazat_vse);
    o.panel.append(&hlavicka);
    let seznam = gtk::Box::new(gtk::Orientation::Vertical, 6);
    o.panel.append(&seznam);

    let nt = Rc::new(Notifikace { o: o.clone(), seznam, smazat_vse: smazat_vse.clone() });
    // slabě: tlačítko drží struktura Notifikace, silný odkaz by se nikdy neuvolnil
    let nt2 = Rc::downgrade(&nt);
    smazat_vse.connect_clicked(move |_| {
        let Some(nt2) = nt2.upgrade() else { return };
        crate::upozorneni::smazat_vse();
        obnovit_zvonek();
        nt2.vykreslit();
    });
    nt.vykreslit();

    // otevřením je vše přečtené
    crate::upozorneni::oznacit_prectene();
    obnovit_zvonek();
}

fn ikona_aplikace(nazev: &str) -> gtk::Image {
    if nazev.starts_with('/') {
        let i = gtk::Image::from_file(nazev);
        i.set_pixel_size(32);
        return i;
    }
    let nazev = if nazev.is_empty() { "dialog-information" } else { nazev };
    let i = gtk::Image::from_icon_name(nazev);
    i.set_pixel_size(32);
    i
}

fn pred(cas: f64) -> String {
    let rozdil = ted() - cas;
    if rozdil < 60.0 {
        "now".into()
    } else if rozdil < 3600.0 {
        format!("{} min ago", (rozdil / 60.0) as i64)
    } else if rozdil < 86400.0 {
        format!("{} h ago", (rozdil / 3600.0) as i64)
    } else {
        chrono::DateTime::from_timestamp(cas as i64, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%-d %b").to_string())
            .unwrap_or_default()
    }
}

impl Notifikace {
    fn vykreslit(self: &Rc<Self>) {
        while let Some(d) = self.seznam.first_child() {
            self.seznam.remove(&d);
        }
        let notifikace = nacist_seznam();
        self.smazat_vse.set_visible(!notifikace.is_empty());
        if notifikace.is_empty() {
            self.seznam.append(&popisek("No notifications", &["dim"]));
        }
        for n in notifikace.iter().take(7) {
            let karta = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            karta.add_css_class("notifikace");
            let ik = ikona_aplikace(n["app_icon"].as_str().or(n["desktop_entry"].as_str()).unwrap_or(""));
            ik.set_valign(gtk::Align::Start);
            karta.append(&ik);
            let texty = gtk::Box::new(gtk::Orientation::Vertical, 2);
            texty.set_hexpand(true);
            let vrch = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let aplikace = popisek(n["app_name"].as_str().unwrap_or(""), &["dim"]);
            aplikace.set_hexpand(true);
            vrch.append(&aplikace);
            vrch.append(&popisek(&pred(n["cas"].as_f64().unwrap_or(ted())), &["dim"]));
            let zavrit = gtk::Button::with_label("\u{f00d}");
            zavrit.add_css_class("zavrit");
            let (id, nt) = (n["id"].as_i64().unwrap_or(0), self.clone());
            zavrit.connect_clicked(move |_| {
                crate::upozorneni::smazat(id as u32);
                obnovit_zvonek();
                nt.vykreslit();
            });
            vrch.append(&zavrit);
            texty.append(&vrch);
            if let Some(s) = n["summary"].as_str().filter(|s| !s.is_empty()) {
                let l = popisek(s, &["nadpis-notifikace"]);
                l.set_wrap(true);
                l.set_max_width_chars(34);
                texty.append(&l);
            }
            if let Some(t) = n["body"].as_str().filter(|s| !s.is_empty()) {
                let l = popisek("", &["telo"]);
                l.set_wrap(true);
                l.set_max_width_chars(34);
                l.set_lines(3);
                l.set_ellipsize(gtk::pango::EllipsizeMode::End);
                // tělo může mít formátování (<b>…), poškozené ukázat jako obyčejný text
                if gtk::pango::parse_markup(t, '\0').is_ok() {
                    l.set_markup(t);
                } else {
                    l.set_text(t);
                }
                texty.append(&l);
            }
            karta.append(&texty);
            self.seznam.append(&karta);
        }
        if notifikace.len() > 7 {
            self.seznam.append(&popisek(&format!("and {} more", notifikace.len() - 7), &["dim"]));
        }
        self.o.prepocitat();
    }
}
