//! Weather: bar module (icon and temperature) and popup (now, the next hours and the week).
//! Data from Open-Meteo (no registration) for the location set in Settings. Downloaded when
//! older than 15 minutes (checked once a minute, so also right after waking from sleep).
//! When the network is down, the bar shows the last known state dimmed.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, SystemTime};

use gtk::prelude::*;
use serde_json::Value;

use crate::common::*;

/// Location for the forecast (Settings → Date & Time → Location): (name, latitude, longitude).
pub fn location() -> Option<(String, f64, f64)> {
    let p = crate::common::settings("location");
    Some((p["name"].as_str()?.to_string(), p["lat"].as_f64()?, p["lon"].as_f64()?))
}

/// A new location from Settings: drop the old forecast and download a new one.
pub fn change_location() {
    let _ = std::fs::remove_file(data_file());
    to_panel(None, false);
    refresh();
}
const VALIDITY: Duration = Duration::from_secs(900);

/// WMO weather code → (description, day icon, night icon) – Font Awesome 6 icons
fn code(k: i64) -> (&'static str, &'static str, &'static str) {
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

fn icon_code(k: &Value, day: bool) -> &'static str {
    let (_, d, n) = code(k.as_i64().unwrap_or(-1));
    if day { d } else { n }
}

fn data_file() -> std::path::PathBuf {
    run_dir().join("weather-data.json")
}

fn load() -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(data_file()).ok()?).ok()
}

fn age_dat() -> Duration {
    std::fs::metadata(data_file())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .unwrap_or(Duration::MAX)
}

/// Downloads the forecast and saves it (right after waking or login the network may not be up yet: 3 tries).
fn download() -> Option<Value> {
    let (_, width, length) = location()?;
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={width}&longitude={length}\
         &current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m,is_day\
         &hourly=temperature_2m,weather_code,precipitation_probability,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,sunrise,sunset\
         &timezone=auto&forecast_days=7"
    );
    for pokus in 0..3 {
        let text = command("curl", &["-sf", "--max-time", "10", &url]);
        if let Ok(data) = serde_json::from_str::<Value>(&text) {
            if data["current"].is_object() {
                let _ = std::fs::write(data_file(), &text);
                return Some(data);
            }
        }
        if pokus < 2 {
            std::thread::sleep(Duration::from_secs(10));
        }
    }
    None
}

fn to_panel(data: Option<&Value>, current: bool) {
    let Some(d) = data else {
        crate::panel::publish("weather", serde_json::json!({"text": ""}));
        return;
    };
    let now = &d["current"];
    let text = format!("{}  {}°", icon_code(&now["weather_code"], now["is_day"].as_i64().unwrap_or(1) == 1), round_value(&now["temperature_2m"]));
    crate::panel::publish("weather", serde_json::json!({"text": text, "class": if current { "ok" } else { "stale" }}));
}

thread_local! {
    static STAHUJE: Cell<bool> = const { Cell::new(false) };
    static LAST_POKUS: Cell<Option<SystemTime>> = const { Cell::new(None) };
    /// the open weather popup, redrawn after the download
    static POPUP: RefCell<Weak<Popup>> = RefCell::new(Weak::new());
}

/// Downloads the weather in the background, then refreshes the bar and the open popup.
pub fn refresh() {
    if STAHUJE.with(|s| s.replace(true)) {
        return;
    }
    LAST_POKUS.with(|p| p.set(Some(SystemTime::now())));
    in_background(download, |data| {
        STAHUJE.with(|s| s.set(false));
        match &data {
            Some(d) => to_panel(Some(d), true),
            None => to_panel(load().as_ref(), false),
        }
        if let (Some(d), Some(o)) = (data, POPUP.with(|o| o.borrow().upgrade())) {
            if o.window.is_visible() {
                show(&o, &d);
            }
        }
    });
}

