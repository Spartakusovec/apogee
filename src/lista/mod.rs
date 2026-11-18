//! Horní lišta (dřív waybar): tři ostrůvky nahoře na obrazovce.
//!
//!   vlevo      název aplikace (klik = nabídka okna), dlaždice, plochy s ikonami oken, režim Swaye
//!   uprostřed  hodiny, pomodoro, hudba s tlačítky ⏮ ⏭
//!   vpravo     nahrávání, mikrofon, počasí, tray, zvuk, jas, síť, baterie, aktualizace, zvoneček, vypnutí
//!
//! Moduly jsou popisky s pango textem; jejich obsah (JSON {text, class, tooltip}) posílají zdroje
//! přes panel::publikovat, lišta je jen kreslí. Prázdný text = modul schovaný.
//! Kolečko nad zvukem a jasem mění hlasitost / jas, nad plochami přepíná plochy.

pub mod zdroje;
pub mod tray;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;

/// Co dělá klik / kolečko na modulu.
#[derive(Clone, Copy)]
enum Akce {
    Okenko(&'static str),
    Fn(fn()),
    Nic,
}

struct Modul {
    id: &'static str,
    levy: Akce,
    prostredni: Akce,
    pravy: Akce,
    /// kolečko nahoru, dolů
    kolecko: Option<(fn(), fn())>,
    /// pevná nápověda (jinak z dat, jen u modulů, které ji mají)
    napoveda: Option<&'static str>,
}

const fn m(id: &'static str, levy: Akce) -> Modul {
    Modul { id, levy, prostredni: Akce::Nic, pravy: Akce::Nic, kolecko: None, napoveda: None }
}

fn moduly_vlevo() -> Vec<Modul> {
    vec![
        m("aplikace", Akce::Okenko("okno")),
        Modul { napoveda: None, ..m("dlazdice", Akce::Fn(crate::sway::prepnout_dlazdice)) },
    ]
}

fn moduly_uprostred() -> Vec<Modul> {
    vec![
        m("hodiny", Akce::Okenko("kalendar")),
        m("pomodoro", Akce::Okenko("notifikace")),
        Modul { napoveda: Some("Previous track"), ..m("predchozi", Akce::Fn(|| crate::hudba::ovladat("predchozi"))) },
        // pravý klik = totéž co prostřední: touchpad pozná nejvýš dva prsty, ťuknutí třemi je pravý klik
        Modul { prostredni: Akce::Fn(prehrat_pozastavit), pravy: Akce::Fn(prehrat_pozastavit), ..m("hudba", Akce::Okenko("prehravac")) },
        Modul { napoveda: Some("Next track"), ..m("dalsi", Akce::Fn(|| crate::hudba::ovladat("dalsi"))) },
    ]
}

fn prehrat_pozastavit() {
    crate::hudba::ovladat("play-pause");
}

fn pavucontrol() {
    crate::spolecne::pozadi("pavucontrol", &[]);
}

fn moduly_vpravo() -> Vec<Modul> {
    vec![
        m("nahravani", Akce::Fn(crate::zaznam::zastavit)),
        m("mikrofon", Akce::Okenko("zvuk")),
        // rozložení klávesnice (jen když je jich víc): klik = další, pravý klik = Settings
        Modul {
            pravy: Akce::Fn(|| crate::nastaveni::otevrit("vstup")),
            ..m("rozlozeni", Akce::Fn(|| crate::sway::prikaz("input type:keyboard xkb_switch_layout next")))
        },
        Modul { pravy: Akce::Fn(|| crate::okenka::prikaz("pocasi-obnovit", "")), ..m("pocasi", Akce::Okenko("pocasi")) },
        // tray se vkládá sem (tray::widget)
        Modul {
            prostredni: Akce::Fn(pavucontrol),
            pravy: Akce::Fn(pavucontrol),
            kolecko: Some((|| crate::osd::hlasitost("vol-up"), || crate::osd::hlasitost("vol-down"))),
            ..m("zvuk", Akce::Okenko("zvuk"))
        },
        Modul { kolecko: Some((|| crate::osd::jas("jas-up"), || crate::osd::jas("jas-down"))), ..m("jas", Akce::Okenko("jas")) },
        m("sit", Akce::Okenko("wifi")),
        m("baterie", Akce::Okenko("baterie")),
        m("aktualizace", Akce::Okenko("aktualizace")),
        Modul { pravy: Akce::Fn(|| crate::okenka::prikaz("dnd", "")), ..m("notifikace", Akce::Okenko("notifikace")) },
        m("vypnout", Akce::Okenko("vypnout")),
    ]
}

/// Moduly, u kterých se ukazuje nápověda z dat (u ostatních ne: otevírají okénko).
const S_NAPOVEDOU: &[&str] = &["dlazdice", "notifikace", "rozlozeni"];

/// Střed modulu v px od levého okraje obrazovky a šířka obrazovky (pro okénko pod modulem).
fn zapamatovat_kotvu(w: &gtk::Widget) {
    let Some(koren) = w.root() else { return };
    let Some(bod) = w.compute_point(&koren, &gtk::graphene::Point::new(w.width() as f32 / 2.0, 0.0)) else { return };
    let Some(povrch) = w.native().and_then(|n| n.surface()) else { return };
    let Some(monitor) = povrch.display().monitor_at_surface(&povrch) else { return };
    // panel je od levého okraje obrazovky odsazený o 8 px (spustit: set_margin)
    crate::spolecne::nastavit_kotvu(bod.x() as f64 + 8.0, monitor.geometry().width());
}

fn provest(a: Akce) {
    match a {
        // až po dokončení obsluhy kliknutí (okénko si bere klávesnici)
        Akce::Okenko(druh) => {
            glib::idle_add_local_once(move || crate::okenka::prepnout(druh));
        }
        Akce::Fn(f) => f(),
        Akce::Nic => {}
    }
}

struct Lista {
    /// modul → (obal, popisek, poslední třída z dat)
    moduly: RefCell<HashMap<&'static str, (gtk::Box, gtk::Label, String)>>,
    plochy: gtk::Box,
}

thread_local! {
    static LISTA: RefCell<Option<Rc<Lista>>> = const { RefCell::new(None) };
}

fn vytvorit_modul(md: &Modul, lista: &Lista) -> gtk::Box {
    let obal = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    obal.set_widget_name(md.id);
    obal.add_css_class("modul");
    if !matches!(md.levy, Akce::Nic) {
        obal.add_css_class("klikaci");
    }
    let popisek = gtk::Label::new(None);
    popisek.set_use_markup(true);
    popisek.set_valign(gtk::Align::Center);
    obal.append(&popisek);
    obal.set_visible(false);
    if let Some(n) = md.napoveda {
        obal.set_tooltip_text(Some(n));
    }
    let (levy, prostredni, pravy) = (md.levy, md.prostredni, md.pravy);
    let klik = gtk::GestureClick::new();
    klik.set_button(0);
    klik.connect_released(move |g, _, _, _| {
        let akce = match g.current_button() {
            1 => levy,
            2 => prostredni,
            3 => pravy,
            _ => return,
        };
        // okénko se otevře pod tímhle modulem
        if matches!(akce, Akce::Okenko(_)) {
            if let Some(w) = g.widget() {
                zapamatovat_kotvu(&w);
            }
        }
        provest(akce);
    });
    obal.add_controller(klik);
    if let Some((nahoru, dolu)) = md.kolecko {
        pridat_kolecko(&obal, KROK_HLASITOSTI, Duration::ZERO, move |n| if n { nahoru() } else { dolu() });
    }
    lista.moduly.borrow_mut().insert(md.id, (obal.clone(), popisek, String::new()));
    obal
}

/// Touchpad posílá posun v pixelech (víc událostí na jedno pohnutí prsty), kolečko myši po zářezech.
/// Kolik pixelů posunu dvěma prsty je jeden krok hlasitosti / jasu a jedno přepnutí plochy.
const KROK_HLASITOSTI: f64 = 25.0;
const KROK_PLOCH: f64 = 30.0;

/// Kolečko / posun dvěma prsty: `akce(true)` = nahoru (prsty nahoru i při přirozeném posouvání),
/// `akce(false)` = dolů. `prodleva` = nejkratší čas mezi dvěma akcemi (jedno švihnutí = jedna plocha).
fn pridat_kolecko(w: &impl IsA<gtk::Widget>, krok_px: f64, prodleva: Duration, akce: impl Fn(bool) + 'static) {
    use gtk::EventControllerScrollFlags as F;
    let c = gtk::EventControllerScroll::new(F::BOTH_AXES | F::PHYSICAL_DIRECTION);
    let nasbirano = Rc::new(Cell::new(0.0f64));
    let posledni = Rc::new(Cell::new(None::<Instant>));
    let n = nasbirano.clone();
    c.connect_scroll(move |c, dx, dy| {
        // vodorovný posun (prsty doprava = další) se počítá jako svislý
        let d = if dy.abs() >= dx.abs() { dy } else { dx };
        let krok = if c.unit() == gtk::gdk::ScrollUnit::Surface { krok_px } else { 1.0 };
        let mut s = n.get();
        if s * d < 0.0 {
            s = 0.0; // obrácený směr: začít znovu
        }
        s += d;
        if s.abs() >= krok * 0.99 {
            let nahoru = s < 0.0;
            s = 0.0;
            if posledni.get().is_none_or(|t| t.elapsed() >= prodleva) {
                posledni.set(Some(Instant::now()));
                akce(nahoru);
            }
        }
        n.set(s);
        glib::Propagation::Stop
    });
    c.connect_scroll_end(move |_| nasbirano.set(0.0));
    w.add_controller(c);
}

fn ostruvek() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class("ostruvek");
    b
}

