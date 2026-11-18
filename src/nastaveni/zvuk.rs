//! Stránka Sound: výstup a vstup (výběr zařízení, hlasitost, ztlumení) a hlasitost aplikací,
//! které právě hrají. Nahrazuje pavucontrol. Data z `pactl -f json`, nastavení přes wpctl / pactl.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::Value;

use super::ui::*;
use crate::okenka::zvuk::nazev_vystupu;
use crate::spolecne::{na_pozadi, pozadi, prikaz};

struct Data {
    sinks: Vec<Value>,
    sources: Vec<Value>,
    aplikace: Vec<Value>,
    vychozi_vystup: String,
    vychozi_vstup: String,
}

fn nacist() -> Data {
    let json = |a: &[&str]| serde_json::from_str::<Value>(&prikaz("pactl", a)).unwrap_or_default();
    let pole = |v: Value| v.as_array().cloned().unwrap_or_default();
    let info = json(&["-f", "json", "info"]);
    Data {
        sinks: pole(json(&["-f", "json", "list", "sinks"])),
        // monitory výstupů nejsou mikrofony
        sources: pole(json(&["-f", "json", "list", "sources"])).into_iter().filter(|s| s["properties"]["device.class"].as_str() != Some("monitor")).collect(),
        aplikace: pole(json(&["-f", "json", "list", "sink-inputs"])),
        vychozi_vystup: info["default_sink_name"].as_str().unwrap_or("").into(),
        vychozi_vstup: info["default_source_name"].as_str().unwrap_or("").into(),
    }
}

fn hlasitost(z: &Value) -> f64 {
    z["volume"]
        .as_object()
        .and_then(|k| k.values().next())
        .and_then(|k| k["value_percent"].as_str())
        .and_then(|p| p.trim_end_matches('%').parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Posuvník hlasitosti 0–100 % s číslem; změny se posílají nejvýš ~15× za sekundu.
fn posuvnik(hodnota: f64, nastavit: impl Fn(f64) + 'static) -> gtk::Box {
    let obal = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    s.set_value(hodnota.min(100.0));
    s.set_size_request(220, -1);
    s.set_draw_value(false);
    let cislo = gtk::Label::new(Some(&format!("{hodnota:.0} %")));
    cislo.add_css_class("popis");
    cislo.set_width_chars(5);
    let cekajici: Rc<Cell<Option<f64>>> = Rc::new(Cell::new(None));
    let casovac: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let nastavit = Rc::new(nastavit);
    let c = cislo.clone();
    s.connect_value_changed(move |s| {
        let v = s.value().round();
        c.set_text(&format!("{v:.0} %"));
        cekajici.set(Some(v));
        if casovac.borrow().is_none() {
            let (cek, cas, nast) = (cekajici.clone(), casovac.clone(), nastavit.clone());
            let id = glib::timeout_add_local_once(Duration::from_millis(60), move || {
                cas.replace(None);
                if let Some(v) = cek.take() {
                    nast(v);
                }
            });
            casovac.replace(Some(id));
        }
    });
    obal.append(&s);
    obal.append(&cislo);
    obal
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Sound");
    let vystup = skupina(&s, "Output", "");
    let vstup = skupina(&s, "Input", "");
    let aplikace = skupina(&s, "Apps", "Volume of each app that is playing sound right now.");
    nacitani(&vystup, "Loading…");
    let karty = (vystup, vstup, aplikace);
    na_pozadi(nacist, move |d| {
        if karty.0.root().is_none() {
            return;
        }
        ukazat(&karty, &d);
    });
    s.upcast()
}

fn ukazat((vystup, vstup, aplikace): &(gtk::ListBox, gtk::ListBox, gtk::ListBox), d: &Data) {
    // výstup: hlasitost výchozího, ztlumení, výběr zařízení
    vycistit(vystup);
    if let Some(z) = d.sinks.iter().find(|z| z["name"].as_str() == Some(d.vychozi_vystup.as_str())) {
        radek(vystup, "\u{f028}", "Volume", "", Some(posuvnik(hlasitost(z), |v| {
            pozadi("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", v / 100.0)]);
        }).upcast_ref()));
        radek_prepinac(vystup, "Mute", "", z["mute"].as_bool().unwrap_or(false), |m| {
            pozadi("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", if m { "1" } else { "0" }]);
        });
    }
    if d.sinks.is_empty() {
        radek(vystup, "", "No speakers or headphones found", "", None);
    }
    for z in &d.sinks {
        let nazev = z["name"].as_str().unwrap_or("").to_string();
        let popis = nazev_vystupu(&nazev, z["description"].as_str().unwrap_or(""));
        let vybrany = nazev == d.vychozi_vystup;
        let p = popis.clone();
        radek_akce(vystup, if vybrany { "\u{f00c}" } else { "" }, &popis, if vybrany { "Playing here" } else { "" }, "", move || {
            pozadi("pactl", &["set-default-sink", &nazev]);
            super::oznamit(&format!("Sound now plays on {p}"));
            glib::timeout_add_local_once(Duration::from_millis(400), || super::obnovit("zvuk"));
        });
    }

    // vstup: mikrofon
    vycistit(vstup);
    if let Some(z) = d.sources.iter().find(|z| z["name"].as_str() == Some(d.vychozi_vstup.as_str())) {
        let nahrava = crate::mikrofon::kdo_nahrava();
        let popis = if nahrava.is_empty() { String::new() } else { format!("In use by {}", nahrava.join(", ")) };
        radek(vstup, "\u{f130}", "Microphone volume", &popis, Some(posuvnik(hlasitost(z), |v| {
            pozadi("wpctl", &["set-volume", "@DEFAULT_AUDIO_SOURCE@", &format!("{:.2}", v / 100.0)]);
        }).upcast_ref()));
        radek_prepinac(vstup, "Mute microphone", "", z["mute"].as_bool().unwrap_or(false), |m| {
            pozadi("wpctl", &["set-mute", "@DEFAULT_AUDIO_SOURCE@", if m { "1" } else { "0" }]);
        });
    }
    if d.sources.len() > 1 {
        for z in &d.sources {
            let nazev = z["name"].as_str().unwrap_or("").to_string();
            let popis = z["description"].as_str().unwrap_or(&nazev).replace("Built-in Audio Analog Stereo", "Built-in microphone");
            let vybrany = nazev == d.vychozi_vstup;
            radek_akce(vstup, if vybrany { "\u{f00c}" } else { "" }, &popis, if vybrany { "In use" } else { "" }, "", move || {
                pozadi("pactl", &["set-default-source", &nazev]);
                glib::timeout_add_local_once(Duration::from_millis(400), || super::obnovit("zvuk"));
            });
        }
    } else if d.sources.is_empty() {
        radek(vstup, "", "No microphone found", "", None);
    }

    // aplikace
    vycistit(aplikace);
    if d.aplikace.is_empty() {
        radek(aplikace, "", "No app is playing sound", "", None);
    }
    for a in &d.aplikace {
        let index = a["index"].as_i64().unwrap_or(-1).to_string();
        let jmeno = a["properties"]["application.name"].as_str().unwrap_or("App").to_string();
        let co = a["properties"]["media.name"].as_str().unwrap_or("").to_string();
        let i = index.clone();
        let row = radek(aplikace, "\u{f001}", &jmeno, &co, Some(posuvnik(hlasitost(a), move |v| {
            pozadi("pactl", &["set-sink-input-volume", &i, &format!("{v:.0}%")]);
        }).upcast_ref()));
        let _ = row;
    }
}
