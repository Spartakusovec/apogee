//! System commands in Spotlight: lock, restart, Wi-Fi, wallpaper… (in English and Czech).

use super::without_diacritics;
use crate::common::background;

pub struct Command {
    pub name: &'static str,
    pub description: &'static str,
    pub icon: &'static str,
    /// matches the start of any word (without diacritics)
    words: &'static [&'static str],
    pub action: fn(),
}

fn popup(kind: &'static str) {
    gtk::glib::idle_add_local_once(move || crate::popups::toggle(kind));
}

pub const COMMANDS: &[Command] = &[
    Command { name: "Lock", description: "Lock the screen", icon: "\u{f023}", words: &["lock", "zamknout", "zamek"], action: || background("gtklock", &["-d"]) },
    Command { name: "Log out", description: "End the session", icon: "\u{f2f5}", words: &["log out", "logout", "sign out", "odhlasit"], action: || crate::sway::command("exit") },
    Command { name: "Suspend", description: "Sleep", icon: "\u{f186}", words: &["suspend", "sleep", "uspat", "spanek"], action: || background("systemctl", &["suspend"]) },
    Command { name: "Restart", description: "Restart the computer", icon: "\u{f2f9}", words: &["restart", "reboot", "restartovat"], action: || background("systemctl", &["reboot"]) },
    Command { name: "Shut down", description: "Turn the computer off", icon: "\u{f011}", words: &["shut down", "shutdown", "power off", "vypnout"], action: || background("systemctl", &["poweroff"]) },
    Command { name: "Wi-Fi", description: "Networks", icon: "\u{f1eb}", words: &["wifi", "wi-fi", "network", "internet", "sit"], action: || popup("wifi") },
    Command { name: "Bluetooth", description: "Devices", icon: "\u{f293}", words: &["bluetooth", "sluchatka", "headphones"], action: || popup("bluetooth") },
    Command { name: "Sound", description: "Volume, microphone, output", icon: "\u{f028}", words: &["sound", "volume", "audio", "microphone", "zvuk", "hlasitost", "mikrofon"], action: || popup("zvuk") },
    Command { name: "Brightness", description: "Display brightness and night light", icon: "\u{f185}", words: &["brightness", "display", "jas", "displej", "obrazovka"], action: || popup("jas") },
    Command { name: "Battery", description: "Battery and power mode", icon: "\u{f240}", words: &["battery", "power mode", "power saver", "baterie", "napajeni", "uspora"], action: || popup("baterie") },
    Command { name: "Night light", description: "Turn the warm light on / off", icon: "\u{f186}", words: &["night light", "nocni rezim", "nocni svetlo"], action: || {
        background(&crate::common::scripts().join("night-light.sh").to_string_lossy(), &["toggle"])
    } },
    Command { name: "Do Not Disturb", description: "Turn notifications on / off", icon: "\u{f1f6}", words: &["do not disturb", "dnd", "nerusit", "notifications"], action: || crate::popups::command("dnd", "") },
    Command { name: "Notifications", description: "Notification center", icon: "\u{f0f3}", words: &["notifications", "notifikace", "oznameni"], action: || popup("notifikace") },
    Command { name: "Screenshot", description: "Area, window or screen", icon: "\u{f030}", words: &["screenshot", "snimek", "snimek obrazovky"], action: || popup("snimek") },
    Command { name: "Screen recording", description: "Record the screen", icon: "\u{f03d}", words: &["screen recording", "record", "nahravani", "nahrat obrazovku"], action: || popup("nahravani") },
    Command { name: "Wallpaper", description: "Change the wallpaper and colors", icon: "\u{f03e}", words: &["wallpaper", "background", "tapeta", "pozadi"], action: || popup("tapety") },
    Command { name: "Updates", description: "System and Flatpak updates", icon: "\u{f019}", words: &["updates", "upgrade", "aktualizace"], action: || popup("aktualizace") },
    Command { name: "Clipboard history", description: "Super + V", icon: "\u{f328}", words: &["clipboard", "schranka", "historie schranky"], action: || popup("schranka") },
    Command { name: "Calendar", description: "This month", icon: "\u{f133}", words: &["calendar", "kalendar", "date", "datum"], action: || popup("kalendar") },
    Command { name: "Weather", description: "Forecast", icon: "\u{f6c4}", words: &["weather", "pocasi", "predpoved"], action: || popup("pocasi") },
    Command { name: "All apps", description: "App grid", icon: "\u{f00a}", words: &["all apps", "apps", "aplikace", "vsechny aplikace"], action: || crate::launcher::toggle() },
];

/// Commands where some word starts with the searched text (at least 2 characters).
pub fn find(query: &str) -> Vec<&'static Command> {
    let d = without_diacritics(&query.to_lowercase());
    if d.chars().count() < 2 {
        return Vec::new();
    }
    COMMANDS
        .iter()
        .filter(|p| {
            p.words.iter().chain(std::iter::once(&p.name)).any(|s| {
                let s = without_diacritics(&s.to_lowercase());
                s.starts_with(&d) || s.split_whitespace().any(|word| word.starts_with(&d))
            })
        })
        .collect()
}
