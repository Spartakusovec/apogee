//! Stránka Keyboard & Touchpad: rozložení klávesnice a jejich přepínání, opakování kláves,
//! NumLock; touchpad (ťuknutí, přetahování, drag lock, posouvání, rychlost, klikání, vypnutí při
//! psaní a při připojené myši), myš a přehled gest. Vše do nastaveni.conf (`input type:…`).

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::Value;

use super::sway_conf;
use super::ui::*;
use crate::spolecne::{nastaveni, prikaz, ulozit_nastaveni};

/// rozložení: (xkb_layout, xkb_variant, název)
const ROZLOZENI: &[(&str, &str, &str)] = &[
    ("cz", "", "Czech"),
    ("cz", "qwerty", "Czech (QWERTY)"),
    ("us", "", "English (US)"),
    ("gb", "", "English (UK)"),
    ("sk", "", "Slovak"),
    ("de", "", "German"),
    ("pl", "", "Polish"),
    ("fr", "", "French"),
    ("es", "", "Spanish"),
    ("it", "", "Italian"),
    ("ua", "", "Ukrainian"),
    ("ru", "", "Russian"),
];

const PREPINANI: &[(&str, &str)] = &[("Alt + Shift", "grp:alt_shift_toggle"), ("Ctrl + Shift", "grp:ctrl_shift_toggle"), ("Caps Lock", "grp:caps_toggle")];

fn nazev(l: &str, v: &str) -> String {
    ROZLOZENI.iter().find(|r| r.0 == l && r.1 == v).map(|r| r.2.to_string()).unwrap_or_else(|| if v.is_empty() { l.to_string() } else { format!("{l} ({v})") })
}

fn rozlozeni() -> Vec<(String, String)> {
    match nastaveni("rozlozeni").as_array() {
        Some(a) if !a.is_empty() => a.iter().map(|r| (r[0].as_str().unwrap_or("cz").to_string(), r[1].as_str().unwrap_or("").to_string())).collect(),
        _ => vec![("cz".into(), String::new())],
    }
}

fn ulozit_rozlozeni(r: &[(String, String)]) {
    ulozit_nastaveni("rozlozeni", serde_json::json!(r.iter().map(|(l, v)| [l, v]).collect::<Vec<_>>()));
    let k = "input type:keyboard";
    sway_conf::nastavit(&format!("{k} xkb_layout"), &r.iter().map(|x| x.0.as_str()).collect::<Vec<_>>().join(","));
    sway_conf::nastavit(&format!("{k} xkb_variant"), &format!("\"{}\"", r.iter().map(|x| x.1.as_str()).collect::<Vec<_>>().join(",")));
    if r.len() > 1 && sway_conf::hodnota(&format!("{k} xkb_options")).is_none() {
        sway_conf::nastavit(&format!("{k} xkb_options"), PREPINANI[0].1);
    }
    super::oznamit("Saved");
    glib::timeout_add_local_once(Duration::from_millis(200), || super::obnovit("vstup"));
}

/// Aktuální nastavení libinput zařízení daného typu (touchpad / pointer), když je připojené.
fn libinput(typ: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(&prikaz("swaymsg", &["-r", "-t", "get_inputs"])).ok()?;
    v.as_array()?.iter().find(|i| i["type"].as_str() == Some(typ)).map(|i| i["libinput"].clone())
}

/// Hodnota: nejdřív z nastaveni.conf, pak ze zařízení, jinak výchozí.
fn hodnota(typ: &str, klic: &str, li: &Option<Value>, li_klic: &str, vychozi: &str) -> String {
    sway_conf::hodnota(&format!("input type:{typ} {klic}"))
        .or_else(|| li.as_ref().and_then(|l| match &l[li_klic] {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        }))
        .unwrap_or_else(|| vychozi.to_string())
}

fn prepinac(karta: &gtk::ListBox, typ: &'static str, klic: &'static str, nazev: &str, popis: &str, zapnuto: bool) {
    radek_prepinac(karta, nazev, popis, zapnuto, move |z| {
        sway_conf::nastavit(&format!("input type:{typ} {klic}"), if z { "enabled" } else { "disabled" });
    });
}

fn posuvnik(od: f64, do_: f64, krok: f64, hodnota: f64, nastavit: impl Fn(f64) + 'static) -> gtk::Scale {
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, od, do_, krok);
    s.set_value(hodnota);
    s.set_size_request(220, -1);
    s.set_draw_value(false);
    let casovac: std::rc::Rc<std::cell::RefCell<Option<glib::SourceId>>> = Default::default();
    let n = std::rc::Rc::new(nastavit);
    s.connect_value_changed(move |s| {
        if let Some(id) = casovac.take() {
            id.remove();
        }
        let (v, c, n) = (s.value(), casovac.clone(), n.clone());
        casovac.replace(Some(glib::timeout_add_local_once(Duration::from_millis(250), move || {
            c.replace(None);
            n(v);
        })));
    });
    s
}