/// Bar: the last known state at once, then download whenever the data is older than 15 minutes
/// (after a failure, try again in 5 minutes at the earliest).
pub fn watch() {
    let age = age_dat();
    to_panel(load().as_ref(), age < VALIDITY);
    let check = || {
        let nedavno = LAST_POKUS.with(|p| p.get())
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|d| d < Duration::from_secs(300));
        if age_dat() >= VALIDITY && !nedavno {
            refresh();
        }
    };
    check();
    gtk::glib::timeout_add_seconds_local(60, move || {
        check();
        gtk::glib::ControlFlow::Continue
    });
}

pub fn fill(o: &Rc<Popup>) {
    if location().is_none() {
        o.panel.append(&title("Weather", ""));
        o.state("Set your location in Settings → Date & Time to see the weather");
        return;
    }
    match load() {
        Some(d) => show(o, &d),
        None => o.state("Loading forecast…"),
    }
    POPUP.with(|w| *w.borrow_mut() = Rc::downgrade(o));
    if age_dat() > VALIDITY {
        refresh();
    }
}

fn round_value(v: &Value) -> i64 {
    v.as_f64().unwrap_or(0.0).round() as i64
}

fn show(o: &Rc<Popup>, data: &Value) {
    o.clear_content();
    let (now, hod, dny) = (&data["current"], &data["hourly"], &data["daily"]);
    let day_now = now["is_day"].as_i64().unwrap_or(1) == 1;

    // now
    let hore = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    hore.append(&icon(icon_code(&now["weather_code"], day_now), "weather-icon"));
    hore.append(&label(&format!("{}°", round_value(&now["temperature_2m"])), &["giant"]));
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_valign(gtk::Align::Center);
    texts.set_hexpand(true);
    texts.append(&label(code(now["weather_code"].as_i64().unwrap_or(-1)).0, &["title"]));
    texts.append(&label(&format!("{} · feels like {}°", location().map(|p| p.0).unwrap_or_default(), round_value(&now["apparent_temperature"])), &["dim"]));
    texts.append(&label(
        &format!("Today {}° / {}°", round_value(&dny["temperature_2m_min"][0]), round_value(&dny["temperature_2m_max"][0])),
        &["dim"],
    ));
    hore.append(&texts);
    o.panel.append(&hore);

    let time = |v: &Value| v.as_str().map(|s| s.get(s.len().saturating_sub(5)..).unwrap_or("").to_string()).unwrap_or_default();
    o.panel.append(&label(
        &format!(
            "\u{f043}  {} %     \u{f72e}  {} km/h     \u{f185}  {}     \u{f186}  {}",
            now["relative_humidity_2m"].as_i64().unwrap_or(0),
            round_value(&now["wind_speed_10m"]),
            time(&dny["sunrise"][0]),
            time(&dny["sunset"][0])
        ),
        &["dim"],
    ));
    o.panel.append(&separator());

    // the next hours, every second one
    let now_hodina: String = now["time"].as_str().unwrap_or("").chars().take(13).collect();
    let times = hod["time"].as_array().cloned().unwrap_or_default();
    let start = times.iter().position(|h| h.as_str().unwrap_or("") >= now_hodina.as_str()).unwrap_or(0);
    let clock = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    clock.set_homogeneous(true);
    for i in (start..(start + 16).min(times.len())).step_by(2) {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let description_time = if i == start { "Now".to_string() } else { times[i].as_str().unwrap_or("").get(11..16).unwrap_or("").to_string() };
        let l = label(&description_time, &["dim"]);
        l.set_xalign(0.5);
        column.append(&l);
        column.append(&icon(icon_code(&hod["weather_code"][i], hod["is_day"][i].as_i64().unwrap_or(1) == 1), "small-icon"));
        let t = label(&format!("{}°", round_value(&hod["temperature_2m"][i])), &[]);
        t.set_xalign(0.5);
        column.append(&t);
        let dest = hod["precipitation_probability"][i].as_i64().unwrap_or(0);
        let d = label(&if dest >= 20 { format!("{dest} %") } else { " ".into() }, &["dest"]);
        d.set_xalign(0.5);
        column.append(&d);
        clock.append(&column);
    }
    o.panel.append(&clock);
    o.panel.append(&separator());

    // week: day, icon, rain, lowest, bar, highest
    let minima: Vec<f64> = dny["temperature_2m_min"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let maxima: Vec<f64> = dny["temperature_2m_max"].as_array().map(|a| a.iter().filter_map(Value::as_f64).collect()).unwrap_or_default();
    let lowest = minima.iter().cloned().fold(f64::MAX, f64::min);
    let highest = maxima.iter().cloned().fold(f64::MIN, f64::max);
    let grid = gtk::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(8);
    for (i, day) in dny["time"].as_array().cloned().unwrap_or_default().iter().enumerate() {
        let name = if i == 0 {
            "Today".to_string()
        } else {
            chrono::NaiveDate::parse_from_str(day.as_str().unwrap_or(""), "%Y-%m-%d").map(|d| d.format("%a").to_string()).unwrap_or_default()
        };
        let n = label(&name, &[]);
        n.set_width_chars(6);
        grid.attach(&n, 0, i as i32, 1, 1);
        grid.attach(&icon(icon_code(&dny["weather_code"][i], true), "small-icon"), 1, i as i32, 1, 1);
        let dest = dny["precipitation_probability_max"][i].as_i64().unwrap_or(0);
        let d = label(&if dest >= 20 { format!("{dest} %") } else { String::new() }, &["dest"]);
        d.set_width_chars(4);
        grid.attach(&d, 2, i as i32, 1, 1);
        let (from, doo) = (minima.get(i).copied().unwrap_or(0.0), maxima.get(i).copied().unwrap_or(0.0));
        let mn = label(&format!("{}°", from.round() as i64), &["dim"]);
        mn.set_xalign(1.0);
        mn.set_width_chars(3);
        grid.attach(&mn, 3, i as i32, 1, 1);
        grid.attach(&pruh_teplot(from, doo, lowest, highest), 4, i as i32, 1, 1);
        let mx = label(&format!("{}°", doo.round() as i64), &[]);
        mx.set_xalign(1.0);
        mx.set_width_chars(3);
        grid.attach(&mx, 5, i as i32, 1, 1);
    }
    o.panel.append(&grid);
    o.panel.append(&separator());
    o.panel.append(&link(o, "Open the Weather app", "gnome-weather", &[]));
    o.recompute();
}

/// A bar like on the iPhone: where the day's temperatures lie in the range of the whole week.
fn pruh_teplot(from: f64, doo: f64, lowest: f64, highest: f64) -> gtk::DrawingArea {
    let palette = crate::style::Palette::load();
    let (base, fill) = (palette.rgb("mantle"), palette.rgb("accent"));
    let workspace = gtk::DrawingArea::new();
    workspace.set_content_width(120);
    workspace.set_content_height(8);
    workspace.set_valign(gtk::Align::Center);
    workspace.set_draw_func(move |_, cr, width, height| {
        let (width, height) = (width as f64, height as f64);
        let range = (highest - lowest).max(1.0);
        let rounded = |x: f64, s: f64, (r, g, b): (f64, f64, f64)| {
            let radius = height / 2.0;
            cr.set_source_rgb(r, g, b);
            cr.new_sub_path();
            cr.arc(x + radius, radius, radius, std::f64::consts::FRAC_PI_2, 3.0 * std::f64::consts::FRAC_PI_2);
            cr.arc(x + s - radius, radius, radius, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
            cr.close_path();
            let _ = cr.fill();
        };
        rounded(0.0, width, base);
        let x1 = (from - lowest) / range * width;
        let x2 = (doo - lowest) / range * width;
        rounded(x1, (x2 - x1).max(height), fill);
    });
    workspace
}
