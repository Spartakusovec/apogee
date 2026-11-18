//! Shared pieces of the popups: the window under the bar, closing, building blocks and background commands.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

pub fn run_dir() -> PathBuf {
    PathBuf::from(std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()))
}

pub fn scripts() -> PathBuf {
    home().join(".config/sway/scripts")
}

fn file_settings() -> PathBuf {
    home().join(".config/apogee/settings.json")
}

/// Remembered choices (screenshots, pomodoro…) in ~/.config/apogee/settings.json.
pub fn settings(key: &str) -> serde_json::Value {
    std::fs::read_to_string(file_settings())
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .map(|v| v[key].clone())
        .unwrap_or(serde_json::Value::Null)
}

pub fn save_settings(key: &str, value: serde_json::Value) {
    let file = file_settings();
    let mut vse = std::fs::read_to_string(&file)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    vse[key] = value;
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let _ = std::fs::write(file, serde_json::to_string_pretty(&vse).unwrap_or_default());
}

/// Runs a command and returns its output (trimmed).
pub fn command(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Runs several commands at once and returns their outputs (waits only for the slowest one).
pub fn current(commands: &[(&str, &[&str])]) -> Vec<String> {
    let deti: Vec<_> = commands
        .iter()
        .map(|(program, args)| Command::new(program).args(*args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok())
        .collect();
    deti.into_iter()
        .map(|d| {
            d.and_then(|d| d.wait_with_output().ok())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default()
        })
        .collect()
}

/// A property over D-Bus (system or session bus): faster than running nmcli,
/// bluetoothctl and friends.
pub fn dbus_property(system: bool, target: &str, path: &str, interface: &str, name: &str) -> Option<glib::Variant> {
    let typ = if system { gtk::gio::BusType::System } else { gtk::gio::BusType::Session };
    let bus = gtk::gio::bus_get_sync(typ, gtk::gio::Cancellable::NONE).ok()?;
    let v = bus
        .call_sync(
            Some(target),
            path,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&(interface, name).to_variant()),
            None,
            gtk::gio::DBusCallFlags::NONE,
            500,
            gtk::gio::Cancellable::NONE,
        )
        .ok()?;
    v.get::<(glib::Variant,)>().map(|(v,)| v)
}

/// All properties of an interface over D-Bus (empty when that fails).
pub fn dbus_vse(system: bool, target: &str, path: &str, interface: &str) -> std::collections::HashMap<String, glib::Variant> {
    let typ = if system { gtk::gio::BusType::System } else { gtk::gio::BusType::Session };
    gtk::gio::bus_get_sync(typ, gtk::gio::Cancellable::NONE)
        .ok()
        .and_then(|s| {
            s.call_sync(
                Some(target),
                path,
                "org.freedesktop.DBus.Properties",
                "GetAll",
                Some(&(interface,).to_variant()),
                None,
                gtk::gio::DBusCallFlags::NONE,
                500,
                gtk::gio::Cancellable::NONE,
            )
            .ok()
        })
        .and_then(|v| v.get::<(std::collections::HashMap<String, glib::Variant>,)>())
        .map(|(m,)| m)
        .unwrap_or_default()
}

/// Battery from UPower (it tracks it anyway; reading /sys/class/power_supply takes up to 70 ms).
pub struct Battery {
    pub percent: f64,
    /// 1 charging, 2 discharging, 4 full, 5 plugged in and not charging
    pub state: u32,
    pub to_empty: i64,
    pub to_full: i64,
    /// W
    pub power_draw: f64,
    /// battery health in %
    pub health: f64,
}

pub fn battery() -> Option<Battery> {
    let m = dbus_vse(true, "org.freedesktop.UPower", "/org/freedesktop/UPower/devices/battery_BAT0", "org.freedesktop.UPower.Device");
    let d = |k: &str| m.get(k).and_then(|v| v.get::<f64>()).unwrap_or(0.0);
    let x = |k: &str| m.get(k).and_then(|v| v.get::<i64>()).unwrap_or(0);
    Some(Battery {
        percent: m.get("Percentage")?.get::<f64>()?,
        state: m.get("State").and_then(|v| v.get::<u32>()).unwrap_or(0),
        to_empty: x("TimeToEmpty"),
        to_full: x("TimeToFull"),
        power_draw: d("EnergyRate"),
        health: d("Capacity"),
    })
}

/// Is Wi-Fi on (NetworkManager)?
pub fn wifi_enabled() -> bool {
    dbus_property(true, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager", "WirelessEnabled")
        .and_then(|v| v.get::<bool>())
        .unwrap_or(false)
}

/// Is Bluetooth on (bluez)?
pub fn bluetooth_enabled() -> bool {
    dbus_property(true, "org.bluez", "/org/bluez/hci0", "org.bluez.Adapter1", "Powered").and_then(|v| v.get::<bool>()).unwrap_or(false)
}

/// Power mode (power-saver, balanced, performance) through the power-profiles interface (tuned).
pub fn mode_power() -> String {
    dbus_property(true, "net.hadess.PowerProfiles", "/net/hadess/PowerProfiles", "net.hadess.PowerProfiles", "ActiveProfile")
        .and_then(|v| v.get::<String>())
        .unwrap_or_default()
}

/// Is night light on? (from the night-light.sh settings; wlsunset is running then)
pub fn night_mode() -> bool {
    std::fs::read_to_string(home().join(".config/sway/night-light.conf"))
        .map(|t| t.lines().any(|r| r.starts_with("mode=") && r != "mode=off"))
        .unwrap_or(false)
}

/// Runs a command in the background and doesn't wait for it.
pub fn background(program: &str, args: &[&str]) {
    use std::os::unix::process::CommandExt;
    let _ = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
}

/// Does slow work (nmcli, bluetoothctl…) outside GTK and hands the result back to GTK.
pub fn in_background<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    let task = gtk::gio::spawn_blocking(work);
    glib::spawn_future_local(async move {
        if let Ok(result) = task.await {
            done(result);
        }
    });
}

/// A popup under the bar.
pub struct Popup {
    pub window: gtk::Window,
    pub panel: gtk::Box,
    /// don't close while a password is being typed, not even when the mouse leaves
    pub keep_open: Cell<bool>,
    closer: RefCell<Option<glib::SourceId>>,
    state_lbl: RefCell<Option<gtk::Label>>,
    on_close: RefCell<Option<Box<dyn Fn()>>>,
    closed: Cell<bool>,
    /// under which bar module (centre, screen width), when it was opened by clicking the bar
    anchor: Option<(f64, i32)>,
}

thread_local! {
    /// Where the next popup belongs: (centre of the clicked bar module in px from the left screen
    /// edge; screen width). Set by the bar on click, used up by the first popup that opens.
    /// (valid only briefly after the click: the click that closed a popup must not leave it to the next one)
    static ANCHOR: Cell<Option<(f64, i32, std::time::Instant)>> = const { Cell::new(None) };
}

pub fn set_anchor(center: f64, width_screen: i32) {
    ANCHOR.with(|k| k.set(Some((center, width_screen, std::time::Instant::now()))));
}

/// The popup horizontally under the module (centre under centre), but fully on screen.
fn place_pod(window: &gtk::Window, center: f64, width_screen: i32, width: i32) {
    let left = (center - width as f64 / 2.0).round() as i32;
    window.set_margin(Edge::Left, left.clamp(8, (width_screen - width - 8).max(8)));
}

pub enum Placement {
    /// in the middle of the screen (window menu)
    Center,
    Right,
    Middle,
}

impl Popup {
    pub fn new(placement: Placement, width: i32, on_close: impl Fn() + 'static) -> Rc<Popup> {
        crate::style::apply();
        let window = gtk::Window::new();
        window.add_css_class("apogee");
        window.init_layer_shell();
        window.set_layer(Layer::Overlay);
        window.set_anchor(Edge::Top, true);
        // the bar reserves a strip at the top, the margin counts from below it
        window.set_margin(Edge::Top, 6);
        let anchor = ANCHOR
            .with(|k| k.take())
            .filter(|(_, _, when)| when.elapsed() < std::time::Duration::from_millis(700))
            .map(|(center, width, _)| (center, width));
        match (anchor, placement) {
            // opened by clicking the bar: under the clicked module
            // (show() works out the exact position from the measured width, before showing)
            (Some((center, screen)), _) => {
                window.set_anchor(Edge::Left, true);
                place_pod(&window, center, screen, width + 38);
            }
            (None, Placement::Right) => {
                window.set_anchor(Edge::Right, true);
                window.set_margin(Edge::Right, 8);
            }
            // without anchors Sway puts it in the middle of the screen
            (None, Placement::Center) => window.set_anchor(Edge::Top, false),
            (None, Placement::Middle) => {}
        }
        window.set_keyboard_mode(KeyboardMode::OnDemand);
        window.set_namespace(Some("apogee"));

        let panel = gtk::Box::new(gtk::Orientation::Vertical, 10);
        panel.set_widget_name("panel");
        panel.set_size_request(width, -1);
        window.set_child(Some(&panel));

        let o = Rc::new(Popup {
            window,
            panel,
            keep_open: Cell::new(false),
            closer: RefCell::new(None),
            state_lbl: RefCell::new(None),
            on_close: RefCell::new(Some(Box::new(on_close))),
            closed: Cell::new(false),
            anchor,
        });

        // Esc closes
        let keys = gtk::EventControllerKey::new();
        let slaby = Rc::downgrade(&o);
        keys.connect_key_pressed(move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                if let Some(o) = slaby.upgrade() {
                    o.close();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        o.window.add_controller(keys);

        // close when the mouse leaves (with a delay), or when it never gets into the popup
        let mouse = gtk::EventControllerMotion::new();
        let slaby = Rc::downgrade(&o);
        mouse.connect_enter(move |_, _, _| {
            if let Some(o) = slaby.upgrade() {
                o.cancel_closing();
            }
        });
        let slaby = Rc::downgrade(&o);
        mouse.connect_leave(move |_| {
            if let Some(o) = slaby.upgrade() {
                if !o.keep_open.get() {
                    o.close_after(900);
                }
            }
        });
        o.window.add_controller(mouse);
        o.close_after(8000);

        o
    }

    pub fn show(&self) {
        // measure the content width before showing, so the popup doesn't jump into place after drawing
        if let Some((center, screen)) = self.anchor {
            let (_, width, _, _) = self.window.measure(gtk::Orientation::Horizontal, -1);
            if width > 0 {
                place_pod(&self.window, center, screen, width);
            }
        }
        self.window.present();
    }

    pub fn close(&self) {
        self.cancel_closing();
        if self.closed.replace(true) {
            return;
        }
        self.window.destroy();
        if let Some(f) = self.on_close.take() {
            f();
        }
        // GTK 4 frees a window when nothing refers to it, but the closures of buttons and controllers hold
        // the popup's data, which holds the window. Take the content apart and remove the controllers (only
        // after the event that closed the popup is handled), otherwise every opening would stay in memory.
        let (window, panel) = (self.window.clone(), self.panel.clone());
        glib::idle_add_local_once(move || {
            // the window holds the focused widget (whose closure holds the popup's data again)
            gtk::prelude::GtkWindowExt::set_focus(&window, None::<&gtk::Widget>);
            window.set_default_widget(None::<&gtk::Widget>);
            parse(panel.upcast_ref());
            remove_controls(&window);
            return_memory();
        });
    }

    /// A keyboard-driven popup (clipboard): don't close by itself, not even when the mouse leaves.
    pub fn keep_open_itself(&self) {
        self.cancel_closing();
        self.keep_open.set(true);
    }

    /// The popup is already closed (slow background work must not fill it again).
    pub fn closed(&self) -> bool {
        self.closed.get()
    }

    fn close_after(self: &Rc<Self>, ms: u64) {
        self.cancel_closing();
        let slaby = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(ms), move || {
            if let Some(o) = slaby.upgrade() {
                o.closer.replace(None);
                o.close();
            }
        });
        self.closer.replace(Some(id));
    }

    fn cancel_closing(&self) {
        if let Some(id) = self.closer.take() {
            id.remove();
        }
    }

    /// After the content changes, the window shrinks / grows to fit it.
    pub fn recompute(&self) {
        self.window.set_default_size(1, 1);
        self.window.queue_resize();
    }

    /// A small status line at the bottom (Connecting…, error…).
    pub fn state(&self, text: &str) {
        let mut lbl = self.state_lbl.borrow_mut();
        if lbl.is_none() {
            let l = label("", &["dim"]);
            self.panel.append(&l);
            *lbl = Some(l);
        }
        if let Some(l) = lbl.as_ref() {
            l.set_text(text);
            l.set_visible(true);
        }
    }

    pub fn clear_content(&self) {
        while let Some(dite) = self.panel.first_child() {
            self.panel.remove(&dite);
        }
        self.state_lbl.replace(None);
    }
}

/// Takes the children out of every Box and Grid (from the leaves up): widgets without a parent are freed
/// together with their closures, which lets go of the popup data they hold.
fn parse(w: &gtk::Widget) {
    let mut dite = w.first_child();
    while let Some(d) = dite {
        dite = d.next_sibling();
        parse(&d);
    }
    if let Some(b) = w.downcast_ref::<gtk::Box>() {
        while let Some(d) = b.first_child() {
            b.remove(&d);
        }
    } else if let Some(g) = w.downcast_ref::<gtk::Grid>() {
        while let Some(d) = g.first_child() {
            g.remove(&d);
        }
    }
}

/// Removes the window's controllers (mouse, keys…) together with their closures.
pub fn remove_controls(window: &gtk::Window) {
    let list = window.observe_controllers();
    let controls: Vec<gtk::EventController> = (0..list.n_items()).filter_map(|i| list.item(i).and_then(|c| c.downcast().ok())).collect();
    for c in controls {
        window.remove_controller(&c);
    }
}

/// After a popup closes, give back to the system the memory glibc keeps for later
/// (without this the daemon holds tens of MB extra after opening a few popups).
pub fn return_memory() {
    glib::timeout_add_local_once(Duration::from_secs(2), || unsafe {
        libc::malloc_trim(0);
    });
}

// ---------- building blocks ----------

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    for t in classes {
        l.add_css_class(t);
    }
    l
}

pub fn icon(char: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(char));
    l.add_css_class(class);
    l
}

