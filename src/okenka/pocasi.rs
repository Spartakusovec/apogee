//! Počasí: modul v panelu (ikona a teplota) a okénko (teď, následující hodiny a týden).
//! Data z Open-Meteo (bez registrace) pro Ostravu, jako v GNOME Počasí. Stahují se, když jsou
//! starší než 15 minut (kontrola jednou za minutu, takže i hned po probuzení z uspání).
//! Když síť nejde, panel ukáže poslední známý stav ztlumeně.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, SystemTime};

use gtk::prelude::*;
use serde_json::Value;

use crate::spolecne::*;

/// Poloha pro předpověď (Settings → Date & Time → Location): (název, zeměpisná šířka, délka).
pub fn poloha() -> Option<(String, f64, f64)> {
    let p = crate::spolecne::nastaveni("poloha");
    Some((p["nazev"].as_str()?.to_string(), p["sirka"].as_f64()?, p["delka"].as_f64()?))
}

/// Nová poloha z Settings: starou předpověď zahodit a stáhnout novou.
pub fn zmenit_polohu() {
    let _ = std::fs::remove_file(data_soubor());
    do_panelu(None, false);
    obnovit();
}
const PLATNOST: Duration = Duration::from_secs(900);

/// kód počasí WMO → (popis, ikona ve dne, ikona v noci) – ikony Font Awesome 6
fn kod(k: i64) -> (&'static str, &'static str, &'static str) {
    match k {
        0 => ("Clear sky", "\u{f185}", "\u{f186}"),
        1 => ("Mainly clear", "\u{f6c4}", "\u{f6c3}"),
        2 => ("Partly cloudy", "\u{f6c4}", "\u{f6c3}"),
        3 => ("Overcast", "\u{f0c2}", "\u{f0c2}"),
        45 => ("Fog", "\u{f75f}", "\u{f75f}"),
        48 => ("Freezing fog", "\u{f75f}", "\u{f75f}"),
        51 => ("Light drizzle", "\u{f743}", "\u{f73c}"),
        53 => ("Drizzle", "\u{f73d}", "\u{f73d}"),
        55 => ("Heavy drizzle", "\u{f73d}", "\u{f73d}"),
        56 | 57 => ("Freezing drizzle", "\u{f73d}", "\u{f73d}"),
        61 => ("Light rain", "\u{f743}", "\u{f73c}"),
        63 => ("Rain", "\u{f73d}", "\u{f73d}"),
        65 => ("Heavy rain", "\u{f740}", "\u{f740}"),
        66 => ("Freezing rain", "\u{f73d}", "\u{f73d}"),
        67 => ("Freezing rain", "\u{f740}", "\u{f740}"),
        71 => ("Light snow", "\u{f2dc}", "\u{f2dc}"),
        73 => ("Snow", "\u{f2dc}", "\u{f2dc}"),
        75 => ("Heavy snow", "\u{f2dc}", "\u{f2dc}"),
        77 => ("Snow grains", "\u{f2dc}", "\u{f2dc}"),
        80 => ("Rain showers", "\u{f743}", "\u{f73c}"),
        81 => ("Rain showers", "\u{f73d}", "\u{f73d}"),
        82 => ("Heavy showers", "\u{f740}", "\u{f740}"),
        85 | 86 => ("Snow showers", "\u{f2dc}", "\u{f2dc}"),
        95 => ("Thunderstorm", "\u{f76c}", "\u{f76c}"),
        96 | 99 => ("Thunderstorm with hail", "\u{f76c}", "\u{f76c}"),
        _ => ("", "\u{f0c2}", "\u{f0c2}"),
    }
}

fn ikona_kodu(k: &Value, den: bool) -> &'static str {
    let (_, d, n) = kod(k.as_i64().unwrap_or(-1));
    if den { d } else { n }
}

fn data_soubor() -> std::path::PathBuf {
    run_dir().join("pocasi-data.json")
}

fn nacist() -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(data_soubor()).ok()?).ok()
}

fn stari_dat() -> Duration {
    std::fs::metadata(data_soubor())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .unwrap_or(Duration::MAX)
}

