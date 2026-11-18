//! Přehrávač (klik na skladbu v panelu): obal alba, skladba, přetáčení a tlačítka ⏮ ⏯ ⏭.
//! Ovládá stejný přehrávač, jaký ukazuje panel (viz hudba.rs).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;

use crate::hudba::{self, Prehravac};
use crate::spolecne::*;

const OBAL: i32 = 96;

struct Okno {
    o: Rc<Okenko>,
    obsah: gtk::Box,
    obal: gtk::Image,
    bez_obalu: gtk::Label,
    titulek: gtk::Label,
    umelec: gtk::Label,
    zdroj: gtk::Label,
    casova_osa: gtk::Box,
    posuvnik: gtk::Scale,
    cas: gtk::Label,
    delka: gtk::Label,
    predchozi: gtk::Button,
    hrat: gtk::Button,
    dalsi: gtk::Button,
    adresa_obalu: RefCell<String>,
    prehravac: RefCell<Option<Prehravac>>,
    /// po přetočení chvíli neposouvat posuvník podle přehrávače (ještě neví, kde je)
    pretoceno: Cell<Option<Instant>>,
    cekajici_pretoceni: RefCell<Option<glib::SourceId>>,
}

fn tlacitko(znak: &str, tridy: &[&str], akce: &'static str) -> gtk::Button {
    let b = gtk::Button::with_label(znak);
    b.add_css_class("ovladani");
    for t in tridy {
        b.add_css_class(t);
    }
    b.connect_clicked(move |_| hudba::ovladat(akce));
    b
}

fn cas(us: i64) -> String {
    let s = (us.max(0) / 1_000_000) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn naplnit(o: &Rc<Okenko>) {
    let obsah = gtk::Box::new(gtk::Orientation::Vertical, 12);

    let hore = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let ramecek = gtk::Overlay::new();
    ramecek.add_css_class("obal");
    ramecek.set_overflow(gtk::Overflow::Hidden);
    ramecek.set_size_request(OBAL, OBAL);
    ramecek.set_valign(gtk::Align::Start);
    let bez_obalu = ikona("\u{f001}", "obal-nahradni");
    ramecek.set_child(Some(&bez_obalu));
    let obal = gtk::Image::new();
    obal.set_pixel_size(OBAL);
    ramecek.add_overlay(&obal);
    hore.append(&ramecek);
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 3);
    texty.set_valign(gtk::Align::Center);
    texty.set_hexpand(true);
    let titulek = popisek("", &["titulek-skladby"]);
    titulek.set_wrap(true);
    titulek.set_lines(2);
    titulek.set_ellipsize(gtk::pango::EllipsizeMode::End);
    titulek.set_max_width_chars(22);
    let umelec = popisek("", &[]);
    umelec.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let zdroj = popisek("", &["dim"]);
    zdroj.set_ellipsize(gtk::pango::EllipsizeMode::End);
    texty.append(&titulek);
    texty.append(&umelec);
    texty.append(&zdroj);
    hore.append(&texty);
    obsah.append(&hore);

    let casova_osa = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let posuvnik = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    posuvnik.set_draw_value(false);
    casova_osa.append(&posuvnik);
    let casy = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let cas_lbl = popisek("0:00", &["maly-cas"]);
    cas_lbl.set_hexpand(true);
    let delka = popisek("", &["maly-cas"]);
    casy.append(&cas_lbl);
    casy.append(&delka);
    casova_osa.append(&casy);
    obsah.append(&casova_osa);

    let tlacitka = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    tlacitka.set_halign(gtk::Align::Center);
    let predchozi = tlacitko("\u{f048}", &[], "predchozi");
    let hrat = tlacitko("\u{f04b}", &["hlavni"], "play-pause");
    let dalsi = tlacitko("\u{f051}", &[], "dalsi");
    predchozi.set_valign(gtk::Align::Center);
    dalsi.set_valign(gtk::Align::Center);
    tlacitka.append(&predchozi);
    tlacitka.append(&hrat);
    tlacitka.append(&dalsi);
    obsah.append(&tlacitka);
    o.panel.append(&obsah);

    let okno = Rc::new(Okno {
        o: o.clone(),
        obsah,
        obal,
        bez_obalu,
        titulek,
        umelec,
        zdroj,
        casova_osa,
        posuvnik: posuvnik.clone(),
        cas: cas_lbl,
        delka,
        predchozi,
        hrat,
        dalsi,
        adresa_obalu: RefCell::new(String::new()),
        prehravac: RefCell::new(None),
        pretoceno: Cell::new(None),
        cekajici_pretoceni: RefCell::new(None),
    });

    // přetáčení: tažení posuvníkem pošle novou pozici, až se ruka na chvíli zastaví
    let slabe = Rc::downgrade(&okno);
    posuvnik.connect_change_value(move |_, _, hodnota| {
        if let Some(w) = slabe.upgrade() {
            w.pretoceno.set(Some(Instant::now()));
            w.cas.set_text(&cas((hodnota * 1e6) as i64));
            if let Some(id) = w.cekajici_pretoceni.take() {
                id.remove();
            }
            let w2 = w.clone();
            let id = glib::timeout_add_local_once(Duration::from_millis(150), move || {
                w2.cekajici_pretoceni.replace(None);
                if let Some(p) = w2.prehravac.borrow().as_ref() {
                    hudba::pretocit(p, (hodnota * 1e6) as i64);
                }
            });
            w.cekajici_pretoceni.replace(Some(id));
        }
        glib::Propagation::Proceed
    });

    let slabe = Rc::downgrade(&okno);
    hudba::pri_zmene(Some(Box::new(move || {
        if let Some(w) = slabe.upgrade() {
            w.obnovit();
        }
    })));
    o.okno.connect_destroy(|_| hudba::pri_zmene(None));

    // pozice se nehlásí sama, ptát se každou sekundu, dokud je okénko otevřené
    let slabe = Rc::downgrade(&okno);
    glib::timeout_add_local(Duration::from_secs(1), move || match slabe.upgrade() {
        Some(w) if w.o.okno.is_visible() => {
            w.pozice();
            glib::ControlFlow::Continue
        }
        _ => glib::ControlFlow::Break,
    });

    okno.obnovit();
}

