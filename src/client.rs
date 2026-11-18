//! apogee – the client for the Apogee daemon (bar, dock, popups, Settings, Alt+Tab) on Sway.
//!
//!     apogee daemon              – start the daemon (apogee-daemon, at login)
//!     apogee <popup>             – sound, brightness, wifi, bluetooth, battery, weather, calendar,
//!                                    notifications, player, screenshot, recording, wallpapers, updates,
//!                                    window (window menu), clipboard (clipboard history), power
//!                                    (the same command again closes the popup, another one replaces it)
//!     apogee osd vol-up|vol-down|vol-mute|brightness-up|brightness-down – keys with an indicator
//!     apogee maximize [obnovit]  – Super+↑ / Super+↓
//!     apogee tiling              – workspace: floating windows ↔ tiling (button in the bar)
//!     apogee dock                – show / hide the dock (Super+X)
//!     apogee launcher            – app grid (button in the dock)
//!     apogee spotlight [text]    – search apps and files, calculator, conversions (Super+Space)
//!     apogee style               – reload the colours (after the wallpaper changed)
//!     apogee alttab | alttab-back – window switcher (Alt+Tab / Alt+Shift+Tab)
//!     apogee modul <name>        – a bar module as JSON for waybar (old bar)
//!     apogee notifications-changed – a notification arrived (called by mako), refresh the bell
//!     apogee dnd [on|off]        – Do Not Disturb: toggle / on / off
//!     apogee weather-refresh     – download the weather again
//!     apogee music previous|next|play-pause – player controls
//!     apogee settings [page]     – the Settings app (wifi, network, bluetooth, sound, display,
//!                                    keyboard, power, appearance, panel, windows, notifications,
//!                                    shortcuts, apps, time, users, storage, updates, about)
//!     apogee screenshot-now area|window|screen – screenshot without a popup (Print Screen)
//!     apogee record area|window|screen [off|system|microphone] – record the screen
//!     apogee recording-stop      – stop recording the screen
//!     apogee check-updates       – zkontrolovat aktualizace hned
//!     (the old Czech command names still work)
//!
//! This client only sends a message to the daemon over $XDG_RUNTIME_DIR/apogee.sock. It doesn't load GTK,
//! so it is instant and small. If the daemon isn't running, it starts it.

use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn socket() -> PathBuf {
    let run = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(run).join("apogee.sock")
}

fn demon() -> PathBuf {
    std::env::current_exe().map(|j| j.with_file_name("apogee-daemon")).unwrap_or_else(|_| "apogee-daemon".into())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("daemon") => {
            let error = Command::new(demon()).exec();
            eprintln!("apogee: cannot start the daemon: {error}");
        }
        Some("module") if args.len() == 2 => module(&args[1]),
        Some(_) => {
            if let Some(mut connection) = connect() {
                let _ = connection.write_all(format!("{}\n", args.join(" ")).as_bytes());
            }
        }
        None => eprintln!("Usage: apogee daemon | settings [page] | sound | brightness | wifi | bluetooth | battery | weather | calendar | notifications | dock | spotlight | alttab | alttab-back | osd <key> | music <action>"),
    }
}

/// Connection to the daemon; if it isn't running, start it and wait a moment.
fn connect() -> Option<UnixStream> {
    if let Ok(s) = UnixStream::connect(socket()) {
        return Some(s);
    }
    // through setsid: the daemon is not the client's child (no zombie processes when it doesn't start because
    // it already runs) and is outside the bar's process group (restarting the bar doesn't end it)
    let _ = Command::new("setsid").arg("-f").arg(demon()).stdin(Stdio::null()).stdout(Stdio::null()).status();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        std::thread::sleep(Duration::from_millis(50));
        if let Ok(s) = UnixStream::connect(socket()) {
            return Some(s);
        }
    }
    eprintln!("apogee: the daemon is not running and could not be started");
    None
}

/// Is the output closed? (waybar ended or restarted)
fn output_closed() -> bool {
    let mut f = libc::pollfd { fd: 1, events: 0, revents: 0 };
    unsafe { libc::poll(&mut f, 1, 0) > 0 && f.revents & (libc::POLLERR | libc::POLLHUP) != 0 }
}

/// Bar module: forwards to waybar what the daemon sends. Ends when waybar closes the output;
/// if the daemon crashes or restarts, it reconnects.
fn module(name: &str) {
    let mut output = std::io::stdout().lock();
    loop {
        if let Some(mut connection) = connect() {
            if connection.write_all(format!("modul {name}\n").as_bytes()).is_ok() {
                let mut buf = [0u8; 4096];
                loop {
                    let mut f = [
                        libc::pollfd { fd: connection.as_raw_fd(), events: libc::POLLIN, revents: 0 },
                        libc::pollfd { fd: 1, events: 0, revents: 0 },
                    ];
                    if unsafe { libc::poll(f.as_mut_ptr(), 2, -1) } < 0 {
                        continue; // interrupted by a signal
                    }
                    if f[1].revents & (libc::POLLERR | libc::POLLHUP) != 0 {
                        return;
                    }
                    if f[0].revents == 0 {
                        continue;
                    }
                    match connection.read(&mut buf) {
                        Ok(0) | Err(_) => break, // the daemon ended
                        Ok(n) => {
                            if output.write_all(&buf[..n]).and_then(|_| output.flush()).is_err() {
                                return;
                            }
                        }
                    }
                }
            }
        }
        if output_closed() {
            return;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}