/// Stáhne předpověď a uloží ji (po probuzení nebo přihlášení ještě nemusí jet síť: 3 pokusy).
fn stahnout() -> Option<Value> {
    let (_, sirka, delka) = poloha()?;
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={sirka}&longitude={delka}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day\
         &hourly=temperature_2m,weather_code,precipitation_probability,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset\
         &timezone=auto&forecast_days=7"
    );
    for pokus in 0..3 {
        let text = prikaz("curl", &["-sf", "--max-time", "10", &url]);
        if let Ok(data) = serde_json::from_str::<Value>(&text) {
            if data["current"].is_object() {
                let _ = std::fs::write(data_soubor(), &text);
                return Some(data);
            }
        }
        if pokus < 2 {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    None
}

fn do_panelu(data: Option<&Value>, aktualni: bool) {
    let Some(d) = data else {
        crate::panel::publikovat("pocasi", serde_json::json!({"text": ""}));
        return;
    };
    let ted = &d["current"];
    let text = format!("{}  {}°", ikona_kodu(&ted["weather_code"], ted["is_day"].as_i64().unwrap_or(1) == 1), zaokrouhlit(&ted["temperature_2m"]));
    crate::panel::publikovat("pocasi", serde_json::json!({"text": text, "class": if aktualni { "ok" } else { "stale" }}));
}

thread_local! {
    static STAHUJE: Cell<bool> = const { Cell::new(false) };
    static POSLEDNI_POKUS: Cell<Option<SystemTime>> = const { Cell::new(None) };
    /// otevřené okénko počasí, překreslí se po stažení
    static OKENKO: RefCell<Weak<Okenko>> = RefCell::new(Weak::new());
}

/// Stáhne počasí na pozadí, pak obnoví panel a otevřené okénko.
pub fn obnovit() {
    if STAHUJE.with(|s| s.replace(true)) {
        return;
    }
    POSLEDNI_POKUS.with(|p| p.set(Some(SystemTime::now())));
    na_pozadi(stahnout, |data| {
        STAHUJE.with(|s| s.set(false));
        match &data {
            Some(d) => do_panelu(Some(d), true),
            None => do_panelu(nacist().as_ref(), false),
        }
        if let (Some(d), Some(o)) = (data, OKENKO.with(|o| o.borrow().upgrade())) {
            if o.okno.is_visible() {
                ukazat(&o, &d);
            }
        }
    });
}

/// Panel: hned poslední známý stav, pak stahovat, kdykoli jsou data starší než 15 minut
/// (po neúspěchu zkusit znovu nejdřív za 5 minut).
pub fn sledovat() {
    let stari = stari_dat();
    do_panelu(nacist().as_ref(), stari < PLATNOST);
    let kontrola = || {
        let nedavno = POSLEDNI_POKUS.with(|p| p.get())
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|d| d < Duration::from_secs(300));
        if stari_dat() >= PLATNOST && !nedavno {
            obnovit();
        }
    };
    kontrola();
    gtk::glib::timeout_add_seconds_local(60, move || {
        kontrola();
        gtk::glib::ControlFlow::Continue
    });
}

pub fn naplnit(o: &Rc<Okenko>) {
    if poloha().is_none() {
        o.panel.append(&nadpis("Weather", ""));
        o.stav("Set your location in Settings → Date & Time to see the weather");
        return;
    }
    match nacist() {
        Some(d) => ukazat(o, &d),
        None => o.stav("Loading forecast…"),
    }
    OKENKO.with(|w| *w.borrow_mut() = Rc::downgrade(o));
    if stari_dat() > PLATNOST {
        obnovit();
    }
}

fn zaokrouhlit(v: &Value) -> i64 {
    v.as_f64().unwrap_or(0.0).round() as i64
}