/// Moduly, které uživatel v Settings schoval (`panel_skryte` v nastaveni.json).
fn skryty(modul: &str) -> bool {
    let id = match modul {
        "predchozi" | "dalsi" => "hudba",
        m => m,
    };
    crate::spolecne::nastaveni("panel_skryte").as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(id)))
}

/// Po změně v Settings: znovu ukázat / schovat moduly podle nastavení.
pub fn obnovit_moduly() {
    let Some(l) = LISTA.with(|l| l.borrow().clone()) else { return };
    for (id, (obal, popisek, _)) in l.moduly.borrow().iter() {
        if *id != "rezim" {
            obal.set_visible(!popisek.text().is_empty() && !skryty(id));
        }
    }
    zdroje::zmenit_format();
}

pub fn spustit() {
    crate::styl::pouzit();
    let okno = gtk::Window::new();
    okno.add_css_class("lista");
    okno.init_layer_shell();
    okno.set_layer(Layer::Top);
    okno.set_namespace(Some("lista"));
    okno.set_keyboard_mode(KeyboardMode::None);
    for (k, okraj) in [(Edge::Top, 8), (Edge::Left, 8), (Edge::Right, 8)] {
        okno.set_anchor(k, true);
        okno.set_margin(k, okraj);
    }
    okno.auto_exclusive_zone_enable();

    let plochy = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    plochy.add_css_class("plochy");
    let lista = Rc::new(Lista { moduly: RefCell::new(HashMap::new()), plochy: plochy.clone() });

    let vlevo = ostruvek();
    for md in moduly_vlevo() {
        vlevo.append(&vytvorit_modul(&md, &lista));
    }
    vlevo.append(&plochy);
    // kolečko nad plochami přepíná plochy
    pridat_kolecko(&plochy, KROK_PLOCH, Duration::from_millis(250), |nahoru| crate::sway::plocha_vedle(!nahoru));
    let rezim = gtk::Label::new(None);
    rezim.set_widget_name("rezim");
    rezim.add_css_class("modul");
    rezim.set_visible(false);
    vlevo.append(&rezim);
    lista.moduly.borrow_mut().insert("rezim", (gtk::Box::new(gtk::Orientation::Horizontal, 0), rezim.clone(), String::new()));
    // režim má jen popisek (bez obalu): obal výše je atrapa, schovává se popisek
    let uprostred = ostruvek();
    for md in moduly_uprostred() {
        uprostred.append(&vytvorit_modul(&md, &lista));
    }
    let vpravo = ostruvek();
    for md in moduly_vpravo() {
        let w = vytvorit_modul(&md, &lista);
        vpravo.append(&w);
        if md.id == "pocasi" {
            vpravo.append(&tray::widget());
        }
    }

    let stred = gtk::CenterBox::new();
    stred.set_start_widget(Some(&vlevo));
    stred.set_center_widget(Some(&uprostred));
    stred.set_end_widget(Some(&vpravo));
    okno.set_child(Some(&stred));
    LISTA.with(|l| l.replace(Some(lista)));

    let rx = crate::panel::odebirat();
    glib::spawn_future_local(async move {
        while let Ok((modul, json)) = rx.recv().await {
            if let Some(l) = LISTA.with(|l| l.borrow().clone()) {
                l.zobrazit(&modul, &json);
            }
        }
    });
    zdroje::spustit();
    okno.present();
}