fn vyber(volby: &[&str], vybrana: usize, zmena: impl Fn(usize) + 'static) -> gtk::DropDown {
    let d = gtk::DropDown::from_strings(volby);
    d.set_selected(vybrana as u32);
    d.connect_selected_notify(move |d| zmena(d.selected() as usize));
    d
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Keyboard & Touchpad");

    // klávesnice
    let karta = skupina(&s, "Keyboard layouts", "The first one is used after you log in.");
    let seznam = rozlozeni();
    for (i, (l, v)) in seznam.iter().enumerate() {
        let tlacitka = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        if i > 0 {
            let sez = seznam.clone();
            tlacitka.append(&tlacitko("Make first", false, move || {
                let mut s = sez.clone();
                let x = s.remove(i);
                s.insert(0, x);
                ulozit_rozlozeni(&s);
            }));
        }
        if seznam.len() > 1 {
            let sez = seznam.clone();
            tlacitka.append(&tlacitko("Remove", false, move || {
                let mut s = sez.clone();
                s.remove(i);
                ulozit_rozlozeni(&s);
            }));
        }
        radek(&karta, "\u{f11c}", &nazev(l, v), if i == 0 { "Default" } else { "" }, Some(tlacitka.upcast_ref()));
    }
    let volne: Vec<&(&str, &str, &str)> = ROZLOZENI.iter().filter(|r| !seznam.iter().any(|(l, v)| l == r.0 && v == r.1)).collect();
    let mut nazvy: Vec<&str> = vec!["Choose…"];
    nazvy.extend(volne.iter().map(|r| r.2));
    let sez = seznam.clone();
    let pridat = vyber(&nazvy, 0, move |i| {
        if i == 0 {
            return;
        }
        let r = volne[i - 1];
        let mut s = sez.clone();
        s.push((r.0.to_string(), r.1.to_string()));
        ulozit_rozlozeni(&s);
    });
    radek(&karta, "\u{f067}", "Add a layout", "", Some(pridat.upcast_ref()));
    if seznam.len() > 1 {
        let ted = sway_conf::hodnota("input type:keyboard xkb_options").unwrap_or_default();
        let d = vyber(&PREPINANI.iter().map(|p| p.0).collect::<Vec<_>>(), PREPINANI.iter().position(|p| p.1 == ted).unwrap_or(0), |i| {
            sway_conf::nastavit("input type:keyboard xkb_options", PREPINANI[i].1);
            super::oznamit("Saved");
        });
        radek(&karta, "\u{f362}", "Switch layouts with", "The panel shows the current layout", Some(d.upcast_ref()));
    }

    let karta = skupina(&s, "Typing", "");
    let zpozdeni: f64 = hodnota("keyboard", "repeat_delay", &None, "", "600").parse().unwrap_or(600.0);
    let rychlost: f64 = hodnota("keyboard", "repeat_rate", &None, "", "25").parse().unwrap_or(25.0);
    radek(&karta, "\u{f017}", "Repeat delay", "How long to hold a key before it repeats", Some(posuvnik(150.0, 1000.0, 10.0, zpozdeni, |v| {
        sway_conf::nastavit("input type:keyboard repeat_delay", &format!("{v:.0}"));
    }).upcast_ref()));
    radek(&karta, "\u{f04e}", "Repeat speed", "", Some(posuvnik(5.0, 60.0, 1.0, rychlost, |v| {
        sway_conf::nastavit("input type:keyboard repeat_rate", &format!("{v:.0}"));
    }).upcast_ref()));
    let numlock = hodnota("keyboard", "xkb_numlock", &None, "", "enabled") == "enabled";
    prepinac(&karta, "keyboard", "xkb_numlock", "Num Lock on after login", "", numlock);
    let pole = gtk::Entry::new();
    pole.set_placeholder_text(Some("Try typing here"));
    pole.set_width_chars(22);
    radek(&karta, "", "Test", "", Some(pole.upcast_ref()));

    // touchpad
    let li = libinput("touchpad");
    let ano = |klic: &str, li_klic: &str, vych: &str| hodnota("touchpad", klic, &li, li_klic, vych) != "disabled";
    let karta = skupina(&s, "Touchpad", "");
    let rychlost: f64 = hodnota("touchpad", "pointer_accel", &li, "accel_speed", "0").parse().unwrap_or(0.0);
    radek(&karta, "\u{f245}", "Pointer speed", "", Some(posuvnik(-1.0, 1.0, 0.05, rychlost, |v| {
        sway_conf::nastavit("input type:touchpad pointer_accel", &format!("{v:.2}"));
    }).upcast_ref()));
    prepinac(&karta, "touchpad", "tap", "Tap to click", "One finger = left click, two = right click, three = middle click", ano("tap", "tap", "enabled"));
    prepinac(&karta, "touchpad", "drag", "Tap and drag", "Tap, then tap again and slide to drag", ano("drag", "tap_drag", "enabled"));
    let zamek = hodnota("touchpad", "drag_lock", &li, "tap_drag_lock", "disabled");
    let volby = [("Off", "disabled"), ("Short pause", "enabled"), ("Until you tap again", "enabled_sticky")];
    let d = vyber(&volby.iter().map(|v| v.0).collect::<Vec<_>>(), volby.iter().position(|v| v.1 == zamek).unwrap_or(0), move |i| {
        sway_conf::nastavit("input type:touchpad drag_lock", volby[i].1);
    });
    radek(&karta, "", "Drag lock", "Lifting the finger while dragging doesn't drop the item", Some(d.upcast_ref()));
    prepinac(&karta, "touchpad", "natural_scroll", "Natural scrolling", "Content moves with your fingers, like on a phone", ano("natural_scroll", "natural_scroll", "enabled"));
    let posun = hodnota("touchpad", "scroll_method", &li, "scroll_method", "two_finger");
    let volby = [("Two fingers", "two_finger"), ("Right edge", "edge")];
    let d = vyber(&volby.iter().map(|v| v.0).collect::<Vec<_>>(), volby.iter().position(|v| v.1 == posun).unwrap_or(0), move |i| {
        sway_conf::nastavit("input type:touchpad scroll_method", volby[i].1);
    });
    radek(&karta, "", "Scroll with", "", Some(d.upcast_ref()));
    let klik = hodnota("touchpad", "click_method", &li, "click_method", "button_areas");
    let volby = [("Bottom corners", "button_areas"), ("Number of fingers", "clickfinger")];
    let d = vyber(&volby.iter().map(|v| v.0).collect::<Vec<_>>(), volby.iter().position(|v| v.1 == klik).unwrap_or(0), move |i| {
        sway_conf::nastavit("input type:touchpad click_method", volby[i].1);
    });
    radek(&karta, "", "Right click when pressing", "Bottom right corner, or press with two fingers", Some(d.upcast_ref()));
    prepinac(&karta, "touchpad", "dwt", "Off while typing", "Stops the palm from moving the pointer", ano("dwt", "dwt", "enabled"));
    let udalosti = hodnota("touchpad", "events", &li, "send_events", "enabled");
    radek_prepinac(&karta, "Off when a mouse is connected", "", udalosti == "disabled_on_external_mouse", |z| {
        sway_conf::nastavit("input type:touchpad events", if z { "disabled_on_external_mouse" } else { "enabled" });
    });

    // myš
    let li = libinput("pointer");
    let karta = skupina(&s, "Mouse", if li.is_some() { "" } else { "No mouse connected right now; these apply when one is." });
    let rychlost: f64 = hodnota("pointer", "pointer_accel", &li, "accel_speed", "0").parse().unwrap_or(0.0);
    radek(&karta, "\u{f8cc}", "Pointer speed", "", Some(posuvnik(-1.0, 1.0, 0.05, rychlost, |v| {
        sway_conf::nastavit("input type:pointer pointer_accel", &format!("{v:.2}"));
    }).upcast_ref()));
    let profil = hodnota("pointer", "accel_profile", &li, "accel_profile", "adaptive");
    radek_prepinac(&karta, "Mouse acceleration", "Fast movements move the pointer further. Off is better for games.", profil != "flat", |z| {
        sway_conf::nastavit("input type:pointer accel_profile", if z { "adaptive" } else { "flat" });
    });
    prepinac(&karta, "pointer", "natural_scroll", "Natural scrolling", "", hodnota("pointer", "natural_scroll", &li, "natural_scroll", "disabled") == "enabled");
    prepinac(&karta, "pointer", "left_handed", "Left-handed", "Swaps the left and right buttons", hodnota("pointer", "left_handed", &li, "left_handed", "disabled") == "enabled");

    // gesta
    let karta = skupina(&s, "Gestures", "");
    for (gesto, co) in [
        ("4 fingers left / right", "Next / previous workspace"),
        ("3 fingers up", "Show or hide the dock"),
        ("3-finger tap", "Middle click (on the volume icon: open the sound mixer)"),
        ("2 fingers up / down", "Scroll"),
    ] {
        radek(&karta, "\u{f256}", gesto, co, None);
    }
    s.upcast()
}