fn ukazat(o: &Rc<Okenko>, data: &Value) {
    o.smazat_obsah();
    let (ted, hod, dny) = (&data["current"], &data["hourly"], &data["daily"]);
    let den_ted = ted["is_day"].as_i64().unwrap_or(1) == 1;

    // teď
    let hore = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    hore.append(&ikona(ikona_kodu(&ted["weather_code"], den_ted), "pocasi-ikona"));
    hore.append(&popisek(&format!("{}°", zaokrouhlit(&ted["temperature_2m"])), &["obri"]));
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texty.set_valign(gtk::Align::Center);
    texty.set_hexpand(true);
    texty.append(&popisek(kod(ted["weather_code"].as_i64().unwrap_or(-1)).0, &["nadpis"]));
    texty.append(&popisek(&format!("{} · feels like {}°", poloha().map(|p| p.0).unwrap_or_default(), zaokrouhlit(&ted["apparent_temperature"])), &["dim"]));
    texty.append(&popisek(
        &format!("Today {}° / {}°", zaokrouhlit(&dny["temperature_2m_min"][0]), zaokrouhlit(&dny["temperature_2m_max"][0])),
        &["dim"],
    ));
    hore.append(&texty);
    o.panel.append(&hore);

    let cas = |v: &Value| v.as_str().map(|s| s.get(s.len().saturating_sub(5)..).unwrap_or("").to_string()).unwrap_or_default();
    o.panel.append(&popisek(
        &format!(
            "\u{f043}  {} %     \u{f72e}  {} km/h     \u{f185}  {}     \u{f186}  {}",
            ted["relative_humidity_2m"].as_i64().unwrap_or(0),
            zaokrouhlit(&ted["wind_speed_10m"]),
            cas(&dny["sunrise"][0]),
            cas(&dny["sunset"][0])
        ),
        &["dim"],
    ));
    o.panel.append(&oddelovac());

    // následující hodiny, každá druhá
    let ted_hodina: String = ted["time"].as_str().unwrap_or("").chars().take(13).collect();
    let casy = hod["time"].as_array().cloned().unwrap_or_default();
    let start = casy.iter().position(|h| h.as_str().unwrap_or("") >= ted_hodina.as_str()).unwrap_or(0);
    let hodiny = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hodiny.set_homogeneous(true);
    for i in (start..(start + 16).min(casy.len())).step_by(2) {
        let sloupec = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let popis_casu = if i == start { "Now".to_string() } else { casy[i].as_str().unwrap_or("").get(11..16).unwrap_or("").to_string() };
        let l = popisek(&popis_casu, &["dim"]);
        l.set_xalign(0.5);
        sloupec.append(&l);
        sloupec.append(&ikona(ikona_kodu(&hod["weather_code"][i], hod["is_day"][i].as_i64().unwrap_or(1) == 1), "mala-ikona"));
        let t = popisek(&format!("{}°", zaokrouhlit(&hod["temperature_2m"][i])), &[]);
        t.set_xalign(0.5);
        sloupec.append(&t);
        let dest = hod["precipitation_probability"][i].as_i64().unwrap_or(0);
        let d = popisek(&if dest >= 20 { format!("{dest} %") } else { " ".into() }, &["dest"]);
        d.set_xalign(0.5);
        sloupec.append(&d);
        hodiny.append(&sloupec);
    }
    o.panel.append(&hodiny);
    o.panel.append(&oddelovac());

    // týden: den, ikona, déšť, nejnižší, pruh, nejvyšší
    let minima: Vec<f64> = dny["temperature_2m_min"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let maxima: Vec<f64> = dny["temperature_2m_max"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let nejnizsi = minima.iter().cloned().fold(f64::MAX, f64::min);
    let nejvyssi = maxima.iter().cloned().fold(f64::MIN, f64::max);
    let mrizka = gtk::Grid::new();
    mrizka.set_column_spacing(12);
    mrizka.set_row_spacing(8);
    for (i, den) in dny["time"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let nazev = if i == 0 {
            "Today".to_string()
        } else {
            chrono::NaiveDate::parse_from_str(den.as_str().unwrap_or(""), "%Y-%m-%d").map(|d| d.format("%a").to_string()).unwrap_or_default()
        };
        let n = popisek(&nazev, &[]);
        n.set_width_chars(6);
        mrizka.attach(&n, 0, i as i32, 1, 1);
        mrizka.attach(&ikona(ikona_kodu(&dny["weather_code"][i], true), "mala-ikona"), 1, i as i32, 1, 1);
        let dest = dny["precipitation_probability_max"][i].as_i64().unwrap_or(0);
        let d = popisek(&if dest >= 20 { format!("{dest} %") } else { String::new() }, &["dest"]);
        d.set_width_chars(4);
        mrizka.attach(&d, 2, i as i32, 1, 1);
        let (od, doo) = (minima.get(i).copied().unwrap_or(0.0), maxima.get(i).copied().unwrap_or(0.0));
        let mn = popisek(&format!("{}°", od.round() as i64), &["dim"]);
        mn.set_xalign(1.0);
        mn.set_width_chars(3);
        mrizka.attach(&mn, 3, i as i32, 1, 1);
        mrizka.attach(&pruh_teplot(od, doo, nejnizsi, nejvyssi), 4, i as i32, 1, 1);
        let mx = popisek(&format!("{}°", doo.round() as i64), &[]);
        mx.set_xalign(1.0);
        mx.set_width_chars(3);
        mrizka.attach(&mx, 5, i as i32, 1, 1);
    }
    o.panel.append(&mrizka);
    o.panel.append(&oddelovac());
    o.panel.append(&odkaz(o, "Open the Weather app", "gnome-weather", &[]));
    o.prepocitat();
}

/// Pruh jako na iPhonu: kde v rozpětí celého týdne leží teploty daného dne.
fn pruh_teplot(od: f64, doo: f64, nejnizsi: f64, nejvyssi: f64) -> gtk::DrawingArea {
    let paleta = crate::styl::Paleta::nacist();
    let (podklad, vypln) = (paleta.rgb("mantle"), paleta.rgb("accent"));
    let plocha = gtk::DrawingArea::new();
    plocha.set_content_width(120);
    plocha.set_content_height(8);
    plocha.set_valign(gtk::Align::Center);
    plocha.set_draw_func(move |_, cr, sirka, vyska| {
        let (sirka, vyska) = (sirka as f64, vyska as f64);
        let rozpeti = (nejvyssi - nejnizsi).max(1.0);
        let zaobleny = |x: f64, s: f64, (r, g, b): (f64, f64, f64)| {
            let polomer = vyska / 2.0;
            cr.set_source_rgb(r, g, b);
            cr.new_sub_path();
            cr.arc(x + polomer, polomer, polomer, std::f64::consts::FRAC_PI_2, 3.0 * std::f64::consts::FRAC_PI_2);
            cr.arc(x + s - polomer, polomer, polomer, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
            cr.close_path();
            let _ = cr.fill();
        };
        zaobleny(0.0, sirka, podklad);
        let x1 = (od - nejnizsi) / rozpeti * sirka;
        let x2 = (doo - nejnizsi) / rozpeti * sirka;
        zaobleny(x1, (x2 - x1).max(vyska), vypln);
    });
    plocha
}
