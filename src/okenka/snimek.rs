//! Snímek a nahrávání obrazovky: co zachytit (oblast, okno, obrazovka), zpoždění, zvuk,
//! editor. Volby si pamatuje v ~/.config/apogee/settings.json. Samotné zachycení viz zaznam.rs.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::prelude::*;
use serde_json::{json, Value};

use crate::spolecne::*;
use crate::zaznam;

/// Řada přepínacích tlačítek, vybrané svítí.
fn volba(moznosti: &[(&str, &'static str)], vybrana: &str, pri_zmene: impl Fn(&'static str) + 'static) -> gtk::Box {
    let rada = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    rada.set_homogeneous(true);
    let pri_zmene = Rc::new(pri_zmene);
    for (text, hodnota) in moznosti {
        let b = gtk::Button::with_label(text);
        if *hodnota == vybrana {
            b.add_css_class("zapnuto");
        }
        let (pri_zmene, hodnota) = (pri_zmene.clone(), *hodnota);
        b.connect_clicked(move |b| {
            // ostatní tlačítka v řadě zhasnout (přes rodiče, ať tlačítka nedrží sama sebe)
            let mut x = b.parent().and_then(|r| r.first_child());
            while let Some(w) = x {
                w.remove_css_class("zapnuto");
                x = w.next_sibling();
            }
            b.add_css_class("zapnuto");
            pri_zmene(hodnota);
        });
        rada.append(&b);
    }
    rada
}

fn ulozit(volby: &RefCell<Value>, klic: &str, hodnota: Value) {
    volby.borrow_mut()[klic] = hodnota;
    ulozit_nastaveni("snimek", volby.borrow().clone());
}

/// `druh`: "snimek" nebo "nahravani" (podle tlačítka, kterým se okénko otevřelo).
pub fn naplnit(o: &Rc<Okenko>, druh: &str) {
    // právě se nahrává: jen čas a zastavení
    if let Some(s) = zaznam::delka() {
        o.panel.append(&nadpis("Recording the screen", &format!("{}:{:02} so far · saved to Videos → Recordings", s / 60, s % 60)));
        let stop = gtk::Button::with_label("Stop recording");
        stop.add_css_class("hlavni-akce");
        let slaby = Rc::downgrade(o);
        stop.connect_clicked(move |_| {
            zaznam::zastavit();
            if let Some(o) = slaby.upgrade() {
                o.zavrit();
            }
        });
        o.panel.append(&stop);
        return;
    }

    let mut v = nastaveni("snimek");
    if !v.is_object() {
        v = json!({});
    }
    let volby = Rc::new(RefCell::new(v));
    let text = |k: &str, vychozi: &'static str| -> String { volby.borrow()[k].as_str().unwrap_or(vychozi).to_string() };
    let rezim = text("rezim", "oblast");
    let zvuk = text("zvuk", "vypnuto");
    let zpozdeni = volby.borrow()["zpozdeni"].as_u64().unwrap_or(0);
    let upravit = volby.borrow()["upravit"].as_bool().unwrap_or(false);
    let nahravani = Rc::new(std::cell::Cell::new(druh == "nahravani"));

    o.panel.append(&volba(&[("Screenshot", "snimek"), ("Recording", "nahravani")], druh, {
        let nahravani = nahravani.clone();
        let o = Rc::downgrade(o);
        move |d| {
            nahravani.set(d == "nahravani");
            if let Some(o) = o.upgrade() {
                // přepnout viditelné volby
                let mut dite = o.panel.first_child();
                while let Some(w) = dite {
                    if w.has_css_class("jen-snimek") {
                        w.set_visible(d == "snimek");
                    }
                    if w.has_css_class("jen-nahravani") {
                        w.set_visible(d == "nahravani");
                    }
                    if let Ok(b) = w.clone().downcast::<gtk::Button>() {
                        if b.has_css_class("hlavni-akce") {
                            b.set_label(if d == "snimek" { "Take screenshot" } else { "Start recording" });
                        }
                    }
                    dite = w.next_sibling();
                }
                o.prepocitat();
            }
        }
    }));

    let sekce = |nazev: &str, trida: Option<&str>, obsah: gtk::Box| {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
        b.append(&popisek(nazev, &["dim"]));
        b.append(&obsah);
        if let Some(t) = trida {
            b.add_css_class(t);
        }
        o.panel.append(&b);
        b
    };
    let vol = volby.clone();
    sekce("Capture", None, volba(&[("Area", "oblast"), ("Window", "okno"), ("Screen", "obrazovka")], &rezim, move |r| ulozit(&vol, "rezim", json!(r))));
    let vol = volby.clone();
    let vybrane_zpozdeni = match zpozdeni { 3 => "3", 5 => "5", _ => "0" };
    let s = sekce("Delay", Some("jen-snimek"), volba(&[("None", "0"), ("3 s", "3"), ("5 s", "5")], vybrane_zpozdeni, move |z| {
        ulozit(&vol, "zpozdeni", json!(z.parse::<u64>().unwrap_or(0)))
    }));
    s.set_visible(!nahravani.get());
    let vol = volby.clone();
    let s = sekce("Sound", Some("jen-nahravani"), volba(&[("Off", "vypnuto"), ("System", "system"), ("Microphone", "mikrofon")], &zvuk, move |z| {
        ulozit(&vol, "zvuk", json!(z))
    }));
    s.set_visible(nahravani.get());

    let radek = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    radek.add_css_class("jen-snimek");
    let t = popisek("Open the editor after capture", &[]);
    t.set_hexpand(true);
    radek.append(&t);
    let sw = gtk::Switch::new();
    sw.set_active(upravit);
    sw.set_valign(gtk::Align::Center);
    let vol = volby.clone();
    sw.connect_active_notify(move |s| ulozit(&vol, "upravit", json!(s.is_active())));
    radek.append(&sw);
    radek.set_visible(!nahravani.get());
    o.panel.append(&radek);

    let na_zacatku_nahravani = nahravani.get();
    let akce = gtk::Button::with_label(if nahravani.get() { "Start recording" } else { "Take screenshot" });
    akce.add_css_class("hlavni-akce");
    akce.set_margin_top(4);
    let vol = volby.clone();
    akce.connect_clicked(move |_| {
        let v = vol.borrow().clone();
        let rezim = v["rezim"].as_str().unwrap_or("oblast").to_string();
        if nahravani.get() {
            zaznam::nahravat(&rezim, v["zvuk"].as_str().unwrap_or("vypnuto"));
        } else {
            zaznam::snimek(&rezim, v["zpozdeni"].as_u64().unwrap_or(0), v["upravit"].as_bool().unwrap_or(false));
        }
    });
    o.panel.append(&akce);
    for (text, trida, videt) in [
        ("Screenshots are also copied to the clipboard. Print Screen takes an area right away.", "jen-snimek", !na_zacatku_nahravani),
        ("Recordings are saved to Videos → Recordings. Click the red dot in the panel to stop.", "jen-nahravani", na_zacatku_nahravani),
    ] {
        let tip = popisek(text, &["dim"]);
        tip.set_wrap(true);
        tip.set_max_width_chars(40);
        tip.add_css_class(trida);
        tip.set_visible(videt);
        o.panel.append(&tip);
    }
}
