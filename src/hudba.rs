//! Hudba přes MPRIS (D-Bus, bez playerctl): co hraje, moduly panelu (skladba, tlačítka ⏮ ⏭),
//! ovládání (panel, klávesy) a data pro okénko přehrávače.
//!
//! Ovládá se přehrávač, který naposledy začal hrát; když nic nehraje, ten naposledy pozastavený.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk::gio;
use gtk::glib::{self, Variant};
use serde_json::json;

use crate::panel::{escapovat, publikovat};

const PREDPONA: &str = "org.mpris.MediaPlayer2.";
const CESTA: &str = "/org/mpris/MediaPlayer2";
const ROZHRANI: &str = "org.mpris.MediaPlayer2.Player";
const DELKA_TEXTU: usize = 45;

#[derive(Clone, Default)]
pub struct Prehravac {
    /// název na sběrnici (org.mpris.MediaPlayer2.vlc)
    pub jmeno: String,
    /// unikátní název (:1.23), od něj chodí signály
    vlastnik: String,
    /// Playing, Paused, Stopped
    pub stav: String,
    pub titulek: String,
    pub umelec: String,
    pub album: String,
    /// mpris:artUrl (file:// nebo https://)
    pub obal: String,
    /// v mikrosekundách, 0 = neznámá
    pub delka: i64,
    stopa: String,
    pub muze_predchozi: bool,
    pub muze_dalsi: bool,
    pub muze_pretacet: bool,
    /// název aplikace (VLC media player, Firefox…)
    pub identita: String,
}

