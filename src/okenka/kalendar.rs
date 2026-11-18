//! Kalendář pod hodinami: měsíc od pondělí, čísla týdnů, dnešek zvýrazněný.
//! Listování: šipky ‹ ›, kolečko / tah na touchpadu, šipky na klávesnici; Today skočí zpátky.

use std::cell::Cell;
use std::rc::Rc;

use chrono::{Datelike, Local, Months, NaiveDate};
use gtk::glib;
use gtk::prelude::*;

use crate::spolecne::*;

struct Kalendar {
    o: Rc<Okenko>,
    dnes: NaiveDate,
    mesic: Cell<NaiveDate>,
    nazev: gtk::Label,
    mrizka: gtk::Grid,
    posun: Cell<f64>,
}

pub fn naplnit(o: &Rc<Okenko>) {
    let dnes = Local::now().date_naive();
    o.panel.append(&popisek(&format!("{}  ·  Week {}", dnes.format("%A, %-d %B %Y"), dnes.iso_week().week()), &["dim"]));

    let hlavicka = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let nazev = popisek("", &["nadpis"]);
    nazev.set_hexpand(true);
    hlavicka.append(&nazev);
    let k = Rc::new(Kalendar {
        o: o.clone(),
        dnes,
        mesic: Cell::new(dnes.with_day(1).unwrap_or(dnes)),
        nazev,
        mrizka: gtk::Grid::new(),
        posun: Cell::new(0.0),
    });
    for (text, krok, trida) in [("\u{f053}", -1, "sipka"), ("Today", 0, "odkaz"), ("\u{f054}", 1, "sipka")] {
        let b = gtk::Button::with_label(text);
        b.add_css_class(trida);
        if trida == "sipka" {
            b.add_css_class("rychle-ikona");
        }
        let k2 = k.clone();
        b.connect_clicked(move |_| k2.listovat(krok));
        hlavicka.append(&b);
    }
    o.panel.append(&hlavicka);
    k.mrizka.set_column_spacing(2);
    k.mrizka.set_row_spacing(2);
    o.panel.append(&k.mrizka);
    o.panel.append(&oddelovac());
    o.panel.append(&odkaz(o, "Open the Calendar app", "gnome-calendar", &[]));

    // kolečko myši / tah dvěma prsty
    let kolecko = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let k2 = k.clone();
    kolecko.connect_scroll(move |_, _, dy| {
        // touchpad posílá spoustu malých posunů: listovat až po pořádném tahu
        let celkem = k2.posun.get() + dy;
        if celkem.abs() >= 1.0 {
            k2.posun.set(0.0);
            k2.listovat(if celkem > 0.0 { 1 } else { -1 });
        } else {
            k2.posun.set(celkem);
        }
        glib::Propagation::Stop
    });
    o.okno.add_controller(kolecko);

    // šipky na klávesnici
    let klavesy = gtk::EventControllerKey::new();
    let k2 = k.clone();
    klavesy.connect_key_pressed(move |_, klavesa, _, _| {
        use gtk::gdk::Key;
        let krok = match klavesa {
            Key::Left | Key::Up => -1,
            Key::Right | Key::Down => 1,
            _ => return glib::Propagation::Proceed,
        };
        k2.listovat(krok);
        glib::Propagation::Stop
    });
    o.okno.add_controller(klavesy);
    k.vykreslit();
}

impl Kalendar {
    fn listovat(self: &Rc<Self>, krok: i32) {
        let m = self.mesic.get();
        self.mesic.set(match krok {
            0 => self.dnes.with_day(1).unwrap_or(self.dnes),
            k if k > 0 => m.checked_add_months(Months::new(1)).unwrap_or(m),
            _ => m.checked_sub_months(Months::new(1)).unwrap_or(m),
        });
        self.vykreslit();
    }

    fn vykreslit(self: &Rc<Self>) {
        while let Some(d) = self.mrizka.first_child() {
            self.mrizka.remove(&d);
        }
        let mesic = self.mesic.get();
        self.nazev.set_text(&mesic.format("%B %Y").to_string());
        for (sloupec, den) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].iter().enumerate() {
            let l = popisek(den, &["den-hlavicka"]);
            l.set_xalign(0.5);
            self.mrizka.attach(&l, sloupec as i32 + 1, 0, 1, 1);
        }
        // od pondělí týdne, ve kterém je 1. den měsíce, šest řádků jako ve spouštěči kalendářů
        let mut den = mesic - chrono::Duration::days(mesic.weekday().num_days_from_monday() as i64);
        let posledni = mesic.checked_add_months(Months::new(1)).unwrap_or(mesic) - chrono::Duration::days(1);
        let mut radek = 1;
        while den <= posledni || den.weekday() != chrono::Weekday::Mon {
            if den.weekday() == chrono::Weekday::Mon {
                let t = popisek(&den.iso_week().week().to_string(), &["tyden"]);
                t.set_xalign(0.5);
                self.mrizka.attach(&t, 0, radek, 1, 1);
            }
            let l = popisek(&den.day().to_string(), &["den"]);
            l.set_xalign(0.5);
            if den.month() != mesic.month() {
                l.add_css_class("jiny-mesic");
            } else if den == self.dnes {
                l.add_css_class("dnes");
            } else if den.weekday().num_days_from_monday() >= 5 {
                l.add_css_class("vikend");
            }
            self.mrizka.attach(&l, den.weekday().num_days_from_monday() as i32 + 1, radek, 1, 1);
            if den.weekday() == chrono::Weekday::Sun {
                radek += 1;
            }
            den += chrono::Duration::days(1);
        }
        self.o.prepocitat();
    }
}