pub fn separator() -> gtk::Separator {
    gtk::Separator::new(gtk::Orientation::Horizontal)
}

pub fn title(text: &str, description: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
    b.append(&label(text, &["title"]));
    if !description.is_empty() {
        b.append(&label(description, &["dim"]));
    }
    b
}

/// A link at the bottom of a popup (e.g. “Sound settings…”): starts a program and closes the popup.
pub fn link(o: &Rc<Popup>, text: &str, program: &'static str, args: &'static [&'static str]) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("link");
    b.set_halign(gtk::Align::Start);
    let slaby = Rc::downgrade(o);
    b.connect_clicked(move |_| {
        background(program, args);
        if let Some(o) = slaby.upgrade() {
            o.close();
        }
    });
    b
}

/// A link from a popup to a page of the Settings app (closes the popup).
pub fn link_settings(o: &Rc<Popup>, text: &str, page: &'static str) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("link");
    b.set_halign(gtk::Align::Start);
    let slaby = Rc::downgrade(o);
    b.connect_clicked(move |_| {
        if let Some(o) = slaby.upgrade() {
            o.close();
        }
        glib::idle_add_local_once(move || crate::settings::open(page));
    });
    b
}

/// A heading with a switch on the right (Wi-Fi, Bluetooth).
pub fn header_s_switch(text: &str, enabled: bool, on_change: impl Fn(bool) + 'static) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let lbl = label(text, &["title"]);
    lbl.set_hexpand(true);
    row.append(&lbl);
    let sw = gtk::Switch::new();
    sw.set_active(enabled);
    sw.set_valign(gtk::Align::Center);
    sw.connect_active_notify(move |s| on_change(s.is_active()));
    row.append(&sw);
    row
}

/// A list row: icon, text, grey text on the right; click runs the action.
pub fn row(char: &str, text: &str, right: &str, active: bool, action: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("row");
    if active {
        b.add_css_class("active");
    }
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let ik = icon(char, "icon");
    ik.set_size_request(22, -1);
    content.append(&ik);
    let t = label(text, &[]);
    t.set_hexpand(true);
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&t);
    if !right.is_empty() {
        content.append(&label(right, &["right"]));
    }
    b.set_child(Some(&content));
    b.connect_clicked(move |_| action());
    b
}