thread_local! {
    /// od naposledy spuštěného
    static PREHRAVACE: RefCell<Vec<Prehravac>> = const { RefCell::new(Vec::new()) };
    static SBERNICE: RefCell<Option<gio::DBusConnection>> = const { RefCell::new(None) };
    static ODBERY: RefCell<Vec<gio::SignalSubscription>> = const { RefCell::new(Vec::new()) };
    /// otevřené okénko přehrávače se překreslí
    static PRI_ZMENE: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

fn je_prehravac(jmeno: &str) -> bool {
    jmeno.starts_with(PREDPONA) && !jmeno.ends_with(".playerctld")
}

fn sbernice() -> Option<gio::DBusConnection> {
    SBERNICE.with(|s| s.borrow().clone())
}

fn volat(cil: &str, rozhrani: &str, metoda: &str, parametry: Option<Variant>, hotovo: impl FnOnce(Option<Variant>) + 'static) {
    let Some(s) = sbernice() else { return };
    s.call(
        Some(cil),
        CESTA,
        rozhrani,
        metoda,
        parametry.as_ref(),
        None,
        gio::DBusCallFlags::NONE,
        2000,
        gio::Cancellable::NONE,
        move |v| hotovo(v.ok()),
    );
}

/// Začne sledovat přehrávače (při startu démona).
pub fn spustit() {
    let Ok(s) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else { return };
    SBERNICE.with(|b| b.replace(Some(s.clone())));
    let odbery = vec![
        // přehrávač se spustil nebo skončil
        s.subscribe_to_signal(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            None,
            gio::DBusSignalFlags::NONE,
            |sig| {
                let Some((jmeno, _, novy)) = sig.parameters.get::<(String, String, String)>() else { return };
                if !je_prehravac(&jmeno) {
                    return;
                }
                PREHRAVACE.with(|p| p.borrow_mut().retain(|x| x.jmeno != jmeno));
                if novy.is_empty() {
                    obnovit();
                } else {
                    pridat(jmeno, novy);
                }
            },
        ),
        // změnil se stav, skladba…
        s.subscribe_to_signal(
            None,
            Some("org.freedesktop.DBus.Properties"),
            Some("PropertiesChanged"),
            Some(CESTA),
            Some(ROZHRANI),
            gio::DBusSignalFlags::NONE,
            |sig| {
                let Some((_, zmeny, zneplatnene)) = sig.parameters.get::<(String, HashMap<String, Variant>, Vec<String>)>() else {
                    return;
                };
                let jmeno = PREHRAVACE.with(|p| p.borrow().iter().find(|x| x.vlastnik == sig.sender_name).map(|x| x.jmeno.clone()));
                if let Some(jmeno) = jmeno {
                    zmenit(&jmeno, &zmeny);
                    if !zneplatnene.is_empty() {
                        nacist(&jmeno);
                    }
                }
            },
        ),
    ];
    ODBERY.with(|o| o.replace(odbery));
    // přehrávače, které už běží
    s.call(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListNames",
        None,
        None,
        gio::DBusCallFlags::NONE,
        2000,
        gio::Cancellable::NONE,
        |v| {
            let jmena = v.ok().and_then(|v| v.get::<(Vec<String>,)>()).map(|(j,)| j).unwrap_or_default();
            for jmeno in jmena.into_iter().filter(|j| je_prehravac(j)) {
                let j = jmeno.clone();
                volat_dbus("GetNameOwner", (jmeno,).into(), move |v| {
                    if let Some((vlastnik,)) = v.and_then(|v| v.get::<(String,)>()) {
                        pridat(j, vlastnik);
                    }
                });
            }
            obnovit();
        },
    );
}

fn volat_dbus(metoda: &str, parametry: Variant, hotovo: impl FnOnce(Option<Variant>) + 'static) {
    let Some(s) = sbernice() else { return };
    s.call(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        metoda,
        Some(&parametry),
        None,
        gio::DBusCallFlags::NONE,
        2000,
        gio::Cancellable::NONE,
        move |v| hotovo(v.ok()),
    );
}

fn pridat(jmeno: String, vlastnik: String) {
    PREHRAVACE.with(|p| p.borrow_mut().push(Prehravac { jmeno: jmeno.clone(), vlastnik, ..Default::default() }));
    nacist(&jmeno);
    let j = jmeno.clone();
    volat(&jmeno, "org.freedesktop.DBus.Properties", "Get", Some(("org.mpris.MediaPlayer2", "Identity").into()), move |v| {
        if let Some(identita) = v.and_then(|v| v.get::<(Variant,)>()).and_then(|(v,)| v.get::<String>()) {
            PREHRAVACE.with(|p| {
                if let Some(x) = p.borrow_mut().iter_mut().find(|x| x.jmeno == j) {
                    x.identita = identita;
                }
            });
            obnovit();
        }
    });
}

/// Načte všechny vlastnosti přehrávače znovu.
fn nacist(jmeno: &str) {
    let j = jmeno.to_string();
    volat(jmeno, "org.freedesktop.DBus.Properties", "GetAll", Some((ROZHRANI,).into()), move |v| {
        if let Some((vlastnosti,)) = v.and_then(|v| v.get::<(HashMap<String, Variant>,)>()) {
            zmenit(&j, &vlastnosti);
        }
    });
}

fn cislo(v: &Variant) -> i64 {
    v.get::<i64>()
        .or_else(|| v.get::<u64>().map(|x| x as i64))
        .or_else(|| v.get::<i32>().map(i64::from))
        .or_else(|| v.get::<u32>().map(i64::from))
        .or_else(|| v.get::<f64>().map(|x| x as i64))
        .unwrap_or(0)
}

fn zmenit(jmeno: &str, zmeny: &HashMap<String, Variant>) {
    PREHRAVACE.with(|p| {
        let mut p = p.borrow_mut();
        let Some(i) = p.iter().position(|x| x.jmeno == jmeno) else { return };
        let x = &mut p[i];
        let predtim = x.stav.clone();
        for (klic, v) in zmeny {
            match klic.as_str() {
                "PlaybackStatus" => x.stav = v.str().unwrap_or("").to_string(),
                "CanGoNext" => x.muze_dalsi = v.get().unwrap_or(false),
                "CanGoPrevious" => x.muze_predchozi = v.get().unwrap_or(false),
                "CanSeek" => x.muze_pretacet = v.get().unwrap_or(false),
                "Metadata" => {
                    let m: HashMap<String, Variant> = v.get().unwrap_or_default();
                    let text = |k: &str| m.get(k).and_then(|v| v.str()).unwrap_or("").to_string();
                    x.titulek = text("xesam:title");
                    x.album = text("xesam:album");
                    x.obal = text("mpris:artUrl");
                    x.stopa = text("mpris:trackid");
                    x.umelec = m.get("xesam:artist").and_then(|v| v.get::<Vec<String>>()).map(|a| a.join(", ")).unwrap_or_else(|| text("xesam:artist"));
                    x.delka = m.get("mpris:length").map(cislo).unwrap_or(0);
                }
                _ => {}
            }
        }
        // naposledy spuštěný přehrávač dopředu
        if x.stav == "Playing" && predtim != "Playing" {
            let x = p.remove(i);
            p.insert(0, x);
        }
    });
    obnovit();
}

/// Přehrávač, který se ovládá a ukazuje v panelu.
pub fn aktivni() -> Option<Prehravac> {
    PREHRAVACE.with(|p| {
        let p = p.borrow();
        p.iter().find(|x| x.stav == "Playing").or_else(|| p.iter().find(|x| x.stav == "Paused")).cloned()
    })
}

/// Okénko přehrávače: zavolat při každé změně (None = okénko se zavřelo).
pub fn pri_zmene(f: Option<Box<dyn Fn()>>) {
    PRI_ZMENE.with(|p| p.replace(f));
}

fn ikona_prehravace(jmeno: &str) -> &'static str {
    let j = jmeno.to_lowercase();
    if j.contains("spotify") {
        "\u{f1bc}"
    } else if j.contains("firefox") {
        "\u{f269}"
    } else if j.contains("chromium") || j.contains("brave") || j.contains("chrome") {
        "\u{f268}"
    } else if j.contains("vlc") {
        "\u{f008}"
    } else {
        "\u{f04b}"
    }
}

