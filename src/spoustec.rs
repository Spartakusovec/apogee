//! Mřížka aplikací (tlačítko v doku): ztmavená obrazovka, uprostřed všechny aplikace s ikonami.
//! Psaním se hledá (název, popis, název programu). Fokus zůstává v hledání, šipky (i PageUp /
//! PageDown) přitom posouvají výběr v mřížce, Enter spustí vybranou (jinak první), klik spustí,
//! Esc nebo klik mimo zavře. Šest sloupců, výška na celé řádky (poslední není uříznutý).
//!
//! Okno se sestaví jednou (na pozadí po startu démona) a pak se jen ukazuje a schovává,
//! takže se otevře hned. Seznam aplikací se obnoví, když se nějaká nainstaluje nebo odinstaluje.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const IKONA: i32 = 56;
/// sloupců v mřížce a nejvyšší výška části s ikonami (pak se posouvá)
const SLOUPCU: i32 = 6;
const VYSKA: i32 = 560;

struct Spoustec {
    okno: gtk::Window,
    panel: gtk::Box,
    hledat: gtk::SearchEntry,
    mrizka: gtk::FlowBox,
    posuv: gtk::ScrolledWindow,
    /// sloupců v řádku (podle obrazovky)
    sloupcu: Cell<i32>,
    /// rozteč řádků změřená z vykreslené mřížky (0 = ještě nezměřená)
    roztec: Cell<i32>,
    /// dlaždice, text pro hledání (malými písmeny), aplikace
    aplikace: RefCell<Vec<(gtk::FlowBoxChild, String, gio::AppInfo)>>,
    /// ID aplikací, ze kterých je mřížka sestavená
    sestaveno_z: RefCell<Vec<String>>,
}

thread_local! {
    static SPOUSTEC: RefCell<Option<Rc<Spoustec>>> = const { RefCell::new(None) };
}

fn vsechny() -> Vec<gio::AppInfo> {
    let mut v: Vec<gio::AppInfo> = gio::AppInfo::all().into_iter().filter(|a| a.should_show()).collect();
    v.sort_by_key(|a| a.display_name().to_lowercase());
    v
}

/// Sestaví okno na pozadí chvíli po startu démona, ať je první otevření okamžité.
pub fn pripravit() {
    glib::timeout_add_local_once(Duration::from_secs(3), || {
        // jednou neviditelně vykreslit, ať se načtou ikony (jinak je první otevření pomalejší)
        let s = ziskat();
        s.okno.set_opacity(0.01);
        s.okno.set_visible(true);
        glib::timeout_add_local_once(Duration::from_millis(400), || {
            if let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) {
                s.prizpusobit();
                s.zmerit_radek();
                s.okno.set_visible(false);
                s.okno.set_opacity(1.0);
            }
        });
    });
}

