//! Ukazatel hlasitosti a jasu (klávesy) a krátkých hlášek (dlaždice) dole nad dokem.
//! Hodnotu ukáže hned; hlasitost nastavuje wpctl na pozadí (z rychle po sobě jdoucích stisků
//! jen poslední hodnotu), jas logind přes D-Bus. Dřív osd.sh s notifikací přes mako (~120 ms).

use std::cell::{Cell, RefCell};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::spolecne::prikaz;

const KROK: f64 = 5.0;

struct Ukazatel {
    okno: gtk::Window,
    ikona: gtk::Label,
    text: gtk::Label,
    uroven: gtk::LevelBar,
    schovat: Option<glib::SourceId>,
}

thread_local! {
    static UKAZATEL: RefCell<Option<Ukazatel>> = const { RefCell::new(None) };
    /// hlasitost v % a ztlumení, naposledy kdy se s nimi pracovalo
    static HLASITOST: Cell<Option<(f64, bool, Instant)>> = const { Cell::new(None) };
    static NASTAVOVAC: RefCell<Option<mpsc::Sender<(f64, bool)>>> = const { RefCell::new(None) };
    /// jas v %, kdy se nastavil (sysfs se změní, až logind jas opravdu nastaví)
    static JAS: Cell<Option<(f64, Instant)>> = const { Cell::new(None) };
}

/// Ukáže ukazatel (hodnota v %, None = jen text) a za 1,5 s ho schová.
pub fn ukazat(ikona: &str, text: &str, hodnota: Option<f64>) {
    UKAZATEL.with(|u| {
        let mut u = u.borrow_mut();
        let u = u.get_or_insert_with(vytvorit);
        u.ikona.set_text(ikona);
        u.text.set_text(text);
        u.uroven.set_visible(hodnota.is_some());
        u.uroven.set_value(hodnota.unwrap_or(0.0));
        if let Some(id) = u.schovat.take() {
            id.remove();
        }
        u.okno.set_default_size(1, 1);
        u.okno.set_visible(true);
        u.schovat = Some(glib::timeout_add_local_once(Duration::from_millis(1500), || {
            UKAZATEL.with(|u| {
                if let Some(u) = u.borrow_mut().as_mut() {
                    u.schovat = None;
                    u.okno.set_visible(false);
                }
            });
            crate::spolecne::vratit_pamet();
        }));
    });
}

fn vytvorit() -> Ukazatel {
    crate::styl::pouzit();
    let okno = gtk::Window::new();
    okno.add_css_class("apogee");
    okno.init_layer_shell();
    okno.set_layer(Layer::Overlay);
    okno.set_keyboard_mode(KeyboardMode::None);
    okno.set_namespace(Some("osd"));
    okno.set_anchor(Edge::Bottom, true);
    okno.set_margin(Edge::Bottom, 110); // nad dokem
    let panel = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    panel.set_widget_name("panel");
    panel.add_css_class("osd");
    let ikona = crate::spolecne::ikona("", "ikona");
    let uroven = gtk::LevelBar::for_interval(0.0, 100.0);
    uroven.set_mode(gtk::LevelBarMode::Continuous);
    for nazev in ["low", "high", "full"] {
        uroven.remove_offset_value(Some(nazev));
    }
    uroven.set_size_request(180, -1);
    uroven.set_valign(gtk::Align::Center);
    let text = crate::spolecne::popisek("", &[]);
    panel.append(&ikona);
    panel.append(&uroven);
    panel.append(&text);
    okno.set_child(Some(&panel));
    Ukazatel { okno, ikona, text, uroven, schovat: None }
}

/// Hlasitost: klávesy vol-up, vol-down, vol-mute.
pub fn hlasitost(akce: &str) {
    // když se s hlasitostí chvíli nic nedělo, mohla se změnit jinde: načíst znovu
    let (mut hlas, mut ztlumeno) = match HLASITOST.with(|h| h.get()) {
        Some((h, z, kdy)) if kdy.elapsed() < Duration::from_secs(3) => (h, z),
        _ => {
            let v = prikaz("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]);
            let h = v.split_whitespace().nth(1).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0) * 100.0;
            (h.round(), v.contains("MUTED"))
        }
    };
    match akce {
        "vol-up" => {
            hlas = (hlas + KROK).min(100.0);
            ztlumeno = false;
        }
        "vol-down" => hlas = (hlas - KROK).max(0.0),
        "vol-mute" => ztlumeno = !ztlumeno,
        _ => return,
    }
    HLASITOST.with(|h| h.set(Some((hlas, ztlumeno, Instant::now()))));
    nastavit_hlasitost(hlas, ztlumeno);
    if ztlumeno {
        ukazat("\u{f6a9}", "Muted", Some(0.0));
    } else {
        let ikona = if hlas == 0.0 { "\u{f026}" } else if hlas < 50.0 { "\u{f027}" } else { "\u{f028}" };
        ukazat(ikona, &format!("{hlas:.0} %"), Some(hlas));
    }
}

/// wpctl na pozadí; z rychle po sobě jdoucích stisků nastaví jen poslední hodnotu.
fn nastavit_hlasitost(hlas: f64, ztlumeno: bool) {
    NASTAVOVAC.with(|n| {
        let mut n = n.borrow_mut();
        let odesilatel = n.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<(f64, bool)>();
            std::thread::spawn(move || {
                while let Ok(mut posledni) = rx.recv() {
                    while let Ok(novejsi) = rx.try_recv() {
                        posledni = novejsi;
                    }
                    let (h, z) = posledni;
                    prikaz("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", h / 100.0)]);
                    prikaz("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", if z { "1" } else { "0" }]);
                }
            });
            tx
        });
        let _ = odesilatel.send((hlas, ztlumeno));
    });
}

fn cist(soubor: &str) -> f64 {
    crate::spolecne::cist_podsviceni(soubor).unwrap_or(0.0)
}

/// Jas: klávesy jas-up, jas-down (po 5 %, nejmíň 1 %, ať obrazovka nezhasne úplně).
pub fn jas(akce: &str) {
    let max = cist("max_brightness").max(1.0);
    let ted = match JAS.with(|j| j.get()) {
        Some((j, kdy)) if kdy.elapsed() < Duration::from_secs(2) => j,
        _ => (cist("brightness") / max * 100.0).round(),
    };
    let novy = match akce {
        "jas-up" => (ted + KROK).min(100.0),
        "jas-down" => (ted - KROK).max(1.0),
        _ => return,
    };
    nastavit_jas(novy);
    ukazat("\u{f185}", &format!("{:.0} %", novy.round()), Some(novy));
}

/// Jas v % (1–100) přes logind, bez ukazatele (posuvník v aplikaci Settings).
pub fn nastavit_jas(novy: f64) {
    let novy = novy.clamp(1.0, 100.0);
    let max = cist("max_brightness").max(1.0);
    JAS.with(|j| j.set(Some((novy, Instant::now()))));
    let hodnota = (novy / 100.0 * max).round() as u32;
    if let Ok(s) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) {
        s.call(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            "SetBrightness",
            Some(&("backlight", crate::spolecne::podsviceni().unwrap_or_default(), hodnota).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1000,
            gio::Cancellable::NONE,
            |_| {},
        );
    }
    crate::panel::publikovat("jas", serde_json::json!({"text": format!("\u{f185}  {:.0} %", novy.round())}));
}