/// Panel: skladba (klik = okénko přehrávače) a tlačítka ⏮ ⏭, vidět jen když něco hraje
/// nebo je pozastavené.
fn obnovit() {
    match aktivni() {
        Some(p) => {
            let mut text = [p.titulek.as_str(), p.umelec.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" - ");
            if text.is_empty() {
                text = p.identita.clone();
            }
            if text.chars().count() > DELKA_TEXTU {
                text = text.chars().take(DELKA_TEXTU - 1).collect::<String>() + "…";
            }
            let (text, trida) = if p.stav == "Playing" {
                (format!("{}  {}", ikona_prehravace(&p.jmeno), escapovat(&text)), "playing")
            } else {
                (format!("\u{f04c}  <i>{}</i>", escapovat(&text)), "paused")
            };
            publikovat("hudba", json!({"text": text, "class": trida}));
            publikovat("predchozi", json!({"text": "\u{f048}", "class": trida}));
            publikovat("dalsi", json!({"text": "\u{f051}", "class": trida}));
        }
        None => {
            for m in ["hudba", "predchozi", "dalsi"] {
                publikovat(m, json!({"text": ""}));
            }
        }
    }
    PRI_ZMENE.with(|p| {
        if let Some(f) = p.borrow().as_ref() {
            f();
        }
    });
}

/// Ovládání: predchozi, dalsi, play-pause (z panelu, kláves i okénka).
pub fn ovladat(akce: &str) {
    // play-pause umí spustit i zastavený přehrávač
    let cil = aktivni().or_else(|| PREHRAVACE.with(|p| p.borrow().first().cloned()));
    let Some(p) = cil else { return };
    let metoda = match akce {
        "predchozi" => "Previous",
        "dalsi" => "Next",
        "play-pause" => "PlayPause",
        _ => return,
    };
    volat(&p.jmeno, ROZHRANI, metoda, None, |_| {});
}

/// Kde přehrávač je (v mikrosekundách).
pub fn pozice(p: &Prehravac, hotovo: impl FnOnce(i64) + 'static) {
    volat(&p.jmeno, "org.freedesktop.DBus.Properties", "Get", Some((ROZHRANI, "Position").into()), move |v| {
        if let Some((v,)) = v.and_then(|v| v.get::<(Variant,)>()) {
            hotovo(cislo(&v));
        }
    });
}

/// Přetočit na danou pozici (v mikrosekundách).
pub fn pretocit(p: &Prehravac, kam: i64) {
    let Ok(stopa) = glib::variant::ObjectPath::try_from(p.stopa.clone()) else { return };
    volat(&p.jmeno, ROZHRANI, "SetPosition", Some((stopa, kam).into()), |_| {});
}