impl Okno {
    fn obnovit(self: &Rc<Self>) {
        let Some(p) = hudba::aktivni() else {
            self.obsah.set_visible(false);
            self.o.stav("Nothing is playing");
            self.prehravac.replace(None);
            return;
        };
        self.obsah.set_visible(true);
        self.o.stav("");
        self.titulek.set_text(if p.titulek.is_empty() { &p.identita } else { &p.titulek });
        self.umelec.set_text(&p.umelec);
        self.umelec.set_visible(!p.umelec.is_empty());
        let zdroj: Vec<&str> = [p.album.as_str(), p.identita.as_str()].into_iter().filter(|t| !t.is_empty()).collect();
        self.zdroj.set_text(&zdroj.join(" · "));
        self.hrat.set_label(if p.stav == "Playing" { "\u{f04c}" } else { "\u{f04b}" });
        self.predchozi.set_sensitive(p.muze_predchozi);
        self.dalsi.set_sensitive(p.muze_dalsi);
        self.casova_osa.set_visible(p.delka > 0);
        self.posuvnik.set_sensitive(p.muze_pretacet);
        self.posuvnik.set_range(0.0, (p.delka as f64 / 1e6).max(1.0));
        self.delka.set_text(&cas(p.delka));
        self.nastavit_obal(&p.obal);
        self.prehravac.replace(Some(p));
        self.pozice();
        self.o.prepocitat();
    }

    fn pozice(self: &Rc<Self>) {
        let Some(p) = self.prehravac.borrow().clone() else { return };
        if p.delka <= 0 {
            return;
        }
        let w = self.clone();
        hudba::pozice(&p, move |us| {
            if w.pretoceno.get().is_some_and(|t| t.elapsed() < Duration::from_millis(1500)) {
                return;
            }
            w.posuvnik.set_value(us as f64 / 1e6);
            w.cas.set_text(&cas(us));
        });
    }

    /// Obal alba: místní soubor rovnou, z internetu se stáhne do ~/.cache/apogee/covers.
    fn nastavit_obal(self: &Rc<Self>, adresa: &str) {
        if *self.adresa_obalu.borrow() == adresa {
            return;
        }
        self.adresa_obalu.replace(adresa.to_string());
        self.obal.clear();
        self.bez_obalu.set_visible(true);
        if let Some(cesta) = adresa.starts_with("file://").then(|| glib::filename_from_uri(adresa).ok()).flatten() {
            self.ukazat_obal(&cesta.0);
        } else if adresa.starts_with("http://") || adresa.starts_with("https://") {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            adresa.hash(&mut h);
            let slozka = domov().join(".cache/apogee/covers");
            let cesta = slozka.join(format!("{:016x}", h.finish()));
            if cesta.exists() {
                self.ukazat_obal(&cesta);
                return;
            }
            let (w, a, c, chtena) = (self.clone(), adresa.to_string(), cesta.clone(), adresa.to_string());
            na_pozadi(
                move || {
                    let _ = std::fs::create_dir_all(&slozka);
                    let docasny = cesta.with_extension("stahuje");
                    let ok = std::process::Command::new("curl")
                        .args(["-sf", "--max-time", "10", "-o"])
                        .arg(&docasny)
                        .arg(&a)
                        .status()
                        .is_ok_and(|s| s.success());
                    ok && std::fs::rename(&docasny, &cesta).is_ok()
                },
                move |ok| {
                    // mezitím se mohla změnit skladba
                    if ok && *w.adresa_obalu.borrow() == chtena {
                        w.ukazat_obal(&c);
                    }
                },
            );
        }
    }

    fn ukazat_obal(&self, cesta: &std::path::Path) {
        if cesta.exists() {
            self.obal.set_from_file(Some(cesta));
            self.bez_obalu.set_visible(false);
        }
    }
}
