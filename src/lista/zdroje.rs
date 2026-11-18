//! Data modulů, která dřív zjišťoval sám waybar: hodiny, hlasitost, jas, síť a baterie.
//! Všechno se posílá do panel::publikovat jako ostatní moduly. Nic se nekontroluje dokola:
//! hlasitost hlásí `pactl subscribe` (vlákno v mikrofon.rs), síť NetworkManager a baterii UPower
//! přes D-Bus; jen jas se jednou za 5 s přečte ze /sys (mění ho hlavně klávesy, ty ho hlásí hned).

use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

use gtk::gio;
use gtk::glib;

use serde_json::{json, Value};

use crate::panel::{escapovat, publikovat};
use crate::spolecne::*;

pub fn spustit() {
    hodiny();
    hlasitost_vlakno();
    jas_do_panelu();
    glib::timeout_add_seconds_local(5, || {
        jas_do_panelu();
        glib::ControlFlow::Continue
    });
    sit();
    baterie_sledovat();
    publikovat("vypnout", json!({"text": "\u{f011}"}));
}

/// Formát hodin podle Settings (`hodiny` v nastaveni.json: datum, sekundy, h12).
pub fn format_hodin() -> String {
    let h = crate::spolecne::nastaveni("hodiny");
    let datum = h["datum"].as_bool().unwrap_or(true);
    let sekundy = h["sekundy"].as_bool().unwrap_or(true);
    let cas = match (h["h12"].as_bool().unwrap_or(false), sekundy) {
        (false, true) => "%H:%M:%S",
        (false, false) => "%H:%M",
        (true, true) => "%-I:%M:%S %p",
        (true, false) => "%-I:%M %p",
    };
    if datum { format!("%a %e %B   {cas}") } else { cas.to_string() }
}

thread_local! {
    static FORMAT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Ukáže čas hned (i po změně formátu v Settings).
pub fn hodiny_ted() -> chrono::DateTime<chrono::Local> {
    let ted = chrono::Local::now();
    let f = FORMAT.with(|f| f.borrow_mut().get_or_insert_with(format_hodin).clone());
    publikovat("hodiny", json!({"text": ted.format(&f).to_string()}));
    ted
}

pub fn zmenit_format() {
    FORMAT.with(|f| f.replace(None));
    hodiny_ted();
}

/// Hodiny každou sekundu, vždy na začátku sekundy (časovač by jinak ujížděl).
fn hodiny() {
    let ted = hodiny_ted();
    let do_dalsi = 1000 - ted.timestamp_subsec_millis().min(999) as u64;
    glib::timeout_add_local_once(Duration::from_millis(do_dalsi + 2), hodiny);
}

static HLASITOST: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// Volá vlákno s `pactl subscribe`, když se změnil výstup zvuku nebo výchozí zařízení.
pub fn hlasitost_zmenena() {
    if let Some(tx) = HLASITOST.get() {
        let _ = tx.send(());
    }
}

/// Hlasitost výchozího výstupu; víc změn rychle za sebou (posuvník) se zjistí jednou.
fn hlasitost_vlakno() {
    let (tx, rx) = mpsc::channel::<()>();
    let _ = HLASITOST.set(tx);
    std::thread::spawn(move || {
        let mut prvni = true;
        loop {
            if !prvni && rx.recv().is_err() {
                return;
            }
            prvni = false;
            std::thread::sleep(Duration::from_millis(30));
            while rx.try_recv().is_ok() {}
            let v = soucasne(&[("pactl", &["-f", "json", "info"]), ("pactl", &["-f", "json", "list", "sinks"])]);
            let info: Value = serde_json::from_str(&v[0]).unwrap_or_default();
            let vychozi = info["default_sink_name"].as_str().unwrap_or("");
            let sinks: Vec<Value> = serde_json::from_str(&v[1]).unwrap_or_default();
            let Some(s) = sinks.iter().find(|s| s["name"].as_str() == Some(vychozi)) else { continue };
            let procenta = s["volume"]
                .as_object()
                .and_then(|k| k.values().next())
                .and_then(|k| k["value_percent"].as_str())
                .and_then(|p| p.trim_end_matches('%').parse::<f64>().ok())
                .unwrap_or(0.0);
            if s["mute"].as_bool().unwrap_or(false) {
                publikovat("zvuk", json!({"text": "\u{f6a9}  muted", "class": "muted"}));
            } else {
                let ikona = if procenta < 33.4 { "\u{f026}" } else if procenta < 66.7 { "\u{f027}" } else { "\u{f028}" };
                publikovat("zvuk", json!({"text": format!("{ikona}  {procenta:.0} %")}));
            }
        }
    });
}

/// Jas ze /sys (podsvícení se čte rychle, na rozdíl od baterie).
pub fn jas_do_panelu() {
    let cti = crate::spolecne::cist_podsviceni;
    if let (Some(j), Some(max)) = (cti("brightness"), cti("max_brightness")) {
        publikovat("jas", json!({"text": format!("\u{f185}  {:.0} %", j / max.max(1.0) * 100.0)}));
    }
}

fn systemova() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok()
}

