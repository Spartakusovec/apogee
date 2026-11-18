//! apogee – okénka pod horním panelem, přepínač oken Alt+Tab a moduly panelu pro Sway.
//!
//!     apogee daemon              – spustí démona (apogee-daemon, při přihlášení)
//!     apogee <okénko>            – sound, brightness, wifi, bluetooth, battery, weather, calendar,
//!                                    notifications, player, screenshot, recording, wallpapers, updates,
//!                                    window (nabídka okna), clipboard (historie schránky), power
//!                                    (další stejný příkaz okénko zavře, jiný ho vymění)
//!     apogee osd vol-up|vol-down|vol-mute|brightness-up|brightness-down – klávesy s ukazatelem
//!     apogee maximize [obnovit]  – Super+↑ / Super+↓
//!     apogee tiling              – plocha plovoucí okna ↔ dlaždice (tlačítko v panelu)
//!     apogee dock                – ukázat / schovat dok (Super+X)
//!     apogee launcher            – mřížka aplikací (tlačítko v doku)
//!     apogee spotlight [text]    – hledání aplikací, souborů, kalkulačka, převody (Super+mezerník)
//!     apogee style               – znovu načíst barvy (po změně tapety)
//!     apogee alttab | alttab-back – přepínač oken (Alt+Tab / Alt+Shift+Tab)
//!     apogee modul <název>       – modul panelu jako JSON pro waybar (starý panel)
//!     apogee notifications-changed – přišla notifikace (volá mako), obnoví zvoneček
//!     apogee dnd [on|off]        – Do Not Disturb přepnout / zapnout / vypnout
//!     apogee weather-refresh     – stáhnout počasí znovu
//!     apogee music previous|next|play-pause – ovládání přehrávače
//!     apogee settings [stránka]  – aplikace Settings (wifi, network, bluetooth, sound, display,
//!                                    keyboard, power, appearance, panel, windows, notifications,
//!                                    shortcuts, apps, time, users, storage, updates, about)
//!     apogee screenshot-now area|window|screen – snímek obrazovky bez okénka (Print Screen)
//!     apogee record oblast|okno|obrazovka [vypnuto|system|mikrofon] – nahrávat obrazovku
//!     apogee recording-stop      – ukončit nahrávání obrazovky
//!     apogee check-updates       – zkontrolovat aktualizace hned
//!     (staré české názvy příkazů fungují dál)
//!
//! Tenhle klient jen pošle zprávu démonovi přes $XDG_RUNTIME_DIR/apogee.sock. GTK nenačítá,
//! takže je okamžitý a malý. Když démon neběží, spustí ho.

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn soket() -> PathBuf {
    let run = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(run).join("apogee.sock")
}

fn demon() -> PathBuf {
    std::env::current_exe().map(|j| j.with_file_name("apogee-daemon")).unwrap_or_else(|_| "apogee-daemon".into())
}

fn main() {
    let argumenty: Vec<String> = std::env::args().skip(1).collect();
    match argumenty.first().map(String::as_str) {
        Some("daemon") => {
            let chyba = Command::new(demon()).exec();
            eprintln!("apogee: cannot start the daemon: {chyba}");
        }
        Some("modul") if argumenty.len() == 2 => modul(&argumenty[1]),
        Some(_) => {
            if let Some(mut spojeni) = pripojit() {
                let _ = spojeni.write_all(format!("{}\n", argumenty.join(" ")).as_bytes());
            }
        }
        None => eprintln!("Usage: apogee daemon | settings [page] | sound | brightness | wifi | bluetooth | battery | weather | calendar | notifications | dock | spotlight | alttab | alttab-back | osd <key> | music <action>"),
    }
}

/// Spojení s démonem; když neběží, spustí ho a chvíli počká.
fn pripojit() -> Option<UnixStream> {
    if let Ok(s) = UnixStream::connect(soket()) {
        return Some(s);
    }
    // přes setsid: démon nebude dítě klienta (žádné mrtvé procesy, když se nespustí, protože
    // už běží) a bude mimo skupinu procesů panelu (restart panelu ho neukončí)
    let _ = Command::new("setsid").arg("-f").arg(demon()).stdin(Stdio::null()).stdout(Stdio::null()).status();
    let zacatek = Instant::now();
    while zacatek.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(50));
        if let Ok(s) = UnixStream::connect(soket()) {
            return Some(s);
        }
    }
    eprintln!("apogee: the daemon is not running and could not be started");
    None
}

/// Je výstup zavřený? (waybar skončil nebo se restartoval)
fn vystup_zavreny() -> bool {
    let mut f = libc::pollfd { fd: 1, events: 0, revents: 0 };
    unsafe { libc::poll(&mut f, 1, 0) > 0 && f.revents & (libc::POLLERR | libc::POLLHUP) != 0 }
}

/// Modul panelu: přeposílá waybaru, co pošle démon. Skončí, až waybar zavře výstup;
/// když démon spadne nebo se restartuje, připojí se znovu.
fn modul(nazev: &str) {
    let mut vystup = std::io::stdout().lock();
    loop {
        if let Some(mut spojeni) = pripojit() {
            if spojeni.write_all(format!("modul {nazev}\n").as_bytes()).is_ok() {
                let mut buf = [0u8; 4096];
                loop {
                    let mut f = [
                        libc::pollfd { fd: spojeni.as_raw_fd(), events: libc::POLLIN, revents: 0 },
                        libc::pollfd { fd: 1, events: 0, revents: 0 },
                    ];
                    if unsafe { libc::poll(f.as_mut_ptr(), 2, -1) } < 0 {
                        continue; // přerušeno signálem
                    }
                    if f[1].revents & (libc::POLLERR | libc::POLLHUP) != 0 {
                        return;
                    }
                    if f[0].revents == 0 {
                        continue;
                    }
                    match spojeni.read(&mut buf) {
                        Ok(0) | Err(_) => break, // démon skončil
                        Ok(n) => {
                            if vystup.write_all(&buf[..n]).and_then(|_| vystup.flush()).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
        if vystup_zavreny() {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
