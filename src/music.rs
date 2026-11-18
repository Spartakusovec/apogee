//! Music over MPRIS (D-Bus, without playerctl): what's playing, bar modules (track, ⏮ ⏭ buttons),
//! controls (bar, keys) and data for the player popup.
//!
//! The player that started playing last is controlled; when nothing plays, the one paused last.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk::gio;
use gtk::glib::{self, Variant};
use serde_json::json;

use crate::panel::{escape, publish};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const INTERFACE: &str = "org.mpris.MediaPlayer2.Player";
const LENGTH_TEXT: usize = 45;

#[derive(Clone, Default)]
pub struct Player {
    /// bus name (org.mpris.MediaPlayer2.vlc)
    pub name: String,
    /// unique name (:1.23), signals come from it
    owner: String,
    /// Playing, Paused, Stopped
    pub state: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// mpris:artUrl (file:// or https://)
    pub wrapper: String,
    /// in microseconds, 0 = unknown
    pub length: i64,
    stopa: String,
    pub can_previous: bool,
    pub can_next: bool,
    pub can_seek: bool,
    /// app name (VLC media player, Firefox…)
    pub identity: String,
}

thread_local! {
    /// most recently started first
    static PLAYERS: RefCell<Vec<Player>> = const { RefCell::new(Vec::new()) };
    static BUS: RefCell<Option<gio::DBusConnection>> = const { RefCell::new(None) };
    static SUBSCRIPTIONS: RefCell<Vec<gio::SignalSubscription>> = const { RefCell::new(Vec::new()) };
    /// an open player popup gets redrawn
    static ON_CHANGE: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

fn is_player(name: &str) -> bool {
    name.starts_with(PREFIX) && !name.ends_with(".playerctld")
}

fn bus() -> Option<gio::DBusConnection> {
    BUS.with(|s| s.borrow().clone())
}

fn call(target: &str, interface: &str, method: &str, params: Option<Variant>, done: impl FnOnce(Option<Variant>) + 'static) {
    let Some(s) = bus() else { return };
    s.call(
        Some(target),
        PATH,
        interface,
        method,
        params.as_ref(),
        None,
        gio::DBusCallFlags::NONE,
        2000,
        gio::Cancellable::NONE,
        move |v| done(v.ok()),
    );
}

/// Starts watching the players (when the daemon starts).
pub fn start() {
    let Ok(s) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else { return };
    BUS.with(|b| b.replace(Some(s.clone())));
    let subscriptions = vec![
        // a player started or ended
        s.subscribe_to_signal(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            None,
            gio::DBusSignalFlags::NONE,
            |sig| {
                let Some((name, _, new)) = sig.parameters.get::<(String, String, String)>() else { return };
                if !is_player(&name) {
                    return;
                }
                PLAYERS.with(|p| p.borrow_mut().retain(|x| x.name != name));
                if new.is_empty() {
                    refresh();
                } else {
                    add(name, new);
                }
            },
        ),
        // state, track… changed
        s.subscribe_to_signal(
            None,
            Some("org.freedesktop.DBus.Properties"),
            Some("PropertiesChanged"),
            Some(PATH),
            Some(INTERFACE),
            gio::DBusSignalFlags::NONE,
            |sig| {
                let Some((_, changes, invalidated)) = sig.parameters.get::<(String, HashMap<String, Variant>, Vec<String>)>() else {
                    return;
                };
                let name = PLAYERS.with(|p| p.borrow().iter().find(|x| x.owner == sig.sender_name).map(|x| x.name.clone()));
                if let Some(name) = name {
                    change(&name, &changes);
                    if !invalidated.is_empty() {
                        load(&name);
                    }
                }
            },
        ),
    ];
    SUBSCRIPTIONS.with(|o| o.replace(subscriptions));
    // players that are already running
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
            let names = v.ok().and_then(|v| v.get::<(Vec<String>,)>()).map(|(j,)| j).unwrap_or_default();
            for name in names.into_iter().filter(|j| is_player(j)) {
                let j = name.clone();
                call_dbus("GetNameOwner", (name,).into(), move |v| {
                    if let Some((owner,)) = v.and_then(|v| v.get::<(String,)>()) {
                        add(j, owner);
                    }
                });
            }
            refresh();
        },
    );
}

fn call_dbus(method: &str, params: Variant, done: impl FnOnce(Option<Variant>) + 'static) {
    let Some(s) = bus() else { return };
    s.call(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        method,
        Some(&params),
        None,
        gio::DBusCallFlags::NONE,
        2000,
        gio::Cancellable::NONE,
        move |v| done(v.ok()),
    );
}

fn add(name: String, owner: String) {
    PLAYERS.with(|p| p.borrow_mut().push(Player { name: name.clone(), owner, ..Default::default() }));
    load(&name);
    let j = name.clone();
    call(&name, "org.freedesktop.DBus.Properties", "Get", Some(("org.mpris.MediaPlayer2", "Identity").into()), move |v| {
        if let Some(identity) = v.and_then(|v| v.get::<(Variant,)>()).and_then(|(v,)| v.get::<String>()) {
            PLAYERS.with(|p| {
                if let Some(x) = p.borrow_mut().iter_mut().find(|x| x.name == j) {
                    x.identity = identity;
                }
            });
            refresh();
        }
    });
}