fn ziskat() -> Rc<Spoustec> {
    if let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) {
        return s;
    }
    crate::styl::pouzit();
    let okno = gtk::Window::new();
    okno.add_css_class("apogee");
    okno.add_css_class("spoustec");
    okno.init_layer_shell();
    okno.set_layer(Layer::Overlay);
    okno.set_namespace(Some("spoustec"));
    okno.set_exclusive_zone(-1);
    for k in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        okno.set_anchor(k, true);
    }
    okno.set_keyboard_mode(KeyboardMode::Exclusive);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 14);
    panel.set_widget_name("panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Center);
    panel.set_size_request(860, -1);
    let hledat = gtk::SearchEntry::new();
    hledat.set_placeholder_text(Some("Search apps…"));
    // filtrovat hned (jinak by šipka stisknutá těsně po psaní prohrála se zpožděným filtrem)
    hledat.set_search_delay(0);
    panel.append(&hledat);
    let mrizka = gtk::FlowBox::new();
    mrizka.add_css_class("aplikace");
    mrizka.set_homogeneous(true);
    mrizka.set_min_children_per_line(SLOUPCU as u32);
    mrizka.set_max_children_per_line(SLOUPCU as u32);
    mrizka.set_selection_mode(gtk::SelectionMode::Single);
    mrizka.set_activate_on_single_click(true);
    mrizka.set_row_spacing(4);
    // jinak se při pár výsledcích řádek roztáhne na celou výšku
    mrizka.set_valign(gtk::Align::Start);
    mrizka.set_column_spacing(4);
    let posuv = gtk::ScrolledWindow::new();
    posuv.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    posuv.set_size_request(-1, VYSKA);
    posuv.set_child(Some(&mrizka));
    panel.append(&posuv);
    // ztmavení přes celou obrazovku (pozadí samotného okna GTK nekreslí)
    let pozadi = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pozadi.add_css_class("ztmaveni");
    pozadi.set_hexpand(true);
    pozadi.set_vexpand(true);
    panel.set_vexpand(true);
    pozadi.append(&panel);
    okno.set_child(Some(&pozadi));

    let s = Rc::new(Spoustec {
        okno: okno.clone(),
        panel,
        hledat: hledat.clone(),
        mrizka: mrizka.clone(),
        posuv: posuv.clone(),
        sloupcu: Cell::new(SLOUPCU),
        roztec: Cell::new(0),
        aplikace: RefCell::new(Vec::new()),
        sestaveno_z: RefCell::new(Vec::new()),
    });

    // hledání
    hledat.connect_search_changed(|h| {
        if let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) {
            s.filtrovat(&h.text().to_lowercase());
        }
    });
    hledat.connect_activate(|_| {
        if let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) {
            let vybrana = s.mrizka.selected_children().into_iter().next();
            let cil = vybrana.or_else(|| s.aplikace.borrow().iter().find(|(c, _, _)| c.is_visible()).map(|(c, _, _)| c.clone()));
            if let Some(c) = cil {
                s.spustit(&c);
            }
        }
    });
    hledat.connect_stop_search(|_| schovat());
    // šipky posouvají výběr v mřížce, fokus zůstává v hledání (dřív než pole posune kurzor v textu)
    let klavesy = gtk::EventControllerKey::new();
    klavesy.set_propagation_phase(gtk::PropagationPhase::Capture);
    klavesy.connect_key_pressed(|_, k, _, _| {
        use gtk::gdk::Key;
        let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) else { return glib::Propagation::Proceed };
        let radek = s.sloupcu.get();
        let stranka = radek * ((s.posuv.height() / s.vyska_radku().max(1)).max(1));
        let posun = match k {
            Key::Right => 1,
            Key::Left => -1,
            Key::Down => radek,
            Key::Up => -radek,
            Key::Page_Down => stranka,
            Key::Page_Up => -stranka,
            _ => return glib::Propagation::Proceed,
        };
        s.posunout_vyber(posun);
        glib::Propagation::Stop
    });
    hledat.add_controller(klavesy);
    // Esc i v mřížce
    let esc = gtk::EventControllerKey::new();
    esc.connect_key_pressed(|_, k, _, _| {
        if k == gtk::gdk::Key::Escape {
            schovat();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    okno.add_controller(esc);
    mrizka.connect_child_activated(|_, c| {
        if let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) {
            s.spustit(c);
        }
    });
    // klik mimo panel zavře
    let klik = gtk::GestureClick::new();
    klik.connect_released(|g, _, x, y| {
        if let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) {
            let uvnitr = s.okno.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|w| w == *s.panel.upcast_ref::<gtk::Widget>() || w.is_ancestor(&s.panel));
            if !uvnitr {
                g.set_state(gtk::EventSequenceState::Claimed);
                schovat();
            }
        }
    });
    okno.add_controller(klik);

    SPOUSTEC.with(|x| x.replace(Some(s.clone())));
    s.obnovit_seznam();
    s
}

impl Spoustec {
    /// Sestaví dlaždice znovu, jen když se seznam aplikací změnil.
    fn obnovit_seznam(&self) {
        let vse = vsechny();
        let id: Vec<String> = vse.iter().map(|a| a.id().map(|i| i.to_string()).unwrap_or_default()).collect();
        if *self.sestaveno_z.borrow() == id {
            return;
        }
        while let Some(c) = self.mrizka.first_child() {
            self.mrizka.remove(&c);
        }
        let mut aplikace = Vec::new();
        for a in vse {
            let dlazdice = gtk::Box::new(gtk::Orientation::Vertical, 6);
            dlazdice.add_css_class("aplikace-dlazdice");
            let obrazek = match a.icon() {
                Some(i) => gtk::Image::from_gicon(&i),
                None => gtk::Image::from_icon_name("application-x-executable"),
            };
            obrazek.set_pixel_size(IKONA);
            dlazdice.append(&obrazek);
            let nazev = gtk::Label::new(Some(&a.display_name()));
            nazev.set_ellipsize(gtk::pango::EllipsizeMode::End);
            nazev.set_max_width_chars(14);
            nazev.set_justify(gtk::Justification::Center);
            dlazdice.append(&nazev);
            let c = gtk::FlowBoxChild::new();
            c.set_child(Some(&dlazdice));
            self.mrizka.append(&c);
            // hledat v názvu, popisu a názvu programu
            let text = format!("{} {} {}", a.display_name(), a.description().unwrap_or_default(), a.executable().to_string_lossy());
            aplikace.push((c, text.to_lowercase(), a));
        }
        self.aplikace.replace(aplikace);
        self.sestaveno_z.replace(id);
    }

