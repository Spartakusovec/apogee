//! Notifications (was mako): the daemon is the notification server (org.freedesktop.Notifications), shows
//! bubbles at the top right (bubbles.rs) and keeps the history for the popup under the bell.
//!
//! Do Not Disturb: no bubbles (except critical ones), notifications only go to the history.
//! The history (at most 50) is saved in ~/.cache/apogee/notifications.json and survives a daemon restart.
//! Images from notifications (image-data) are saved as PNG in ~/.cache/apogee/notifications/.

pub mod bubbles;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;
use serde_json::{json, Value};

use crate::common::*;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const MAX_V_HISTORY: usize = 50;
const DEFAULT_DURATION_MS: u32 = 6000;

const XML: &str = r#"<node><interface name="org.freedesktop.Notifications">
<method name="GetCapabilities"><arg type="as" direction="out"/></method>
<method name="Notify">
 <arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/>
 <arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/>
 <arg type="u" direction="out"/>
</method>
<method name="CloseNotification"><arg type="u" direction="in"/></method>
<method name="GetServerInformation">
 <arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/>
</method>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
</interface></node>"#;

/// Why a notification disappeared (number from the specification)
#[derive(Clone, Copy)]
pub enum Reason {
    Expired = 1,
    Closed = 2,
    App = 3,
}

#[derive(Clone)]
pub struct Notification {
    pub id: u32,
    pub apps: String,
    /// icon name or image path
    pub icon: String,
    pub desktop: String,
    pub title: String,
    pub body: String,
    /// (key, label); “default” = click on the bubble
    pub action: Vec<(String, String)>,
    /// 0 low, 1 normal, 2 critical
    pub urgency: u8,
    pub time: f64,
    pub read: bool,
    /// only a bubble, not in the history (hint transient)
    pub transient: bool,
    /// progress 0–100 (hint value)
    pub progress: Option<i32>,
    /// x-canonical-private-synchronous / x-dunst-stack-tag: replaces an older one with the same tag
    pub tag: Option<String>,
    /// after how many ms the bubble disappears (0 = never)
    pub duration_ms: u32,
}

thread_local! {
    static HISTORY: RefCell<Vec<Notification>> = RefCell::new(load_history());
    static NEXT_ID: Cell<u32> = const { Cell::new(0) };
    static BUS: RefCell<Option<gio::DBusConnection>> = const { RefCell::new(None) };
}

fn file_history() -> PathBuf {
    home().join(".cache/apogee/notifications.json")
}

fn folder_images() -> PathBuf {
    home().join(".cache/apogee/notifications")
}

fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn load_history() -> Vec<Notification> {
    let v: Value = std::fs::read_to_string(file_history()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|n| Notification {
                    id: n["id"].as_u64().unwrap_or(0) as u32,
                    apps: n["app_name"].as_str().unwrap_or("").into(),
                    icon: n["app_icon"].as_str().unwrap_or("").into(),
                    desktop: n["desktop_entry"].as_str().unwrap_or("").into(),
                    title: n["summary"].as_str().unwrap_or("").into(),
                    body: n["body"].as_str().unwrap_or("").into(),
                    action: Vec::new(),
                    urgency: n["urgency"].as_u64().unwrap_or(1) as u8,
                    time: n["time"].as_f64().unwrap_or(0.0),
                    read: n["read"].as_bool().unwrap_or(true),
                    transient: false,
                    progress: None,
                    tag: None,
                    duration_ms: 0,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn save_history() {
    let v: Vec<Value> = HISTORY.with(|h| h.borrow().iter().map(to_json).collect());
    if let Some(s) = file_history().parent() {
        let _ = std::fs::create_dir_all(s);
    }
    let _ = std::fs::write(file_history(), Value::Array(v).to_string());
}

/// A notification in the shape the popup under the bell reads.
pub fn to_json(n: &Notification) -> Value {
    json!({
        "id": n.id, "app_name": n.apps, "app_icon": n.icon, "desktop_entry": n.desktop,
        "summary": n.title, "body": n.body, "urgency": n.urgency, "time": n.time, "read": n.read,
    })
}

/// History, newest first.
pub fn history() -> Vec<Notification> {
    HISTORY.with(|h| h.borrow().iter().rev().cloned().collect())
}

pub fn unread() -> usize {
    HISTORY.with(|h| h.borrow().iter().filter(|n| !n.read).count())
}

pub fn mark_read() {
    HISTORY.with(|h| h.borrow_mut().iter_mut().for_each(|n| n.read = true));
    save_history();
}

/// Delete from the history (and close the bubble if visible).
pub fn clear(id: u32) {
    HISTORY.with(|h| h.borrow_mut().retain(|n| n.id != id));
    bubbles::close(id, Reason::Closed);
    save_history();
}

pub fn clear_vse() {
    let id: Vec<u32> = HISTORY.with(|h| h.borrow_mut().drain(..).map(|n| n.id).collect());
    for i in id {
        bubbles::close(i, Reason::Closed);
    }
    save_history();
}

/// Apps the user muted in Settings (settings.json `muted_apps`).
pub fn muted(apps: &str) -> bool {
    !apps.is_empty() && settings("muted_apps").as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(apps)))
}

/// Apps that ever sent a notification (from the history), for the list in Settings.
pub fn apps_v_history() -> Vec<String> {
    let mut v: Vec<String> = HISTORY.with(|h| h.borrow().iter().map(|n| n.apps.clone()).filter(|a| !a.is_empty()).collect());
    v.sort_by_key(|a| a.to_lowercase());
    v.dedup();
    v
}

/// Scheduled Do Not Disturb (`dnd_schedule`: {enabled, from, to} in minutes after midnight): checked every
/// minute; it switches only at the edges of the interval, so a manual switch in between stays.
fn watch_dnd_time() {
    use chrono::Timelike;
    thread_local! { static WAS: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
    let c = settings("dnd_schedule");
    if c["enabled"].as_bool().unwrap_or(false) {
        let (from, to_) = (c["from"].as_u64().unwrap_or(22 * 60) as u32, c["to"].as_u64().unwrap_or(7 * 60) as u32);
        let t = chrono::Local::now();
        let m = t.hour() * 60 + t.minute();
        let inside = if from <= to_ { m >= from && m < to_ } else { m >= from || m < to_ };
        if WAS.with(|b| b.get()) != Some(inside) {
            if WAS.with(|b| b.get()).is_some() || inside {
                crate::popups::command("dnd", if inside { "on" } else { "off" });
            }
            WAS.with(|b| b.set(Some(inside)));
        }
    } else {
        WAS.with(|b| b.set(None));
    }
}

pub fn dnd() -> bool {
    settings("dnd").as_bool().unwrap_or(false)
}

pub fn set_dnd(enable: bool) {
    save_settings("dnd", json!(enable));
    if enable {
        bubbles::hide_non_critical();
    }
}

/// A notification closed (expired, closed by the user): tell the app.
pub fn announce_close(id: u32, reason: Reason) {
    if let Some(s) = BUS.with(|s| s.borrow().clone()) {
        let _ = s.emit_signal(None, PATH, NAME, "NotificationClosed", Some(&(id, reason as u32).to_variant()));
    }
}

/// The user picked an action (click on the bubble = “default”).
pub fn invoke_action(id: u32, key: &str) {
    if let Some(s) = BUS.with(|s| s.borrow().clone()) {
        let _ = s.emit_signal(None, PATH, NAME, "ActionInvoked", Some(&(id, key).to_variant()));
    }
}

/// Start the server (when the daemon starts). If another program (mako) holds the name, the daemon waits in the queue.
pub fn start() {
    glib::timeout_add_seconds_local_once(5, watch_dnd_time);
    glib::timeout_add_seconds_local(30, || {
        watch_dnd_time();
        glib::ControlFlow::Continue
    });
    let Ok(s) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else { return };
    BUS.with(|x| x.replace(Some(s.clone())));
    NEXT_ID.with(|d| d.set(HISTORY.with(|h| h.borrow().iter().map(|n| n.id).max().unwrap_or(0))));
    let Ok(node) = gio::DBusNodeInfo::for_xml(XML) else { return };
    let Some(interface) = node.lookup_interface(NAME) else { return };
    let ok = s
        .register_object(PATH, &interface)
        .method_call(|_, sender, _, _, method, params, invocation| match method {
            "GetCapabilities" => {
                let capabilities = vec!["actions", "body", "body-markup", "icon-static", "persistence", "x-canonical-private-synchronous"];
                invocation.return_value(Some(&(capabilities,).to_variant()));
            }
            "GetServerInformation" => invocation.return_value(Some(&("apogee", "Spartakusovec", "1.0", "1.2").to_variant())),
            "CloseNotification" => {
                if let Some((id,)) = params.get::<(u32,)>() {
                    bubbles::close(id, Reason::App);
                }
                invocation.return_value(None);
            }
            "Notify" => {
                let id = accept(&params, sender.unwrap_or(""));
                invocation.return_value(Some(&(id,).to_variant()));
            }
            _ => invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", "Unknown method"),
        })
        .build();
    if ok.is_err() {
        return;
    }
    let id = gio::bus_own_name_on_connection(&s, NAME, gio::BusNameOwnerFlags::ALLOW_REPLACEMENT | gio::BusNameOwnerFlags::REPLACE, |_, _| {}, |_, _| {});
    std::mem::forget(id);
    crate::popups::command("notifications-change", "");
}

/// Image from the image-data hint (iiibiiay) → PNG in the cache, returns the path.
fn save_image(id: u32, v: &Variant) -> Option<String> {
    let (width, height, row, alfa, _bits, channels, data) = v.get::<(i32, i32, i32, bool, i32, i32, Vec<u8>)>()?;
    if width <= 0 || height <= 0 || channels < 3 {
        return None;
    }
    let image = gtk::gdk_pixbuf::Pixbuf::from_bytes(&glib::Bytes::from_owned(data), gtk::gdk_pixbuf::Colorspace::Rgb, alfa, 8, width, height, row);
    let _ = std::fs::create_dir_all(folder_images());
    let path = folder_images().join(format!("{id}.png"));
    let small = if width > 128 || height > 128 {
        let m = 128.0 / width.max(height) as f64;
        image.scale_simple((width as f64 * m) as i32, (height as f64 * m) as i32, gtk::gdk_pixbuf::InterpType::Bilinear)?
    } else {
        image
    };
    small.savev(&path, "png", &[]).ok()?;
    Some(path.to_string_lossy().to_string())
}

/// Notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout) → id
fn accept(p: &Variant, _sender: &str) -> u32 {
    let Some((apps, replace, icon, title, body, action, hints, timeout)) =
        p.get::<(String, u32, String, String, String, Vec<String>, HashMap<String, Variant>, i32)>()
    else {
        return 0;
    };
    let text = |k: &str| hints.get(k).and_then(|v| v.str().map(String::from));
    let tag = text("x-canonical-private-synchronous").or_else(|| text("x-dunst-stack-tag"));
    // replace: by replaces_id, or an older one with the same tag
    let existing = HISTORY.with(|h| {
        let h = h.borrow();
        if replace > 0 && h.iter().any(|n| n.id == replace) {
            Some(replace)
        } else {
            tag.as_ref().and_then(|z| h.iter().rev().find(|n| n.tag.as_ref() == Some(z)).map(|n| n.id))
        }
    }).or_else(|| (replace > 0 && bubbles::is_visible(replace)).then_some(replace));
    let id = existing.unwrap_or_else(|| {
        NEXT_ID.with(|d| {
            d.set(d.get() + 1);
            d.get()
        })
    });
    let urgency = hints.get("urgency").and_then(|v| v.get::<u8>()).unwrap_or(1);
    let image = ["image-data", "image_data", "icon_data"].iter().find_map(|k| hints.get(*k)).and_then(|v| save_image(id, v));
    let path_images = text("image-path").or_else(|| text("image_path"));
    let icon = image
        .or(path_images)
        .unwrap_or(icon)
        .trim_start_matches("file://")
        .to_string();
    let n = Notification {
        id,
        apps,
        icon,
        desktop: text("desktop-entry").unwrap_or_default(),
        title,
        body,
        action: action.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect(),
        urgency,
        time: now(),
        read: false,
        transient: hints.get("transient").and_then(|v| v.get::<bool>()).unwrap_or(false),
        progress: hints.get("value").and_then(|v| v.get::<i32>()),
        tag,
        duration_ms: if urgency == 2 || timeout == 0 { 0 } else if timeout < 0 { DEFAULT_DURATION_MS } else { timeout as u32 },
    };
    if !n.transient {
        HISTORY.with(|h| {
            let mut h = h.borrow_mut();
            h.retain(|x| x.id != id);
            h.push(n.clone());
            let nadbytek = h.len().saturating_sub(MAX_V_HISTORY);
            h.drain(..nadbytek);
        });
        save_history();
    }
    // muted apps (Settings → Notifications): only to the history, no bubble
    if (!dnd() && !muted(&n.apps)) || urgency == 2 {
        bubbles::show(&n);
    }
    crate::popups::command("notifications-change", "");
    id
}