/// Loads all properties of a player again.
fn load(name: &str) {
    let j = name.to_string();
    call(name, "org.freedesktop.DBus.Properties", "GetAll", Some((INTERFACE,).into()), move |v| {
        if let Some((properties,)) = v.and_then(|v| v.get::<(HashMap<String, Variant>,)>()) {
            change(&j, &properties);
        }
    });
}

fn number(v: &Variant) -> i64 {
    v.get::<i64>()
        .or_else(|| v.get::<u64>().map(|x| x as i64))
        .or_else(|| v.get::<i32>().map(i64::from))
        .or_else(|| v.get::<u32>().map(i64::from))
        .or_else(|| v.get::<f64>().map(|x| x as i64))
        .unwrap_or(0)
}

fn change(name: &str, changes: &HashMap<String, Variant>) {
    PLAYERS.with(|p| {
        let mut p = p.borrow_mut();
        let Some(i) = p.iter().position(|x| x.name == name) else { return };
        let x = &mut p[i];
        let before = x.state.clone();
        for (key, v) in changes {
            match key.as_str() {
                "PlaybackStatus" => x.state = v.str().unwrap_or("").to_string(),
                "CanGoNext" => x.can_next = v.get().unwrap_or(false),
                "CanGoPrevious" => x.can_previous = v.get().unwrap_or(false),
                "CanSeek" => x.can_seek = v.get().unwrap_or(false),
                "Metadata" => {
                    let m: HashMap<String, Variant> = v.get().unwrap_or_default();
                    let text = |k: &str| m.get(k).and_then(|v| v.str()).unwrap_or("").to_string();
                    x.title = text("xesam:title");
                    x.album = text("xesam:album");
                    x.wrapper = text("mpris:artUrl");
                    x.stopa = text("mpris:trackid");
                    x.artist = m.get("xesam:artist").and_then(|v| v.get::<Vec<String>>()).map(|a| a.join(", ")).unwrap_or_else(|| text("xesam:artist"));
                    x.length = m.get("mpris:length").map(number).unwrap_or(0);
                }
                _ => {}
            }
        }
        // the most recently started player to the front
        if x.state == "Playing" && before != "Playing" {
            let x = p.remove(i);
            p.insert(0, x);
        }
    });
    refresh();
}

/// The player that is controlled and shown in the bar.
pub fn active() -> Option<Player> {
    PLAYERS.with(|p| {
        let p = p.borrow();
        p.iter().find(|x| x.state == "Playing").or_else(|| p.iter().find(|x| x.state == "Paused")).cloned()
    })
}

/// Player popup: call on every change (None = the popup closed).
pub fn on_change(f: Option<Box<dyn Fn()>>) {
    ON_CHANGE.with(|p| p.replace(f));
}

fn icon_players(name: &str) -> &'static str {
    let j = name.to_lowercase();
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

/// Bar: the track (click = player popup) and ⏮ ⏭ buttons, visible only while something plays
/// or is paused.
fn refresh() {
    match active() {
        Some(p) => {
            let mut text = [p.title.as_str(), p.artist.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" - ");
            if text.is_empty() {
                text = p.identity.clone();
            }
            if text.chars().count() > LENGTH_TEXT {
                text = text.chars().take(LENGTH_TEXT - 1).collect::<String>() + "…";
            }
            let (text, class) = if p.state == "Playing" {
                (format!("{}  {}", icon_players(&p.name), escape(&text)), "playing")
            } else {
                (format!("\u{f04c}  <i>{}</i>", escape(&text)), "paused")
            };
            publish("music", json!({"text": text, "class": class}));
            publish("previous", json!({"text": "\u{f048}", "class": class}));
            publish("next", json!({"text": "\u{f051}", "class": class}));
        }
        None => {
            for m in ["music", "previous", "next"] {
                publish(m, json!({"text": ""}));
            }
        }
    }
    ON_CHANGE.with(|p| {
        if let Some(f) = p.borrow().as_ref() {
            f();
        }
    });
}

/// Controls: previous, next, play-pause (from the bar, keys and the popup).
pub fn control(action: &str) {
    // play-pause can start a stopped player too
    let target = active().or_else(|| PLAYERS.with(|p| p.borrow().first().cloned()));
    let Some(p) = target else { return };
    let method = match action {
        "previous" => "Previous",
        "next" => "Next",
        "play-pause" => "PlayPause",
        _ => return,
    };
    call(&p.name, INTERFACE, method, None, |_| {});
}

/// Where the player is (in microseconds).
pub fn position(p: &Player, done: impl FnOnce(i64) + 'static) {
    call(&p.name, "org.freedesktop.DBus.Properties", "Get", Some((INTERFACE, "Position").into()), move |v| {
        if let Some((v,)) = v.and_then(|v| v.get::<(Variant,)>()) {
            done(number(&v));
        }
    });
}

/// Seek to the given position (in microseconds).
pub fn seek(p: &Player, target: i64) {
    let Ok(stopa) = glib::variant::ObjectPath::try_from(p.stopa.clone()) else { return };
    call(&p.name, INTERFACE, "SetPosition", Some((stopa, target).into()), |_| {});
}
