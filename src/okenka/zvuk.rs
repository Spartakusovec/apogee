//! Zvuk: hlasitost a ztlumení výstupu, mikrofon, výběr výstupu (repro, sluchátka, AirPlay…).

use std::rc::Rc;

use gtk::prelude::*;

use crate::spolecne::*;

const REPRO: &str = "\u{f028}";
const ZTLUMENO: &str = "\u{f6a9}";
const MIKROFON: &str = "\u{f130}";
const MIKROFON_VYP: &str = "\u{f131}";

/// Hlasitost a ztlumení zařízení podle `pactl -f json list sinks/sources` (stejná čísla jako wpctl)
fn hlasitost(seznam: &[serde_json::Value], nazev: &str) -> (f64, bool) {
    let Some(z) = seznam.iter().find(|z| z["name"].as_str() == Some(nazev)) else { return (0.0, false) };
    let procenta = z["volume"]
        .as_object()
        .and_then(|k| k.values().next())
        .and_then(|k| k["value_percent"].as_str())
        .and_then(|p| p.trim_end_matches('%').parse::<f64>().ok())
        .unwrap_or(0.0);
    (procenta, z["mute"].as_bool().unwrap_or(false))
}

pub fn nazev_vystupu(nazev: &str, popis: &str) -> String {
    if let Some(zbytek) = nazev.strip_prefix("raop_sink.") {
        // AirPlay: raop_sink.Jana--MacBook-Air.local.… → Jana's MacBook Air (AirPlay)
        let host = zbytek.split(".local").next().unwrap_or(zbytek);
        return format!("{} (AirPlay)", host.replace("--", "'s ").replace('-', " "));
    }
    if !popis.is_empty() && popis != "(null)" {
        if popis.starts_with("Built-in Audio") {
            return "Built-in speakers / headphones".into();
        }
        return popis.into();
    }
    nazev.into()
}

/// Tlačítko ztlumení, které přepíná ikonu i vzhled.
fn tlacitko_ztlumeni(zarizeni: &'static str, ztlumeno: bool, zapnuto: &'static str, vypnuto: &'static str) -> gtk::Button {
    let b = gtk::Button::with_label(if ztlumeno { vypnuto } else { zapnuto });
    b.add_css_class(if ztlumeno { "zapnuto" } else { "radek" });
    b.add_css_class("rychle-ikona");
    b.connect_clicked(move |b| {
        pozadi("wpctl", &["set-mute", zarizeni, "toggle"]);
        let ted_ztlumeno = !b.has_css_class("zapnuto");
        b.set_label(if ted_ztlumeno { vypnuto } else { zapnuto });
        if ted_ztlumeno {
            b.remove_css_class("radek");
            b.add_css_class("zapnuto");
        } else {
            b.remove_css_class("zapnuto");
            b.add_css_class("radek");
        }
    });
    b
}

pub fn naplnit(o: &Rc<Okenko>) {
    // všechny dotazy najednou, ať okénko nečeká na každý zvlášť (pactl je rychlejší než wpctl)
    let v = soucasne(&[
        ("pactl", &["-f", "json", "list", "sinks"]),
        ("pactl", &["-f", "json", "list", "sources"]),
        ("pactl", &["-f", "json", "info"]),
    ]);
    let json = |t: &str| serde_json::from_str::<serde_json::Value>(t).ok();
    let seznam = |t: &str| json(t).and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let (sinks, sources, info) = (seznam(&v[0]), seznam(&v[1]), json(&v[2]).unwrap_or_default());
    let vychozi = info["default_sink_name"].as_str().unwrap_or("").to_string();
    let (hlas, ztlumeno) = hlasitost(&sinks, &vychozi);
    let vystupy: Vec<(String, String)> = sinks
        .iter()
        .map(|v| (v["name"].as_str().unwrap_or("").to_string(), v["description"].as_str().unwrap_or("").to_string()))
        .collect();
    let popis = vystupy.iter().find(|(n, _)| *n == vychozi).map(|(n, p)| nazev_vystupu(n, p)).unwrap_or_default();
    o.panel.append(&nadpis("Sound", &popis));

    let ztlumit = tlacitko_ztlumeni("@DEFAULT_AUDIO_SINK@", ztlumeno, REPRO, ZTLUMENO);
    let (radek_hlas, _) = posuvnik(&ztlumit, hlas, 0.0, 100.0, procenta, |v| {
        pozadi("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", v / 100.0)]);
    });
    o.panel.append(&radek_hlas);

    // mikrofon
    let (mikrofon, mik_ztlumen) = hlasitost(&sources, info["default_source_name"].as_str().unwrap_or(""));
    let mik = tlacitko_ztlumeni("@DEFAULT_AUDIO_SOURCE@", mik_ztlumen, MIKROFON, MIKROFON_VYP);
    let (radek_mik, _) = posuvnik(&mik, mikrofon, 0.0, 100.0, procenta, |v| {
        pozadi("wpctl", &["set-volume", "@DEFAULT_AUDIO_SOURCE@", &format!("{:.2}", v / 100.0)]);
    });
    // kdo právě nahrává (proto svítí ikona mikrofonu v panelu)
    let nahrava = crate::mikrofon::kdo_nahrava();
    let nadpis_mik = if nahrava.is_empty() { "Microphone".to_string() } else { format!("Microphone · in use by {}", nahrava.join(", ")) };
    o.panel.append(&popisek(&nadpis_mik, &["dim"]));
    o.panel.append(&radek_mik);

    if vystupy.len() > 1 {
        o.panel.append(&oddelovac());
        o.panel.append(&popisek("Output", &["dim"]));
        for (nazev, popis) in &vystupy {
            let n = nazev.clone();
            let slaby = Rc::downgrade(o);
            o.panel.append(&radek(
                if *nazev == vychozi { "\u{f00c}" } else { "" },
                &nazev_vystupu(nazev, popis),
                "",
                *nazev == vychozi,
                move || {
                    pozadi("pactl", &["set-default-sink", &n]);
                    if let Some(o) = slaby.upgrade() {
                        o.zavrit();
                    }
                },
            ));
        }
    }
    o.panel.append(&oddelovac());
    o.panel.append(&odkaz_nastaveni(o, "Sound settings…", "zvuk"));
}
