//! Popups under the bar. At most one is open: the same command closes it, another replaces it.

mod updates;
mod battery;
mod bluetooth;
pub mod brightness;
mod calendar;
mod notifications;
mod window;
pub mod weather;
mod pomodoro;
mod player;
mod clipboard;
mod screenshot;
pub mod wallpapers;
mod power;
mod wifi;
pub mod sound;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::common::{Popup, Placement};

thread_local! {
    static OPEN: RefCell<Option<(String, u64, Rc<Popup>)>> = const { RefCell::new(None) };
    static COUNTER: Cell<u64> = const { Cell::new(0) };
}

/// What runs in the background from the daemon start: bar modules (bell, weather, music, microphone,
/// updates, recording, pomodoro) and battery watching.
pub fn start_on_background() {
    crate::music::start();
    crate::microphone::watch();
    crate::updates::watch();
    crate::recording::start_panel();
    crate::pomodoro::start_panel();
    notifications::refresh_bell();
    weather::watch();
    battery::watch();
}

/// A command from the client that isn't the window switcher.
pub fn command(kind: &str, argument: &str) {
    match kind {
        "notifications-change" => notifications::refresh_bell(),
        "dnd" => notifications::set_dnd(match argument {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        }),
        "weather-refresh" => weather::refresh(),
        "music" => crate::music::control(argument),
        // Print Screen: screenshot right away, without a popup
        "screenshot-now" => crate::recording::screenshot(argument, 0, false),
        "recording-stop" => crate::recording::stop(),
        // volume and brightness keys
        "osd" if argument.starts_with("vol") => crate::osd::volume(argument),
        "osd" => crate::osd::brightness(argument),
        // maximizing is done by the patched Sway; Super+↑/↓ call its commands directly
        "maximalizovat" => crate::sway::command(if argument == "refresh" { "maximize disable" } else { "maximize enable" }),
        "tiling" => crate::sway::toggle_tiling(),
        "dock" => crate::dock::toggle(),
        // aplikace Settings: `apogee nastaveni [wifi|bluetooth|zvuk|napajeni]`
        "settings" => crate::settings::open(argument),
        "launcher" => crate::launcher::toggle(),
        "spotlight" => crate::spotlight::toggle(argument),
        // set-wallpaper: new colours right away, for the dock too
        "style" => crate::style::apply(),
        "record" => {
            let (mode, sound) = argument.split_once(' ').unwrap_or((argument, "off"));
            crate::recording::record(mode, sound)
        }
        "updates-check" => crate::updates::check(|_| {}),
        _ => toggle(kind),
    }
}

pub fn toggle(kind: &str) {
    if let Some((stary, _, popup)) = OPEN.with(|o| o.borrow_mut().take()) {
        popup.close();
        if stary == kind {
            return;
        }
    }
    let (placement, width) = match kind {
        "window" => (Placement::Center, 380),
        "clipboard" => (Placement::Center, 560),
        "power" => (Placement::Center, 260),
        "calendar" | "player" => (Placement::Middle, 360),
        "weather" | "notifications" | "wallpapers" | "updates" => (Placement::Right, 400),
        "sound" | "brightness" | "wifi" | "bluetooth" | "battery" | "screenshot" | "recording" => (Placement::Right, 360),
        _ => {
            eprintln!("apogee: unknown command {kind}");
            return;
        }
    };
    let id = COUNTER.with(|p| {
        p.set(p.get() + 1);
        p.get()
    });
    // forget it after closing (Esc, mouse leaving…), but only if it is still the same popup
    let o = Popup::new(placement, width, move || {
        OPEN.with(|o| {
            let mut o = o.borrow_mut();
            if o.as_ref().is_some_and(|(_, i, _)| *i == id) {
                *o = None;
            }
        })
    });
    match kind {
        "sound" => sound::fill(&o),
        "brightness" => brightness::fill(&o),
        "wifi" => wifi::fill(&o),
        "bluetooth" => bluetooth::fill(&o),
        "battery" => battery::fill(&o),
        "weather" => weather::fill(&o),
        "calendar" => calendar::fill(&o),
        "notifications" => notifications::fill(&o),
        "player" => player::fill(&o),
        "window" => window::fill(&o),
        "clipboard" => clipboard::fill(&o),
        "power" => power::fill(&o),
        "screenshot" | "recording" => screenshot::fill(&o, kind),
        "wallpapers" => wallpapers::fill(&o),
        "updates" => updates::fill(&o),
        _ => {}
    }
    o.show();
    OPEN.with(|x| x.replace(Some((kind.to_string(), id, o))));
}

/// Closes the open popup (e.g. before a screenshot).
pub fn close() {
    if let Some((_, _, popup)) = OPEN.with(|o| o.borrow_mut().take()) {
        popup.close();
    }
}
