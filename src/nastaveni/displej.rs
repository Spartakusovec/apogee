//! Stránka Display: jas, noční režim (night-light.conf jako okénko v panelu) a pro každou
//! obrazovku rozlišení, obnovovací frekvence, měřítko a zapnutí (jen u druhého monitoru).
//! Rozlišení a měřítko se 15 s ptají „Keep?“, jinak se vrátí. Ukládá se do nastaveni.conf.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::Value;

use super::sway_conf;
use super::ui::*;
use crate::okenka::jas as nocni;
use crate::spolecne::{na_pozadi, prikaz};

const MERITKA: [f64; 6] = [1.0, 1.25, 1.5, 1.75, 2.0, 0.0];

fn jas() -> f64 {
    let cist = crate::spolecne::cist_podsviceni;
    match (cist("brightness"), cist("max_brightness")) {
        (Some(j), Some(m)) if m > 0.0 => (j / m * 100.0).round(),
        _ => 50.0,
    }
}

/// Posuvník, který nastavuje se zpožděním (ať se neposílá stovka změn za sekundu).
fn posuvnik(od: f64, do_: f64, hodnota: f64, prodleva: u64, nastavit: impl Fn(f64) + 'static) -> gtk::Scale {
    let s = gtk::Scale::with_range(gtk::Orientation::Horizontal, od, do_, 1.0);
    s.set_value(hodnota);
    s.set_size_request(240, -1);
    s.set_draw_value(false);
    let casovac: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let nastavit = Rc::new(nastavit);
    s.connect_value_changed(move |s| {
        if let Some(id) = casovac.take() {
            id.remove();
        }
        let (v, c, n) = (s.value(), casovac.clone(), nastavit.clone());
        casovac.replace(Some(glib::timeout_add_local_once(Duration::from_millis(prodleva), move || {
            c.replace(None);
            n(v);
        })));
    });
    s
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Display");

    let karta = skupina(&s, "", "");
    radek(&karta, "\u{f185}", "Brightness", "", Some(posuvnik(1.0, 100.0, jas(), 30, crate::osd::nastavit_jas).upcast_ref()));

    // noční režim
    let conf = nocni::nacist();
    let rezim = conf.get("rezim").cloned().unwrap_or_else(|| "zapad".into());
    let teplota: f64 = conf.get("teplota").and_then(|t| t.parse().ok()).unwrap_or(4000.0);
    let karta = skupina(&s, "Night light", "Warmer colors in the evening are easier on the eyes and help you fall asleep.");
    radek_prepinac(&karta, "Night light", "", rezim != "vypnuto", |zap| {
        crate::spolecne::pozadi(&crate::spolecne::skripty().join("night-light.sh").to_string_lossy(), &["prepnout"]);
        super::oznamit(if zap { "Night light on" } else { "Night light off" });
        glib::timeout_add_local_once(Duration::from_millis(300), || super::obnovit("displej"));
    });
    if rezim != "vypnuto" {
        let kdy = gtk::DropDown::from_strings(&["Sunset to sunrise", "All the time"]);
        kdy.set_selected((rezim == "vzdy") as u32);
        kdy.connect_selected_notify(|d| nocni::ulozit("rezim", if d.selected() == 1 { "vzdy" } else { "zapad" }));
        let kde = crate::okenka::pocasi::poloha().map(|p| format!("Sunset and sunrise times for {}", p.0)).unwrap_or_else(|| "Set your location in Date & Time for sunset times".into());
        radek(&karta, "\u{f186}", "When", &kde, Some(kdy.upcast_ref()));
        // posuvník: vlevo 5500 K (skoro neutrální), vpravo 2500 K (hodně teplé)
        let t = posuvnik(0.0, 100.0, (5500.0 - teplota) / 30.0, 350, |v| {
            let k = ((5500.0 - v * 30.0) / 100.0).round() * 100.0;
            nocni::ulozit("teplota", &format!("{k}"));
        });
        radek(&karta, "\u{f2c9}", "Warmth", "", Some(t.upcast_ref()));
    }

    // obrazovky
    let obal = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.append(&obal);
    let o2 = obal.clone();
    na_pozadi(
        || serde_json::from_str::<Value>(&prikaz("swaymsg", &["-r", "-t", "get_outputs"])).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default(),
        move |vystupy| {
            if o2.root().is_none() {
                return;
            }
            let vice = vystupy.len() > 1;
            for v in &vystupy {
                obrazovka(&o2, v, vice);
            }
            if !vice {
                let k = skupina(&o2, "", "");
                radek(&k, "\u{f26c}", "External monitor", "Plug in a monitor over HDMI and it shows up here.", None);
            }
        },
    );
    s.upcast()
}

