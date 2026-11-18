//! Bubliny notifikací vpravo nahoře pod lištou. Klik spustí výchozí akci (třeba „Edit“ u snímku),
//! jinak bublinu zavře; pravý klik zavře. Když je nad bublinou myš, nezmizí. Nejvýš pět najednou,
//! nejnovější nahoře.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use super::{Duvod, Upozorneni};

const NEJVIC: usize = 5;
const IKONA: i32 = 40;

struct Bublina {
    id: u32,
    karta: gtk::Box,
    nalehavost: u8,
    doba_ms: u32,
    casovac: RefCell<Option<glib::SourceId>>,
}

thread_local! {
    static OKNO: RefCell<Option<(gtk::Window, gtk::Box)>> = const { RefCell::new(None) };
    static BUBLINY: RefCell<Vec<Rc<Bublina>>> = const { RefCell::new(Vec::new()) };
}

fn okno() -> (gtk::Window, gtk::Box) {
    if let Some(o) = OKNO.with(|o| o.borrow().clone()) {
        return o;
    }
    crate::styl::pouzit();
    let okno = gtk::Window::new();
    okno.add_css_class("apogee");
    okno.add_css_class("bubliny");
    okno.init_layer_shell();
    okno.set_layer(Layer::Top);
    okno.set_namespace(Some("notifikace"));
    okno.set_keyboard_mode(KeyboardMode::None);
    okno.set_anchor(Edge::Top, true);
    okno.set_anchor(Edge::Right, true);
    okno.set_margin(Edge::Top, 6);
    okno.set_margin(Edge::Right, 8);
    let sloupec = gtk::Box::new(gtk::Orientation::Vertical, 8);
    okno.set_child(Some(&sloupec));
    OKNO.with(|o| o.replace(Some((okno.clone(), sloupec.clone()))));
    (okno, sloupec)
}

pub fn je_videt(id: u32) -> bool {
    BUBLINY.with(|b| b.borrow().iter().any(|x| x.id == id))
}

fn obrazek(n: &Upozorneni) -> gtk::Image {
    let nazev = if !n.ikona.is_empty() { n.ikona.clone() } else if !n.desktop.is_empty() { crate::aplikace::info(&n.desktop).ikona } else { "dialog-information".into() };
    let i = if nazev.starts_with('/') { gtk::Image::from_file(&nazev) } else { gtk::Image::from_icon_name(&nazev) };
    i.set_pixel_size(IKONA);
    i.set_valign(gtk::Align::Start);
    i
}

fn obsah(n: &Upozorneni, karta: &gtk::Box) {
    while let Some(d) = karta.first_child() {
        karta.remove(&d);
    }
    karta.append(&obrazek(n));
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texty.set_hexpand(true);
    let radek = |text: &str, trida: &str, znacky: bool| {
        let l = gtk::Label::new(None);
        l.set_xalign(0.0);
        l.set_wrap(true);
        l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        l.set_max_width_chars(36);
        l.add_css_class(trida);
        // tělo může mít formátování (<b>…); poškozené ukázat jako obyčejný text
        if znacky && gtk::pango::parse_markup(text, '\0').is_ok() {
            l.set_markup(text);
        } else {
            l.set_text(text);
        }
        l
    };
    if !n.aplikace.is_empty() {
        texty.append(&radek(&n.aplikace, "aplikace", false));
    }
    if !n.nadpis.is_empty() {
        texty.append(&radek(&n.nadpis, "nadpis", false));
    }
    if !n.telo.is_empty() {
        let t = radek(&n.telo, "telo", true);
        t.set_lines(4);
        t.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texty.append(&t);
    }
    if let Some(p) = n.prubeh {
        let u = gtk::LevelBar::for_interval(0.0, 100.0);
        u.set_mode(gtk::LevelBarMode::Continuous);
        for nazev in ["low", "high", "full"] {
            u.remove_offset_value(Some(nazev));
        }
        u.set_value(p.clamp(0, 100) as f64);
        u.set_margin_top(6);
        texty.append(&u);
    }
    // ostatní akce jako tlačítka („default“ je klik na bublinu)
    let tlacitka: Vec<&(String, String)> = n.akce.iter().filter(|(k, _)| k != "default").collect();
    if !tlacitka.is_empty() {
        let rada = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        rada.set_margin_top(6);
        for (klic, popisek) in tlacitka {
            let b = gtk::Button::with_label(popisek);
            b.add_css_class("akce");
            let (id, klic) = (n.id, klic.clone());
            b.connect_clicked(move |_| {
                super::vyvolat_akci(id, &klic);
                zavrit(id, Duvod::Zavrena);
            });
            rada.append(&b);
        }
        texty.append(&rada);
    }
    karta.append(&texty);
}

