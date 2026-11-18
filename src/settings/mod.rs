//! The Settings app (`apogee settings [page]`): one window for settings that otherwise
//! need a terminal. Design: docs/settings-design.md.
//!
//! A normal window (move, maximize, minimize), search and pages on the left, content on the
//! right. A page is built every time it's switched to (shows the fresh state); data loads
//! in the background. Changes apply at once, a short toast shows at the bottom. The whole window
//! is destroyed on close (the daemon doesn't keep anything extra in the background).

mod updates;
mod apps;
mod bluetooth;
mod time;
mod display;
mod sway_conf;
mod power;
mod network;
mod about;
mod windows;
mod notifications;
mod panel_dock;
mod ui;
mod storage;
mod user;
mod input;
mod appearance;
mod wifi;
mod wifi_details;
mod shortcuts;
mod sound;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

/// page: id (for `apogee settings <id>`), name, icon (Font Awesome), search words
const PAGES: &[(&str, &str, &str, &str)] = &[
    ("wifi", "Wi-Fi", "\u{f1eb}", "wifi wi-fi wireless network internet password hidden airplane sit heslo"),
    ("network", "Network", "\u{f6ff}", "network wired ethernet cable vpn wireguard openvpn hotspot share internet airplane flight sit kabel letadlo"),
    ("bluetooth", "Bluetooth", "\u{f293}", "bluetooth headphones headset speaker mouse keyboard pair devices sluchatka"),
    ("sound", "Sound", "\u{f028}", "sound audio volume speakers headphones microphone output input apps mute zvuk hlasitost mikrofon"),
    ("display", "Display", "\u{f108}", "display screen brightness night light warm scale size resolution jas displej obrazovka nocni"),
    ("input", "Keyboard & Touchpad", "\u{f11c}", "keyboard touchpad mouse layout czech english language typing repeat numlock tap click drag scroll natural speed pointer gestures klavesnice mys"),
    ("power", "Power & Battery", "\u{f240}", "power battery saver balanced performance health conservation charge charging screen lock dim idle sleep blank baterie napajeni zamknout"),
    ("appearance", "Appearance", "\u{f53f}", "appearance wallpaper background colors theme text size font buttons tapeta pozadi barvy vzhled pismo"),
    ("panel_dock", "Panel & Dock", "\u{f2d0}", "panel bar top clock date seconds 12-hour modules weather dock pinned apps hide autohide lista hodiny"),
    ("windows", "Windows & Workspaces", "\u{f2d2}", "windows workspaces tiling floating gaps focus snap maximize okna plochy dlazdice"),
    ("notifications", "Notifications", "\u{f0f3}", "notifications do not disturb dnd quiet night mute apps oznameni nerusit"),
    ("shortcuts", "Shortcuts", "\u{f0e7}", "shortcuts keys keyboard hotkeys gestures zkratky klavesy"),
    ("apps", "Apps", "\u{f00a}", "apps default browser video music pictures pdf email startup autostart login aplikace vychozi"),
    ("time", "Date & Time", "\u{f017}", "date time timezone zone clock ntp automatic region formats language datum cas"),
    ("user", "Users", "\u{f007}", "users account user name picture photo avatar password login uzivatel heslo ucet"),
    ("storage", "Storage", "\u{f0a0}", "storage disk space free clean trash cache flatpak downloads uloziste misto"),
    ("updates", "Updates", "\u{f019}", "updates upgrade dnf flatpak firmware bios fwupd sway rebuild aktualizace"),
    ("about", "About", "\u{f05a}", "about computer system hardware processor memory ram disk storage graphics nvidia version backup restore fedora sway"),
];

/// group headings in the sidebar: (first page of the group, heading)
const GROUPS: &[(&str, &str)] = &[("wifi", "Connections"), ("sound", "Device"), ("appearance", "Personalization"), ("apps", "System")];

pub struct Settings {
    pub window: gtk::Window,
    content: gtk::ScrolledWindow,
    list: gtk::ListBox,
    search: gtk::SearchEntry,
    toast: gtk::Revealer,
    toast_text: gtk::Label,
    toast_timer: RefCell<Option<glib::SourceId>>,
    active: RefCell<String>,
}

thread_local! {
    static SETTINGS: RefCell<Option<Rc<Settings>>> = const { RefCell::new(None) };
}

/// Opens the window (or brings it to the front) on the given page (empty = the last one opened).
pub fn open(page: &str) {
    let n = SETTINGS.with(|n| n.borrow().clone()).unwrap_or_else(create);
    if page == "eduroam" {
        n.show("wifi");
        wifi_details::enterprise("");
        n.window.present();
        return;
    }
    if let Some(network) = page.strip_prefix("wifi:") {
        n.show("wifi");
        wifi_details::details(network);
        n.window.present();
        return;
    }
    let id = PAGES.iter().find(|s| s.0 == page).map(|s| s.0.to_string()).unwrap_or_else(|| {
        let a = n.active.borrow().clone();
        if a.is_empty() { PAGES[0].0.to_string() } else { a }
    });
    n.show(&id);
    n.window.present();
}

