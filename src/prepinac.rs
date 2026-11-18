//! Přepínač oken Alt+Tab: seznam oken (ikona, název · titulek · plocha) podle posledního použití.
//! Drž Alt, Tab vybírá, puštění Altu přepne. Rychlé ťuknutí přepne rovnou na předchozí okno.
//! Esc zavře, Enter nebo klik myší přepne.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

use crate::sway;

const RADKU: usize = 10;

fn ladeni(zprava: &str) {
    if std::env::var_os("POMOCNIK_LADENI").is_some() {
        eprintln!("[prepinac {:?}] {zprava}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 100000).unwrap_or(0));
    }
}
const IKONA: i32 = 38;

struct Prepinac {
    okno: gtk::Window,
    seznam: gtk::Box,
    okna: Vec<sway::Okno>,
    vybrane: Cell<usize>,
    overeno: Cell<bool>,
    hotovo: Cell<bool>,
}

thread_local! {
    static OTEVRENY: RefCell<Option<Rc<Prepinac>>> = const { RefCell::new(None) };
}

pub fn alttab(smer: i32) {
    if let Some(p) = OTEVRENY.with(|o| o.borrow().clone()) {
        p.posun(smer);
        return;
    }
    let okna = sway::okna();
    if okna.len() < 2 {
        return;
    }
    crate::okenka::zavrit();
    crate::styl::pouzit();
    let vybrane = if smer > 0 { 1 } else { okna.len() - 1 };

    let okno = gtk::Window::new();
    okno.add_css_class("apogee");
    okno.init_layer_shell();
    okno.set_layer(Layer::Overlay);
    // klávesnice patří na chvíli přepínači, jinak by nepoznal puštění Altu
    okno.set_keyboard_mode(KeyboardMode::Exclusive);
    okno.set_namespace(Some("prepinac-oken"));
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_widget_name("prepinac");
    panel.set_size_request(1025, -1);
    let nadpis = gtk::Label::new(Some("Windows:"));
    nadpis.set_xalign(0.0);
    nadpis.add_css_class("nadpis-prepinace");
    panel.append(&nadpis);
    let seznam = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.append(&seznam);
    okno.set_child(Some(&panel));
    // Dokud se neví, jestli je Alt pořád dole, skoro nic neukazovat (rychlé ťuknutí nemá blikat).
    // Ne úplně 0: úplně průhledné okno GTK 4 nevykreslí, Sway ho nezobrazí a nedá mu klávesnici.
    okno.set_opacity(0.01);

    let p = Rc::new(Prepinac {
        okno: okno.clone(),
        seznam,
        okna,
        vybrane: Cell::new(vybrane),
        overeno: Cell::new(false),
        hotovo: Cell::new(false),
    });

    let klavesy = gtk::EventControllerKey::new();
    let p2 = p.clone();
    klavesy.connect_key_pressed(move |_, k, _, _| {
        ladeni(&format!("key pressed: {k:?}"));
        use gdk::Key;
        match k {
            Key::Tab | Key::Down | Key::Right => p2.posun(1),
            Key::ISO_Left_Tab | Key::Up | Key::Left => p2.posun(-1),
            Key::Return | Key::KP_Enter | Key::space => p2.potvrdit(),
            Key::Escape => p2.zavrit(),
            _ => {}
        }
        glib::Propagation::Stop
    });
    klavesy.connect_modifiers(|_, stav| {
        ladeni(&format!("signal modifiers: {stav:?}"));
        glib::Propagation::Proceed
    });
    let p2 = p.clone();
    klavesy.connect_key_released(move |_, k, _, _| {
        ladeni(&format!("key released: {k:?}"));
        if matches!(k, gdk::Key::Alt_L | gdk::Key::Alt_R | gdk::Key::Meta_L | gdk::Key::Meta_R) {
            p2.potvrdit();
        }
    });
    okno.add_controller(klavesy);

    // po převzetí klávesnice: Alt už puštěný = rychlé ťuknutí, jinak seznam ukázat
    // (is_active se u okének připnutých k okraji obrazovky nemění, proto EventControllerFocus)
    let fokus = gtk::EventControllerFocus::new();
    let p2 = p.clone();
    fokus.connect_enter(move |_| {
        ladeni("keyboard focus entered");
        let p3 = p2.clone();
        glib::timeout_add_local_once(Duration::from_millis(60), move || p3.drzi_alt());
    });
    okno.add_controller(fokus);
    let p2 = p.clone();
    glib::timeout_add_local_once(Duration::from_millis(400), move || p2.drzi_alt());

    p.vykreslit();
    OTEVRENY.with(|o| o.replace(Some(p.clone())));
    okno.present();
}

impl Prepinac {
    fn drzi_alt(self: &Rc<Self>) {
        if self.overeno.replace(true) || self.hotovo.get() {
            return;
        }
        let stav = gdk::Display::default()
            .and_then(|d| d.default_seat())
            .and_then(|s| s.keyboard())
            .map(|k| k.modifier_state());
        ladeni(&format!("drzi_alt: modifier_state = {stav:?}"));
        let alt = gdk::Display::default()
            .and_then(|d| d.default_seat())
            .and_then(|s| s.keyboard())
            .map(|k| k.modifier_state().contains(gdk::ModifierType::ALT_MASK))
            .unwrap_or(false);
        if alt {
            self.okno.set_opacity(1.0);
        } else {
            self.potvrdit();
        }
    }

    fn vykreslit(self: &Rc<Self>) {
        while let Some(d) = self.seznam.first_child() {
            self.seznam.remove(&d);
        }
        let vybrane = self.vybrane.get();
        let zacatek = vybrane.saturating_sub(RADKU / 2).min(self.okna.len().saturating_sub(RADKU));
        for i in zacatek..(zacatek + RADKU).min(self.okna.len()) {
            let o = &self.okna[i];
            let info = crate::aplikace::info(&o.app);
            let mut titulek = if o.titulek == info.jmeno { String::new() } else { o.titulek.clone() };
            if titulek.chars().count() > 55 {
                titulek = titulek.chars().take(52).collect::<String>() + "…";
            }
            let radek = gtk::Box::new(gtk::Orientation::Horizontal, 18);
            radek.add_css_class("radek-okna");
            radek.set_size_request(-1, 52); // výška řádku jako ve spouštěči
            if i == vybrane {
                radek.add_css_class("vybrany");
            }
            let ikona = if info.ikona.starts_with('/') {
                gtk::Image::from_file(&info.ikona)
            } else {
                gtk::Image::from_icon_name(&info.ikona)
            };
            ikona.set_pixel_size(IKONA);
            radek.append(&ikona);
            // stejně jako původní seznam ve spouštěči: Název · titulek · plocha
            let mut casti = vec![info.jmeno.clone()];
            if !titulek.is_empty() {
                casti.push(titulek);
            }
            casti.push(o.plocha.clone());
            let text = gtk::Label::new(Some(&casti.join("  ·  ")));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            text.set_ellipsize(gtk::pango::EllipsizeMode::End);
            radek.append(&text);
            let klik = gtk::GestureClick::new();
            let p = self.clone();
            klik.connect_released(move |_, _, _, _| {
                p.vybrane.set(i);
                p.potvrdit();
            });
            radek.add_controller(klik);
            self.seznam.append(&radek);
        }
    }

    fn posun(self: &Rc<Self>, krok: i32) {
        let n = self.okna.len() as i32;
        self.vybrane.set(((self.vybrane.get() as i32 + krok).rem_euclid(n)) as usize);
        self.vykreslit();
    }

    fn potvrdit(self: &Rc<Self>) {
        if self.hotovo.replace(true) {
            return;
        }
        self.okno.set_visible(false);
        sway::prikaz(&format!("[con_id={}] focus", self.okna[self.vybrane.get()].id));
        self.konec();
    }

    /// Esc: zavřít a vrátit fokus do okna, kde jsi byl (Sway ho sám nevrátí).
    fn zavrit(self: &Rc<Self>) {
        if self.hotovo.replace(true) {
            return;
        }
        self.okno.set_visible(false);
        if let Some(puvodni) = self.okna.iter().find(|o| o.aktivni).or(self.okna.first()) {
            sway::prikaz(&format!("[con_id={}] focus", puvodni.id));
        }
        self.konec();
    }

    fn konec(&self) {
        OTEVRENY.with(|o| o.replace(None));
        // přerušit kruhové odkazy (řádky a ovladače drží Rc na přepínač), jinak zůstane v paměti
        while let Some(d) = self.seznam.first_child() {
            self.seznam.remove(&d);
        }
        crate::spolecne::odebrat_ovladace(&self.okno);
        self.okno.destroy();
        crate::spolecne::vratit_pamet();
    }
}