fn naplanovat(b: &Rc<Bublina>) {
    if let Some(id) = b.casovac.take() {
        id.remove();
    }
    if b.doba_ms == 0 {
        return;
    }
    let id = b.id;
    let c = glib::timeout_add_local_once(Duration::from_millis(b.doba_ms as u64), move || {
        if let Some(x) = BUBLINY.with(|v| v.borrow().iter().find(|x| x.id == id).cloned()) {
            x.casovac.replace(None);
        }
        zavrit(id, Duvod::Vyprsela);
    });
    b.casovac.replace(Some(c));
}

pub fn ukazat(n: &Upozorneni) {
    let (okno, sloupec) = okno();
    // stejné id (nahrazení): jen nový obsah a nový čas
    if let Some(b) = BUBLINY.with(|v| v.borrow().iter().find(|x| x.id == n.id).cloned()) {
        obsah(n, &b.karta);
        naplanovat(&b);
        return;
    }
    let karta = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    karta.add_css_class("bublina");
    if n.nalehavost == 2 {
        karta.add_css_class("kriticka");
    }
    karta.set_size_request(380, -1);
    obsah(n, &karta);
    let b = Rc::new(Bublina { id: n.id, karta: karta.clone(), nalehavost: n.nalehavost, doba_ms: n.doba_ms, casovac: RefCell::new(None) });

    let (id, ma_vychozi) = (n.id, n.akce.iter().any(|(k, _)| k == "default"));
    let klik = gtk::GestureClick::new();
    klik.set_button(0);
    klik.connect_released(move |g, _, _, _| match g.current_button() {
        1 => {
            if ma_vychozi {
                super::vyvolat_akci(id, "default");
            }
            zavrit(id, Duvod::Zavrena);
        }
        3 => zavrit(id, Duvod::Zavrena),
        _ => {}
    });
    karta.add_controller(klik);
    // myš nad bublinou: nezmizí
    let mys = gtk::EventControllerMotion::new();
    let slaba = Rc::downgrade(&b);
    mys.connect_enter(move |_, _, _| {
        if let Some(b) = slaba.upgrade() {
            if let Some(c) = b.casovac.take() {
                c.remove();
            }
        }
    });
    let slaba = Rc::downgrade(&b);
    mys.connect_leave(move |_| {
        if let Some(b) = slaba.upgrade() {
            naplanovat(&b);
        }
    });
    karta.add_controller(mys);

    sloupec.prepend(&karta);
    naplanovat(&b);
    BUBLINY.with(|v| v.borrow_mut().insert(0, b));
    // víc než pět: nejstarší pryč (v historii zůstanou)
    let nadbytecne: Vec<u32> = BUBLINY.with(|v| v.borrow().iter().skip(NEJVIC).map(|x| x.id).collect());
    for id in nadbytecne {
        zavrit(id, Duvod::Vyprsela);
    }
    okno.set_default_size(1, 1);
    okno.set_visible(true);
}

/// Zavře bublinu (když je vidět) a oznámí to aplikaci.
pub fn zavrit(id: u32, duvod: Duvod) {
    let Some(b) = BUBLINY.with(|v| {
        let mut v = v.borrow_mut();
        let i = v.iter().position(|x| x.id == id)?;
        Some(v.remove(i))
    }) else {
        return;
    };
    if let Some(c) = b.casovac.take() {
        c.remove();
    }
    if let Some((okno, sloupec)) = OKNO.with(|o| o.borrow().clone()) {
        sloupec.remove(&b.karta);
        if sloupec.first_child().is_none() {
            okno.set_visible(false);
            crate::spolecne::vratit_pamet();
        } else {
            okno.set_default_size(1, 1);
        }
    }
    super::ohlasit_zavreni(id, duvod);
}

/// Do Not Disturb: zavřít všechny bubliny kromě kritických.
pub fn schovat_nekriticke() {
    let id: Vec<u32> = BUBLINY.with(|v| v.borrow().iter().filter(|x| x.nalehavost != 2).map(|x| x.id).collect());
    for i in id {
        zavrit(i, Duvod::Zavrena);
    }
}