fn create() -> Rc<Settings> {
    crate::style::apply();
    // Sway and the dock recognise the app by this (apogee-settings.desktop: Settings, icon)
    glib::set_prgname(Some("apogee-settings"));
    let window = gtk::Window::new();
    window.set_title(Some("Settings"));
    window.set_default_size(920, 640);
    window.add_css_class("settings");

    // search and pages on the left
    let leva = gtk::Box::new(gtk::Orientation::Vertical, 10);
    leva.add_css_class("sidebar");
    leva.set_size_request(230, -1);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search settings"));
    search.set_search_delay(0);
    leva.append(&search);
    let list = gtk::ListBox::new();
    list.add_css_class("pages");
    list.set_selection_mode(gtk::SelectionMode::Single);
    for (id, name, icon, _) in PAGES {
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let i = gtk::Label::new(Some(icon));
        i.add_css_class("icon");
        i.set_width_chars(2);
        r.append(&i);
        r.append(&gtk::Label::new(Some(name)));
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&r));
        row.set_widget_name(id);
        list.append(&row);
    }
    // group headings (Connections, Device…), hidden while searching
    list.set_header_func(|row, _| {
        let id = row.widget_name();
        let title = GROUPS.iter().find(|s| s.0 == id).map(|s| s.1);
        match title {
            Some(n) if SETTINGS.with(|x| x.borrow().as_ref().is_none_or(|x| x.search.text().is_empty())) => {
                let l = gtk::Label::new(Some(n));
                l.add_css_class("group-pages");
                l.set_xalign(0.0);
                row.set_header(Some(&l));
            }
            _ => row.set_header(None::<&gtk::Widget>),
        }
    });
    let offset = gtk::ScrolledWindow::new();
    offset.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    offset.set_vexpand(true);
    offset.set_child(Some(&list));
    leva.append(&offset);

    // page content on the right
    let content = gtk::ScrolledWindow::new();
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    content.add_css_class("content");

    let main = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    main.append(&leva);
    main.append(&content);

    // a short toast at the bottom centre (“Connected to …”)
    let toast_text = gtk::Label::new(None);
    toast_text.add_css_class("toast");
    let toast = gtk::Revealer::new();
    toast.set_transition_type(gtk::RevealerTransitionType::SlideUp);
    toast.set_child(Some(&toast_text));
    toast.set_halign(gtk::Align::Center);
    toast.set_valign(gtk::Align::End);
    toast.set_margin_bottom(18);
    toast.set_can_target(false);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&main));
    overlay.add_overlay(&toast);
    window.set_child(Some(&overlay));

    let n = Rc::new(Settings {
        window: window.clone(),
        content,
        list: list.clone(),
        search: search.clone(),
        toast,
        toast_text,
        toast_timer: RefCell::new(None),
        active: RefCell::new(String::new()),
    });

    list.connect_row_selected(|_, row| {
        let Some(row) = row else { return };
        let id = row.widget_name().to_string();
        if let Some(n) = SETTINGS.with(|n| n.borrow().clone()) {
            if *n.active.borrow() != id {
                n.show(&id);
            }
        }
    });
    search.connect_search_changed(|h| {
        if let Some(n) = SETTINGS.with(|n| n.borrow().clone()) {
            n.filter(&h.text());
        }
    });
    // Enter: the first page found
    search.connect_activate(|_| {
        if let Some(n) = SETTINGS.with(|n| n.borrow().clone()) {
            let mut r = n.list.first_child();
            while let Some(w) = r {
                if w.is_visible() {
                    if let Some(row) = w.downcast_ref::<gtk::ListBoxRow>() {
                        n.list.select_row(Some(row));
                    }
                    break;
                }
                r = w.next_sibling();
            }
        }
    });
    // Ctrl+F search, Esc clears the search
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(|_, k, _, state| {
        let Some(n) = SETTINGS.with(|n| n.borrow().clone()) else { return glib::Propagation::Proceed };
        if k == gtk::gdk::Key::f && state.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            n.search.grab_focus();
            return glib::Propagation::Stop;
        }
        if k == gtk::gdk::Key::Escape && !n.search.text().is_empty() {
            n.search.set_text("");
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
    window.connect_close_request(|_| {
        // destroy the whole window, so the daemon doesn't keep the pages in memory
        if let Some(n) = SETTINGS.with(|n| n.borrow_mut().take()) {
            if let Some(id) = n.toast_timer.take() {
                id.remove();
            }
        }
        glib::idle_add_local_once(crate::common::return_memory);
        glib::Propagation::Proceed
    });

    SETTINGS.with(|x| x.replace(Some(n.clone())));
    n
}

/// The window is open (pages check this before showing data loaded in the background).
pub fn is_open() -> bool {
    SETTINGS.with(|n| n.borrow().is_some())
}

/// A short toast at the bottom of the window.
pub fn notify(text: &str) {
    let Some(n) = SETTINGS.with(|n| n.borrow().clone()) else { return };
    n.toast_text.set_text(text);
    n.toast.set_reveal_child(true);
    if let Some(id) = n.toast_timer.take() {
        id.remove();
    }
    let id = glib::timeout_add_local_once(Duration::from_millis(3200), || {
        if let Some(n) = SETTINGS.with(|n| n.borrow().clone()) {
            n.toast_timer.replace(None);
            n.toast.set_reveal_child(false);
        }
    });
    n.toast_timer.replace(Some(id));
}

/// Rebuilds the active page (after a change that takes effect a bit later).
pub fn refresh(page: &str) {
    if let Some(n) = SETTINGS.with(|n| n.borrow().clone()) {
        if *n.active.borrow() == page {
            n.show(page);
        }
    }
}

impl Settings {
    fn show(&self, id: &str) {
        self.active.replace(id.to_string());
        // select in the list (without rebuilding again through connect_row_selected)
        let mut r = self.list.first_child();
        while let Some(w) = r {
            if w.widget_name() == id {
                if let Some(row) = w.downcast_ref::<gtk::ListBoxRow>() {
                    if self.list.selected_row().as_ref() != Some(row) {
                        self.list.select_row(Some(row));
                    }
                }
            }
            r = w.next_sibling();
        }
        let page = match id {
            "bluetooth" => bluetooth::create(),
            "sound" => sound::create(),
            "display" => display::create(),
            "power" => power::create(),
            "appearance" => appearance::create(),
            "input" => input::create(),
            "panel_dock" => panel_dock::create(),
            "windows" => windows::create(),
            "shortcuts" => shortcuts::create(),
            "notifications" => notifications::create(),
            "apps" => apps::create(),
            "time" => time::create(),
            "network" => network::create(),
            "user" => user::create(),
            "storage" => storage::create(),
            "updates" => updates::create(),
            "about" => about::create(),
            _ => wifi::create(),
        };
        self.content.set_child(Some(&page));
        self.content.vadjustment().set_value(0.0);
    }

    fn filter(&self, text: &str) {
        let words: Vec<String> = text.split_whitespace().map(|s| crate::spotlight::without_diacritics(&s.to_lowercase())).collect();
        let mut r = self.list.first_child();
        while let Some(w) = r {
            let id = w.widget_name();
            let yes = PAGES.iter().find(|s| s.0 == id).is_some_and(|(_, name, _, key)| {
                let v = format!("{} {}", name.to_lowercase(), key);
                words.iter().all(|s| v.split_whitespace().any(|x| x.starts_with(s.as_str())))
            });
            w.set_visible(yes);
            r = w.next_sibling();
        }
        self.list.invalidate_headers();
    }
}

/// A subpage (e.g. network details) with a back arrow to the active page.
pub fn subpage(_back: &str, content: gtk::Widget) {
    let Some(n) = SETTINGS.with(|n| n.borrow().clone()) else { return };
    // the back arrow carries the name of the page the subpage came from (Wi-Fi, Network…)
    let active = n.active.borrow().clone();
    let back = PAGES.iter().find(|s| s.0 == active).map(|s| s.1).unwrap_or("Back");
    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let b = gtk::Button::with_label(&format!("\u{2039}  {back}"));
    b.add_css_class("back");
    b.set_halign(gtk::Align::Start);
    b.connect_clicked(|_| {
        if let Some(n) = SETTINGS.with(|n| n.borrow().clone()) {
            let id = n.active.borrow().clone();
            n.show(&id);
        }
    });
    wrapper.append(&b);
    wrapper.append(&content);
    n.content.set_child(Some(&wrapper));
    n.content.vadjustment().set_value(0.0);
}

/// The settings window (parent for dialogs).
pub fn window() -> Option<gtk::Window> {
    SETTINGS.with(|n| n.borrow().as_ref().map(|n| n.window.clone()))
}

/// Apps after login (Settings → Apps), called by the daemon at start.
pub fn start_po_login() {
    apps::start_po_login();
}

/// Rebuilds the page that is open right now (back to it from a subpage).
pub fn refresh_active() {
    if let Some(n) = SETTINGS.with(|n| n.borrow().clone()) {
        let id = n.active.borrow().clone();
        n.show(&id);
    }
}