fn obrazovka(stranka: &gtk::Box, v: &Value, vice: bool) {
    let jmeno = v["name"].as_str().unwrap_or("").to_string();
    let vestavena = jmeno.starts_with("eDP");
    let nadpis = if vestavena {
        "Built-in display".to_string()
    } else {
        let n = format!("{} {}", v["make"].as_str().unwrap_or(""), v["model"].as_str().unwrap_or("")).replace("Unknown", "");
        if n.trim().is_empty() { jmeno.clone() } else { n.trim().to_string() }
    };
    let zapnuta = v["active"].as_bool().unwrap_or(true);
    let karta = skupina(stranka, &nadpis, "");
    if vice {
        let j = jmeno.clone();
        radek_prepinac(&karta, "Use this display", "", zapnuta, move |z| {
            sway_conf::nastavit(&format!("output {j}"), if z { "enable" } else { "disable" });
            glib::timeout_add_local_once(Duration::from_millis(800), || super::obnovit("displej"));
        });
        if !vestavena && zapnuta {
            let j = jmeno.clone();
            let zrcadlo = gtk::DropDown::from_strings(&["Right of the laptop", "Left of the laptop", "Above the laptop"]);
            zrcadlo.connect_selected_notify(move |d| umistit(&j, d.selected()));
            radek(&karta, "\u{f0db}", "Position", "", Some(zrcadlo.upcast_ref()));
        }
    }
    if !zapnuta {
        return;
    }

    // rozlišení a frekvence
    let rezimy: Vec<(i64, i64, i64)> = {
        let mut r: Vec<(i64, i64, i64)> = v["modes"].as_array().map(|m| m.iter().map(|m| (m["width"].as_i64().unwrap_or(0), m["height"].as_i64().unwrap_or(0), m["refresh"].as_i64().unwrap_or(0))).collect()).unwrap_or_default();
        r.sort_by(|a, b| (b.0 * b.1, b.2).cmp(&(a.0 * a.1, a.2)));
        r.dedup();
        r
    };
    let c = &v["current_mode"];
    let ted = (c["width"].as_i64().unwrap_or(0), c["height"].as_i64().unwrap_or(0), c["refresh"].as_i64().unwrap_or(0));
    if rezimy.len() > 1 {
        let texty: Vec<String> = rezimy.iter().enumerate().map(|(i, m)| format!("{} × {}  ·  {:.0} Hz{}", m.0, m.1, m.2 as f64 / 1000.0, if i == 0 { "  (best)" } else { "" })).collect();
        let d = gtk::DropDown::from_strings(&texty.iter().map(String::as_str).collect::<Vec<_>>());
        d.set_selected(rezimy.iter().position(|m| *m == ted).unwrap_or(0) as u32);
        let (j, r) = (jmeno.clone(), rezimy.clone());
        let puvodni = format!("{}x{}@{:.3}Hz", ted.0, ted.1, ted.2 as f64 / 1000.0);
        d.connect_selected_notify(move |d| {
            let m = r[d.selected() as usize];
            let novy = format!("{}x{}@{:.3}Hz", m.0, m.1, m.2 as f64 / 1000.0);
            zkusit(&format!("output {j} mode"), &novy, &puvodni);
        });
        radek(&karta, "\u{f108}", "Resolution", "", Some(d.upcast_ref()));
    } else {
        radek(&karta, "\u{f108}", "Resolution", &format!("{} × {}  ·  {:.0} Hz", ted.0, ted.1, ted.2 as f64 / 1000.0), None);
    }

    // měřítko
    let meritko = v["scale"].as_f64().unwrap_or(1.0);
    let mut volby: Vec<String> = MERITKA[..5].iter().map(|m| format!("{:.0} %", m * 100.0)).collect();
    let mut hodnoty: Vec<f64> = MERITKA[..5].to_vec();
    if !hodnoty.iter().any(|m| (m - meritko).abs() < 0.01) {
        volby.push(format!("{:.0} %", meritko * 100.0));
        hodnoty.push(meritko);
    }
    let d = gtk::DropDown::from_strings(&volby.iter().map(String::as_str).collect::<Vec<_>>());
    d.set_selected(hodnoty.iter().position(|m| (m - meritko).abs() < 0.01).unwrap_or(0) as u32);
    let j = jmeno.clone();
    d.connect_selected_notify(move |d| {
        let m = hodnoty[d.selected() as usize];
        zkusit(&format!("output {j} scale"), &format!("{m}"), &format!("{meritko}"));
    });
    radek(&karta, "\u{f00e}", "Scale", "Makes everything bigger. For just bigger text, see Appearance → Text size.", Some(d.upcast_ref()));
}

