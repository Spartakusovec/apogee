//! Pomodoro v okénku notifikací: časy práce a přestávky, spustit, pozastavit, zastavit.

use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::pomodoro::{self, Faze};
use crate::spolecne::*;

fn pole(minuty: Option<u64>) -> gtk::Entry {
    let e = gtk::Entry::new();
    e.set_width_chars(3);
    e.set_max_width_chars(3);
    e.set_max_length(3);
    e.set_input_purpose(gtk::InputPurpose::Digits);
    if let Some(m) = minuty {
        e.set_text(&m.to_string());
    }
    e
}

fn minuty(e: &gtk::Entry) -> Option<u64> {
    e.text().trim().parse::<u64>().ok().filter(|&m| (1..=600).contains(&m))
}

pub fn sekce(o: &Rc<Okenko>) -> gtk::Box {
    let sekce = gtk::Box::new(gtk::Orientation::Vertical, 8);

    let hlavicka = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    hlavicka.append(&ikona("\u{f2f2}", "ikona"));
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texty.set_hexpand(true);
    texty.set_valign(gtk::Align::Center);
    texty.append(&popisek("Focus timer", &[]));
    let stav = popisek("", &["dim"]);
    texty.append(&stav);
    hlavicka.append(&texty);
    let pauza = gtk::Button::with_label("Pause");
    pauza.set_valign(gtk::Align::Center);
    let stop = gtk::Button::with_label("Stop");
    stop.set_valign(gtk::Align::Center);
    hlavicka.append(&pauza);
    hlavicka.append(&stop);
    sekce.append(&hlavicka);

    // časy: dokud je nenastavíš, nic se nespustí
    let nastaveni = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let (prace, prestavka) = pomodoro::casy().map_or((None, None), |(a, b)| (Some(a), Some(b)));
    let pole_prace = pole(prace);
    let pole_prestavka = pole(prestavka);
    nastaveni.append(&popisek("Work", &["dim"]));
    nastaveni.append(&pole_prace);
    nastaveni.append(&popisek("min", &["dim"]));
    let mezera = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mezera.set_size_request(6, -1);
    nastaveni.append(&mezera);
    nastaveni.append(&popisek("Break", &["dim"]));
    nastaveni.append(&pole_prestavka);
    nastaveni.append(&popisek("min", &["dim"]));
    let mezera = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mezera.set_hexpand(true);
    nastaveni.append(&mezera);
    let start = gtk::Button::with_label("Start");
    start.add_css_class("zapnuto");
    nastaveni.append(&start);
    sekce.append(&nastaveni);

    let obnovit = {
        let (pp, pr) = (&pole_prace, &pole_prestavka);
        Rc::new(glib::clone!(
            #[weak]
            stav,
            #[weak]
            pauza,
            #[weak]
            stop,
            #[weak]
            nastaveni,
            #[weak]
            start,
            #[weak]
            pp,
            #[weak]
            pr,
            move || {
            match pomodoro::info() {
                Some((faze, zbyva, pozastaveno)) => {
                    let cast = if faze == Faze::Prace { "Work" } else { "Break" };
                    let kolik = pomodoro::odpocet(zbyva);
                    stav.set_text(&if pozastaveno { format!("{cast} · paused at {kolik}") } else { format!("{cast} · {kolik} left") });
                    pauza.set_label(if pozastaveno { "Resume" } else { "Pause" });
                    pauza.set_visible(true);
                    stop.set_visible(true);
                    nastaveni.set_visible(false);
                }
                None => {
                    stav.set_text("Set your work and break minutes");
                    pauza.set_visible(false);
                    stop.set_visible(false);
                    nastaveni.set_visible(true);
                    start.set_sensitive(minuty(&pp).is_some() && minuty(&pr).is_some());
                }
            }
        }))
    };
    obnovit();
    for e in [&pole_prace, &pole_prestavka] {
        let (obnovit, o) = (obnovit.clone(), Rc::downgrade(o));
        e.connect_changed(move |_| obnovit());
        // při psaní okénko nezavírat, když myš odjede
        e.connect_has_focus_notify(move |e| {
            if let Some(o) = o.upgrade() {
                o.drzet.set(e.has_focus());
            }
        });
    }
    let (ob, o2, pp, pr) = (obnovit.clone(), Rc::downgrade(o), pole_prace.downgrade(), pole_prestavka.downgrade());
    start.connect_clicked(move |_| {
        let (Some(pp), Some(pr)) = (pp.upgrade(), pr.upgrade()) else { return };
        if let (Some(a), Some(b), Some(o)) = (minuty(&pp), minuty(&pr), o2.upgrade()) {
            o.drzet.set(false);
            pomodoro::spustit(a, b);
            ob();
            o.prepocitat();
        }
    });
    let ob = obnovit.clone();
    pauza.connect_clicked(move |_| {
        pomodoro::pozastavit_nebo_pokracovat();
        ob();
    });
    let (ob, o2) = (obnovit.clone(), Rc::downgrade(o));
    stop.connect_clicked(move |_| {
        pomodoro::zastavit();
        ob();
        if let Some(o) = o2.upgrade() {
            o.prepocitat();
        }
    });
    // odpočet, dokud je okénko otevřené
    let slabe = Rc::downgrade(o);
    glib::timeout_add_local(Duration::from_millis(500), move || match slabe.upgrade() {
        Some(o) if o.okno.is_visible() => {
            if pomodoro::info().is_some() {
                obnovit();
            }
            glib::ControlFlow::Continue
        }
        _ => glib::ControlFlow::Break,
    });
    sekce
}