/// A slider with an icon (or button) on the left and the value on the right; change is called with a short delay.
pub fn slider(
    left: &impl IsA<gtk::Widget>,
    value: f64,
    min: f64,
    max: f64,
    format_values: fn(f64) -> String,
    change: impl Fn(f64) + 'static,
) -> (gtk::Box, gtk::Scale) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.append(left);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, 1.0);
    scale.set_draw_value(false);
    scale.set_value(value);
    scale.set_hexpand(true);
    let value_lbl = label(&format_values(value), &["value"]);
    value_lbl.set_xalign(1.0);
    row.append(&scale);
    row.append(&value_lbl);
    let wait: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let change = Rc::new(change);
    scale.connect_value_changed(move |s| {
        let v = s.value();
        value_lbl.set_text(&format_values(v));
        if let Some(id) = wait.take() {
            id.remove();
        }
        let change = change.clone();
        let c = wait.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(40), move || {
            c.replace(None);
            change(v);
        });
        wait.replace(Some(id));
    });
    (row, scale)
}

pub fn percent(v: f64) -> String {
    format!("{} %", v.round() as i64)
}

/// Screen backlight: the first device in /sys/class/backlight (intel_backlight, amdgpu_bl0,
/// acpi_video0…); raw/platform ones win over firmware (the kernel's recommendation).
pub fn backlight() -> Option<String> {
    let mut v: Vec<(u8, String)> = std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let typ = std::fs::read_to_string(e.path().join("type")).unwrap_or_default();
            let order = match typ.trim() { "raw" => 0, "platform" => 1, _ => 2 };
            (order, name)
        })
        .collect();
    v.sort();
    v.into_iter().next().map(|(_, j)| j)
}

/// A value from the backlight files (brightness, max_brightness).
pub fn read_backlight(file: &str) -> Option<f64> {
    let z = backlight()?;
    std::fs::read_to_string(format!("/sys/class/backlight/{z}/{file}")).ok()?.trim().parse().ok()
}