/// Druhý monitor vedle notebooku (pozice podle šířky vestavěné obrazovky).
fn umistit(jmeno: &str, kam: u32) {
    let vystupy: Vec<Value> = serde_json::from_str::<Value>(&prikaz("swaymsg", &["-r", "-t", "get_outputs"])).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let rect = |n: &Value| (n["rect"]["width"].as_i64().unwrap_or(1920), n["rect"]["height"].as_i64().unwrap_or(1080));
    let Some(notebook) = vystupy.iter().find(|v| v["name"].as_str().is_some_and(|n| n.starts_with("eDP"))) else { return };
    let Some(monitor) = vystupy.iter().find(|v| v["name"].as_str() == Some(jmeno)) else { return };
    let ((nw, _), (mw, mh)) = (rect(notebook), rect(monitor));
    let nj = notebook["name"].as_str().unwrap_or("eDP-1");
    let (n_pos, m_pos) = match kam {
        1 => (format!("{mw} 0"), "0 0".to_string()),
        2 => (format!("0 {mh}"), "0 0".to_string()),
        _ => ("0 0".to_string(), format!("{nw} 0")),
    };
    sway_conf::nastavit(&format!("output {nj} position"), &n_pos);
    sway_conf::nastavit(&format!("output {jmeno} position"), &m_pos);
    super::oznamit("Saved");
}

/// Použije nastavení obrazovky hned, ale uloží ho, jen když uživatel do 15 s potvrdí.
fn zkusit(klic: &str, novy: &str, puvodni: &str) {
    crate::sway::prikaz(&format!("{klic} {novy}"));
    let zruseni = gtk::gio::Cancellable::new();
    let z2 = zruseni.clone();
    let casovac = glib::timeout_add_local_once(Duration::from_secs(15), move || z2.cancel());
    let dialog = gtk::AlertDialog::builder()
        .message("Keep these display settings?")
        .detail("They go back in 15 seconds if you don't choose.")
        .buttons(["Go back", "Keep"])
        .default_button(1)
        .cancel_button(0)
        .modal(true)
        .build();
    let okno = super::okno();
    let (k, n, p) = (klic.to_string(), novy.to_string(), puvodni.to_string());
    let casovac = RefCell::new(Some(casovac));
    dialog.choose(okno.as_ref(), Some(&zruseni), move |v| {
        if let Some(c) = casovac.take() {
            if !zruseni_vyprselo(&v) {
                c.remove();
            }
        }
        if v == Ok(1) {
            sway_conf::nastavit(&k, &n);
            super::oznamit("Saved");
        } else {
            crate::sway::prikaz(&format!("{k} {p}"));
            super::oznamit("Went back to the previous settings");
        }
        glib::timeout_add_local_once(Duration::from_millis(500), || super::obnovit("displej"));
    });
}

/// Dialog skončil zrušením po 15 s (časovač už doběhl, nesmí se odstraňovat znovu).
fn zruseni_vyprselo(v: &Result<i32, glib::Error>) -> bool {
    matches!(v, Err(e) if e.matches(gtk::gio::IOErrorEnum::Cancelled))
}