impl Lista {
    fn zobrazit(&self, modul: &str, json: &str) {
        let v: Value = serde_json::from_str(json).unwrap_or_default();
        if modul == "plochy" {
            self.plochy_kreslit(&v);
            return;
        }
        let mut moduly = self.moduly.borrow_mut();
        let Some((obal, popisek, stara_trida)) = moduly.get_mut(modul) else { return };
        let text = v["text"].as_str().unwrap_or("");
        if modul == "rezim" {
            popisek.set_markup(text);
            popisek.set_visible(!text.is_empty());
            return;
        }
        popisek.set_markup(text);
        obal.set_visible(!text.is_empty() && !skryty(modul));
        let trida = v["class"].as_str().unwrap_or("").to_string();
        if *stara_trida != trida {
            for t in stara_trida.split_whitespace() {
                obal.remove_css_class(t);
            }
            for t in trida.split_whitespace() {
                obal.add_css_class(t);
            }
            *stara_trida = trida;
        }
        if S_NAPOVEDOU.contains(&modul) {
            let n = v["tooltip"].as_str().unwrap_or("");
            obal.set_tooltip_markup(if n.is_empty() { None } else { Some(n) });
        }
    }

    /// Plochy: číslo a ikony oken, aktivní zvýrazněná, prázdné ztlumené; klik přepne.
    fn plochy_kreslit(&self, v: &Value) {
        let plochy = v["plochy"].as_array().cloned().unwrap_or_default();
        // přestavět jen když se změnil počet, jinak upravit na místě
        let mut tlacitka: Vec<gtk::Box> = Vec::new();
        let mut d = self.plochy.first_child();
        while let Some(w) = d {
            d = w.next_sibling();
            if let Ok(b) = w.downcast::<gtk::Box>() {
                tlacitka.push(b);
            }
        }
        if tlacitka.len() != plochy.len() {
            for t in &tlacitka {
                self.plochy.remove(t);
            }
            tlacitka.clear();
            for _ in &plochy {
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                b.add_css_class("plocha");
                let l = gtk::Label::new(None);
                l.set_valign(gtk::Align::Center);
                b.append(&l);
                let klik = gtk::GestureClick::new();
                let slaby = b.downgrade();
                klik.connect_released(move |_, _, _, _| {
                    if let Some(b) = slaby.upgrade() {
                        crate::sway::prikaz(&format!("workspace \"{}\"", b.widget_name().replace('"', "")));
                    }
                });
                b.add_controller(klik);
                self.plochy.append(&b);
                tlacitka.push(b);
            }
        }
        for (b, p) in tlacitka.iter().zip(plochy.iter()) {
            let jmeno = p["jmeno"].as_str().unwrap_or("");
            b.set_widget_name(jmeno);
            let ikony = p["ikony"].as_str().unwrap_or("");
            let text = if ikony.is_empty() { jmeno.to_string() } else { format!("{jmeno}  {ikony}") };
            if let Some(l) = b.first_child().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                l.set_text(&text);
            }
            for (trida, klic) in [("aktivni", "aktivni"), ("prazdna", "prazdna"), ("nalehava", "nalehava")] {
                if p[klic].as_bool().unwrap_or(false) {
                    b.add_css_class(trida);
                } else {
                    b.remove_css_class(trida);
                }
            }
        }
    }
}
