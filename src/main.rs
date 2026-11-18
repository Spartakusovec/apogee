//! apogee-daemon – the Apogee daemon: bar, dock, popups, notifications, Settings and Alt+Tab
//! for Sway. It gets commands from the `apogee` client (client.rs)
//! over $XDG_RUNTIME_DIR/apogee.sock.

mod migrate;
mod updates;
mod apps;
mod dock;
mod network_secrets;
mod music;
mod bar;
mod microphone;
mod settings;
mod popups;
mod osd;
mod panel;
mod pomodoro;
mod switcher;
mod common;
mod style;
mod notifier;
mod launcher;
mod spotlight;
mod sway;
mod recording;

use std::io::{BufRead, BufReader};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::time::Duration;

use gtk::glib;

fn socket() -> PathBuf {
    common::run_dir().join("apogee.sock")
}

fn main() {
    demon();
}

fn demon() {
    // log a daemon crash to ~/.cache/apogee/errors.log (the daemon's own output is lost)
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        let file = common::home().join(".cache/apogee/errors.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(file) {
            let _ = writeln!(f, "{} {info}", chrono::Local::now().format("%F %T"));
        }
        eprintln!("{info}");
    }));
    // only one daemon: the lock is held for the whole run (bar modules may start it
    // several times at once during login)
    let lock = match std::fs::File::create(common::run_dir().join("apogee.lock")) {
        Ok(z) => z,
        Err(_) => return,
    };
    // --wait (the apogee-watch.sh watchdog): wait until the previous daemon ends, then take over
    let wait = std::env::args().any(|a| a == "--wait");
    let mode = if wait { libc::LOCK_EX } else { libc::LOCK_EX | libc::LOCK_NB };
    if unsafe { libc::flock(lock.as_raw_fd(), mode) } != 0 {
        return;
    }
    // files from older versions had Czech keys
    migrate::run();
    let _ = std::fs::remove_file(socket());
    let listener = match UnixListener::bind(socket()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("apogee: cannot listen on {}: {e}", socket().display());
            return;
        }
    };
    // The popups are small: render them on the CPU. Rendering on the GPU loads the OpenGL/Vulkan
    // drivers and the daemon would use ~150 MB instead of ~40 MB after opening all popups.
    let original_renderer = std::env::var_os("GSK_RENDERER");
    let original_disable = std::env::var_os("GDK_DISABLE");
    std::env::set_var("GSK_RENDERER", "cairo");
    std::env::set_var("GDK_DISABLE", "gl,vulkan,dmabuf");
    gtk::init().expect("GTK init");
    // GTK read GDK_DISABLE at start (it won't use OpenGL or Vulkan any more, it draws on the CPU).
    // But programs started by the daemon (dock, links in popups…) should get a normal environment.
    match original_renderer {
        Some(v) => std::env::set_var("GSK_RENDERER", v),
        None => std::env::remove_var("GSK_RENDERER"),
    }
    match original_disable {
        Some(v) => std::env::set_var("GDK_DISABLE", v),
        None => std::env::remove_var("GDK_DISABLE"),
    }

    // commands from the socket → the GTK main thread; bar modules stay connected
    let (sender, receiver) = async_channel::unbounded::<String>();
    std::thread::spawn(move || {
        for connection in listener.incoming().flatten() {
            let mut row = String::new();
            if BufReader::new(&connection).read_line(&mut row).is_err() {
                continue;
            }
            let row = row.trim();
            if let Some(name) = row.strip_prefix("module ") {
                // a stuck module must not hold up the daemon
                let _ = connection.set_write_timeout(Some(Duration::from_secs(1)));
                panel::add_subscriber(name, connection);
            } else {
                let _ = sender.send_blocking(row.to_string());
            }
        }
    });

    sway::watch();
    apps::load_on_background();
    notifier::start();
    // Wi-Fi passwords when NetworkManager asks for them (instead of nm-applet)
    network_secrets::start();
    popups::start_on_background();
    dock::start();
    // back to waybar: "bar": false in ~/.config/apogee/settings.json (and a bar in the Sway config)
    if common::settings("bar").as_bool() != Some(false) {
        bar::start();
    }
    launcher::prepare();
    spotlight::prepare();
    settings::start_po_login();

    glib::spawn_future_local(async move {
        while let Ok(command) = receiver.recv().await {
            let (command, argument) = command.split_once(' ').unwrap_or((&command, ""));
            let (command, argument) = czech_names(command, argument);
            let (command, argument) = (command.as_str(), argument.as_str());
            match command {
                "alttab" => switcher::alttab(1),
                "alttab-back" => switcher::alttab(-1),
                kind => popups::command(kind, argument),
            }
        }
    });
    let _lock = lock;
    glib::MainLoop::new(None, false).run();
}

/// English commands (`apogee settings wifi`, `apogee osd brightness-up`…) to the internal names;
/// the old Czech commands pass unchanged.
fn czech_names(command: &str, argument: &str) -> (String, String) {
    let p = match command {
        "alttab-back" => "alttab-back",
        "notifications" => "notifications",
        "notifications-changed" => "notifications-change",
        "weather-refresh" => "weather-refresh",
        "music" => "music",
        "screenshot-now" => "screenshot-now",
        "recording-stop" => "recording-stop",
        "maximize" => "maximalizovat",
        "tiling" => "tiling",
        "dock" => "dock",
        "settings" => "settings",
        "launcher" => "launcher",
        "style" => "style",
        "record" => "record",
        "check-updates" => "updates-check",
        "window" => "window",
        "clipboard" => "clipboard",
        "power" => "power",
        "calendar" => "calendar",
        "player" => "player",
        "weather" => "weather",
        "wallpapers" => "wallpapers",
        "updates" => "updates",
        "sound" => "sound",
        "brightness" => "brightness",
        "battery" => "battery",
        "screenshot" => "screenshot",
        "recording" => "recording",
        p => p,
    };
    let a = match (p, argument) {
        ("music", "next") => "next",
        ("music", "previous") => "previous",
        ("screenshot-now", "area") => "area",
        ("screenshot-now", "screen") => "screen",
        ("screenshot-now", "window") => "window",
        ("osd", "brightness-up") => "brightness-up",
        ("osd", "brightness-down") => "brightness-down",
        ("settings", "network") => "network",
        ("settings", "sound") => "sound",
        ("settings", "display") => "display",
        ("settings", "keyboard") => "input",
        ("settings", "power") => "power",
        ("settings", "appearance") => "appearance",
        ("settings", "panel") => "panel_dock",
        ("settings", "windows") => "windows",
        ("settings", "notifications") => "notifications",
        ("settings", "shortcuts") => "shortcuts",
        ("settings", "apps") => "apps",
        ("settings", "time") => "time",
        ("settings", "users") => "user",
        ("settings", "storage") => "storage",
        ("settings", "updates") => "updates",
        ("settings", "about") => "about",
        (_, a) => a,
    };
    (p.to_string(), a.to_string())
}