/// Síť: název Wi-Fi, „wired“ nebo „offline“ (NetworkManager, při každé změně).
fn sit() {
    sit_do_panelu();
    let Some(s) = systemova() else { return };
    let odber = s.subscribe_to_signal(
        Some("org.freedesktop.NetworkManager"),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some("/org/freedesktop/NetworkManager"),
        None,
        gio::DBusSignalFlags::NONE,
        |_| sit_do_panelu(),
    );
    std::mem::forget(odber); // po celou dobu běhu démona
}

fn sit_do_panelu() {
    let primarni = dbus_vlastnost(true, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager", "PrimaryConnection")
        .and_then(|v| v.str().map(String::from))
        .unwrap_or_else(|| "/".into());
    if primarni == "/" {
        publikovat("sit", json!({"text": "\u{f127}  offline", "class": "disconnected"}));
        return;
    }
    let m = dbus_vse(true, "org.freedesktop.NetworkManager", &primarni, "org.freedesktop.NetworkManager.Connection.Active");
    let text = |k: &str| m.get(k).and_then(|v| v.str().map(String::from)).unwrap_or_default();
    let typ = text("Type");
    if typ.contains("wireless") {
        publikovat("sit", json!({"text": format!("\u{f1eb}  {}", escapovat(&text("Id")))}));
    } else if typ.contains("ethernet") {
        publikovat("sit", json!({"text": "\u{f6ff}  wired"}));
    } else {
        publikovat("sit", json!({"text": format!("\u{f0c1}  {}", escapovat(&text("Id")))}));
    }
}

/// Baterie: UPower hlásí každou změnu nabití i stavu.
fn baterie_sledovat() {
    baterie_do_panelu();
    let Some(s) = systemova() else { return };
    let odber = s.subscribe_to_signal(
        Some("org.freedesktop.UPower"),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some("/org/freedesktop/UPower/devices/battery_BAT0"),
        None,
        gio::DBusSignalFlags::NONE,
        |_| baterie_do_panelu(),
    );
    std::mem::forget(odber);
}

fn baterie_do_panelu() {
    let Some(b) = baterie() else { return };
    let p = b.procenta.round();
    let (ikona, mut trida) = match b.stav {
        1 => ("\u{f0e7}", "charging"),
        4 | 5 => ("\u{f1e6}", "plugged"),
        _ => (["\u{f244}", "\u{f243}", "\u{f242}", "\u{f241}", "\u{f240}"][((p / 20.0) as usize).min(4)], ""),
    };
    if b.stav != 1 {
        if p <= 10.0 {
            trida = if trida.is_empty() { "critical" } else { trida };
        } else if p <= 25.0 && trida.is_empty() {
            trida = "warning";
        }
    }
    publikovat("baterie", json!({"text": format!("{ikona}  {p:.0} %"), "class": trida}));
}