    fn filtrovat(&self, hledane: &str) {
        let mut prvni = None;
        for (c, text, _) in self.aplikace.borrow().iter() {
            let ano = hledane.split_whitespace().all(|slovo| text.contains(slovo));
            c.set_visible(ano);
            if ano && prvni.is_none() {
                prvni = Some(c.clone());
            }
        }
        match prvni {
            Some(c) if !hledane.is_empty() => self.mrizka.select_child(&c),
            _ => self.mrizka.unselect_all(),
        }
    }

    fn viditelne(&self) -> Vec<gtk::FlowBoxChild> {
        self.aplikace.borrow().iter().filter(|(c, _, _)| c.is_visible()).map(|(c, _, _)| c.clone()).collect()
    }

    /// Rozteč řádků mřížky. Změřená z vykreslené mřížky (dlaždice pod sebou), dokud není, odhad
    /// z přirozené výšky dlaždice (ta vychází o pár pixelů menší než skutečný řádek).
    fn vyska_radku(&self) -> i32 {
        if self.roztec.get() > 0 {
            return self.roztec.get();
        }
        let dlazdice = self.viditelne().first().map(|c| c.measure(gtk::Orientation::Vertical, -1).1).unwrap_or(110);
        dlazdice + self.mrizka.row_spacing() as i32
    }

    /// Změří rozteč řádků z vykreslené mřížky; když se liší od použité, přizpůsobí velikost.
    fn zmerit_radek(&self) {
        let v = self.viditelne();
        let Some(prvni) = v.first().and_then(|c| c.compute_bounds(&self.mrizka)) else { return };
        // první dlaždice o řádek níž (rozvržení může být ještě se starým počtem sloupců)
        let Some(dalsi) = v.iter().filter_map(|c| c.compute_bounds(&self.mrizka)).map(|b| b.y()).find(|y| *y > prvni.y() + 1.0) else { return };
        let roztec = (dalsi - prvni.y()).round() as i32;
        if roztec > 0 && roztec != self.roztec.get() {
            self.roztec.set(roztec);
            self.prizpusobit();
        }
    }

    /// Posune výběr o `o` dlaždic (řádek = počet sloupců); bez výběru vybere první.
    fn posunout_vyber(&self, o: i32) {
        let v = self.viditelne();
        if v.is_empty() {
            return;
        }
        let ted = self.mrizka.selected_children().first().and_then(|s| v.iter().position(|c| c == s));
        let nova = match ted {
            None => 0,
            Some(i) => {
                let n = i as i32 + o;
                // nahoře / dole za okrajem zůstat (nahoru o řádek z prvního řádku nic nedělá)
                if n < 0 {
                    if o.abs() >= self.sloupcu.get() && i as i32 >= self.sloupcu.get() { 0 } else { i }
                } else if n >= v.len() as i32 {
                    if o.abs() >= self.sloupcu.get() { v.len() - 1 } else { i }
                } else {
                    n as usize
                }
            }
        };
        self.mrizka.select_child(&v[nova]);
        // vybraná dlaždice vždy celá v pohledu
        if let Some(b) = v[nova].compute_bounds(&self.mrizka) {
            let a = self.posuv.vadjustment();
            let (y, h) = (b.y() as f64, b.height() as f64);
            if y < a.value() {
                a.set_value(y);
            } else if y + h > a.value() + a.page_size() {
                a.set_value(y + h - a.page_size());
            }
        }
    }

    /// Výška části s ikonami na celé řádky (nejvýš VYSKA), ať poslední řádek není uříznutý.
    fn prizpusobit(&self) {
        let radek = self.vyska_radku();
        let radku = (VYSKA / radek).max(2);
        self.posuv.set_size_request(-1, radku * radek - self.mrizka.row_spacing() as i32);
    }

    fn spustit(&self, c: &gtk::FlowBoxChild) {
        let aplikace = self.aplikace.borrow().iter().find(|(x, _, _)| x == c).map(|(_, _, a)| a.clone());
        schovat();
        if let Some(a) = aplikace {
            let kontext = gtk::gdk::Display::default().map(|d| d.app_launch_context());
            let _ = a.launch(&[], kontext.as_ref());
        }
    }
}

pub fn prepnout() {
    let s = ziskat();
    if s.okno.is_visible() {
        schovat();
        return;
    }
    crate::okenka::zavrit();
    s.obnovit_seznam();
    s.hledat.set_text("");
    s.filtrovat("");
    s.prizpusobit();
    s.posuv.vadjustment().set_value(0.0);
    glib::timeout_add_local_once(Duration::from_millis(60), || {
        if let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) {
            s.zmerit_radek();
        }
    });
    s.okno.set_visible(true);
    s.hledat.grab_focus();
}

fn schovat() {
    if let Some(s) = SPOUSTEC.with(|s| s.borrow().clone()) {
        s.okno.set_visible(false);
    }
}
