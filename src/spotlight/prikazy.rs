//! Systémové příkazy ve spotlightu: zamknout, restartovat, Wi-Fi, tapeta… (anglicky i česky).

use super::bez_diakritiky;
use crate::spolecne::pozadi;

pub struct Prikaz {
    pub nazev: &'static str,
    pub popis: &'static str,
    pub ikona: &'static str,
    /// hledá se podle začátku kteréhokoli slova (bez diakritiky)
    slova: &'static [&'static str],
    pub akce: fn(),
}

fn okenko(druh: &'static str) {
    gtk::glib::idle_add_local_once(move || crate::okenka::prepnout(druh));
}

pub const PRIKAZY: &[Prikaz] = &[
    Prikaz { nazev: "Lock", popis: "Lock the screen", ikona: "\u{f023}", slova: &["lock", "zamknout", "zamek"], akce: || pozadi("gtklock", &["-d"]) },
    Prikaz { nazev: "Log out", popis: "End the session", ikona: "\u{f2f5}", slova: &["log out", "logout", "sign out", "odhlasit"], akce: || crate::sway::prikaz("exit") },
    Prikaz { nazev: "Suspend", popis: "Sleep", ikona: "\u{f186}", slova: &["suspend", "sleep", "uspat", "spanek"], akce: || pozadi("systemctl", &["suspend"]) },
    Prikaz { nazev: "Restart", popis: "Restart the computer", ikona: "\u{f2f9}", slova: &["restart", "reboot", "restartovat"], akce: || pozadi("systemctl", &["reboot"]) },
    Prikaz { nazev: "Shut down", popis: "Turn the computer off", ikona: "\u{f011}", slova: &["shut down", "shutdown", "power off", "vypnout"], akce: || pozadi("systemctl", &["poweroff"]) },
    Prikaz { nazev: "Wi-Fi", popis: "Networks", ikona: "\u{f1eb}", slova: &["wifi", "wi-fi", "network", "internet", "sit"], akce: || okenko("wifi") },
    Prikaz { nazev: "Bluetooth", popis: "Devices", ikona: "\u{f293}", slova: &["bluetooth", "sluchatka", "headphones"], akce: || okenko("bluetooth") },
    Prikaz { nazev: "Sound", popis: "Volume, microphone, output", ikona: "\u{f028}", slova: &["sound", "volume", "audio", "microphone", "zvuk", "hlasitost", "mikrofon"], akce: || okenko("zvuk") },
    Prikaz { nazev: "Brightness", popis: "Display brightness and night light", ikona: "\u{f185}", slova: &["brightness", "display", "jas", "displej", "obrazovka"], akce: || okenko("jas") },
    Prikaz { nazev: "Battery", popis: "Battery and power mode", ikona: "\u{f240}", slova: &["battery", "power mode", "power saver", "baterie", "napajeni", "uspora"], akce: || okenko("baterie") },
    Prikaz { nazev: "Night light", popis: "Turn the warm light on / off", ikona: "\u{f186}", slova: &["night light", "nocni rezim", "nocni svetlo"], akce: || {
        pozadi(&crate::spolecne::skripty().join("night-light.sh").to_string_lossy(), &["prepnout"])
    } },
    Prikaz { nazev: "Do Not Disturb", popis: "Turn notifications on / off", ikona: "\u{f1f6}", slova: &["do not disturb", "dnd", "nerusit", "notifications"], akce: || crate::okenka::prikaz("dnd", "") },
    Prikaz { nazev: "Notifications", popis: "Notification center", ikona: "\u{f0f3}", slova: &["notifications", "notifikace", "oznameni"], akce: || okenko("notifikace") },
    Prikaz { nazev: "Screenshot", popis: "Area, window or screen", ikona: "\u{f030}", slova: &["screenshot", "snimek", "snimek obrazovky"], akce: || okenko("snimek") },
    Prikaz { nazev: "Screen recording", popis: "Record the screen", ikona: "\u{f03d}", slova: &["screen recording", "record", "nahravani", "nahrat obrazovku"], akce: || okenko("nahravani") },
    Prikaz { nazev: "Wallpaper", popis: "Change the wallpaper and colors", ikona: "\u{f03e}", slova: &["wallpaper", "background", "tapeta", "pozadi"], akce: || okenko("tapety") },
    Prikaz { nazev: "Updates", popis: "System and Flatpak updates", ikona: "\u{f019}", slova: &["updates", "upgrade", "aktualizace"], akce: || okenko("aktualizace") },
    Prikaz { nazev: "Clipboard history", popis: "Super + V", ikona: "\u{f328}", slova: &["clipboard", "schranka", "historie schranky"], akce: || okenko("schranka") },
    Prikaz { nazev: "Calendar", popis: "This month", ikona: "\u{f133}", slova: &["calendar", "kalendar", "date", "datum"], akce: || okenko("kalendar") },
    Prikaz { nazev: "Weather", popis: "Forecast", ikona: "\u{f6c4}", slova: &["weather", "pocasi", "predpoved"], akce: || okenko("pocasi") },
    Prikaz { nazev: "All apps", popis: "App grid", ikona: "\u{f00a}", slova: &["all apps", "apps", "aplikace", "vsechny aplikace"], akce: || crate::spoustec::prepnout() },
];

/// Příkazy, jejichž některé slovo začíná hledaným textem (aspoň 2 znaky).
pub fn najit(dotaz: &str) -> Vec<&'static Prikaz> {
    let d = bez_diakritiky(&dotaz.to_lowercase());
    if d.chars().count() < 2 {
        return Vec::new();
    }
    PRIKAZY
        .iter()
        .filter(|p| {
            p.slova.iter().chain(std::iter::once(&p.nazev)).any(|s| {
                let s = bez_diakritiky(&s.to_lowercase());
                s.starts_with(&d) || s.split_whitespace().any(|slovo| slovo.starts_with(&d))
            })
        })
        .collect()
}
