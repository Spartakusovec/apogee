//! Alt+Tab window switcher: list of windows (icon, name · title · workspace) by last use.
//! Hold Alt, Tab selects, releasing Alt switches. A quick tap switches straight to the previous window.
//! Esc closes, Enter or a mouse click switches.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, Layer, LayerShell};

use crate::sway;

const ROWS: usize = 10;

fn debug(message: &str) {
    if std::env::var_os("APOGEE_DEBUG").is_some() {
        eprintln!("[prepinac {:?}] {message}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() % 100000).unwrap_or(0));
    }
}
const ICON: i32 = 38;

struct Switcher {
    window: gtk::Window,
    list: gtk::Box,
    windows: Vec<sway::Window>,
    selected: Cell<usize>,
    verified: Cell<bool>,
    done: Cell<bool>,
}

thread_local! {
    static OPEN: RefCell<Option<Rc<Switcher>>> = const { RefCell::new(None) };
}

pub fn alttab(direction: i32) {
    if let Some(p) = OPEN.with(|o| o.borrow().clone()) {
        p.offset(direction);
        return;
    }
    let windows = sway::windows();
    if windows.len() < 2 {
        return;
    }
    crate::popups::close();
    crate::style::apply();
    let selected = if direction > 0 { 1 } else { windows.len() - 1 };

    let window = gtk::Window::new();
    window.add_css_class("apogee");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    // the keyboard belongs to the switcher for a moment, otherwise it wouldn't notice Alt being released
    window.set_keyboard_mode(KeyboardMode::Exclusive);
    window.set_namespace(Some("switcher-windows"));
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_widget_name("switcher");
    panel.set_size_request(1025, -1);
    let title = gtk::Label::new(Some("Windows:"));
    title.set_xalign(0.0);
    title.add_css_class("title-switcher");
    panel.append(&title);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.append(&list);
    window.set_child(Some(&panel));
    // Until it's known whether Alt is still down, show almost nothing (a quick tap must not flicker).
    // Not exactly 0: GTK 4 doesn't draw a fully transparent window, so Sway wouldn't show it or give it the keyboard.
    window.set_opacity(0.01);

    let p = Rc::new(Switcher {
        window: window.clone(),
        list,
        windows,
        selected: Cell::new(selected),
        verified: Cell::new(false),
        done: Cell::new(false),
    });

    let keys = gtk::EventControllerKey::new();
    let p2 = p.clone();
    keys.connect_key_pressed(move |_, k, _, _| {
        debug(&format!("key pressed: {k:?}"));
        use gdk::Key;
        match k {
            Key::Tab | Key::Down | Key::Right => p2.offset(1),
            Key::ISO_Left_Tab | Key::Up | Key::Left => p2.offset(-1),
            Key::Return | Key::KP_Enter | Key::space => p2.confirm(),
            Key::Escape => p2.close(),
            _ => {}
        }
        glib::Propagation::Stop
    });
    keys.connect_modifiers(|_, state| {
        debug(&format!("signal modifiers: {state:?}"));
        glib::Propagation::Proceed
    });
    let p2 = p.clone();
    keys.connect_key_released(move |_, k, _, _| {
        debug(&format!("key released: {k:?}"));
        if matches!(k, gdk::Key::Alt_L | gdk::Key::Alt_R | gdk::Key::Meta_L | gdk::Key::Meta_R) {
            p2.confirm();
        }
    });
    window.add_controller(keys);

    // after taking the keyboard: Alt already released = a quick tap, otherwise show the list
    // (is_active doesn't change for windows anchored to the screen edge, hence EventControllerFocus)
    let focus = gtk::EventControllerFocus::new();
    let p2 = p.clone();
    focus.connect_enter(move |_| {
        debug("keyboard focus entered");
        let p3 = p2.clone();
        glib::timeout_add_local_once(Duration::from_millis(60), move || p3.holds_alt());
    });
    window.add_controller(focus);
    let p2 = p.clone();
    glib::timeout_add_local_once(Duration::from_millis(400), move || p2.holds_alt());

    p.draw();
    OPEN.with(|o| o.replace(Some(p.clone())));
    window.present();
}

impl Switcher {
    fn holds_alt(self: &Rc<Self>) {
        if self.verified.replace(true) || self.done.get() {
            return;
        }
        let state = gdk::Display::default()
            .and_then(|d| d.default_seat())
            .and_then(|s| s.keyboard())
            .map(|k| k.modifier_state());
        debug(&format!("drzi_alt: modifier_state = {state:?}"));
        let alt = gdk::Display::default()
            .and_then(|d| d.default_seat())
            .and_then(|s| s.keyboard())
            .map(|k| k.modifier_state().contains(gdk::ModifierType::ALT_MASK))
            .unwrap_or(false);
        if alt {
            self.window.set_opacity(1.0);
        } else {
            self.confirm();
        }
    }

    fn draw(self: &Rc<Self>) {
        while let Some(d) = self.list.first_child() {
            self.list.remove(&d);
        }
        let selected = self.selected.get();
        let start = selected.saturating_sub(ROWS / 2).min(self.windows.len().saturating_sub(ROWS));
        for i in start..(start + ROWS).min(self.windows.len()) {
            let o = &self.windows[i];
            let info = crate::apps::info(&o.app);
            let mut title = if o.title == info.name { String::new() } else { o.title.clone() };
            if title.chars().count() > 55 {
                title = title.chars().take(52).collect::<String>() + "…";
            }
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 18);
            row.add_css_class("row-windows");
            row.set_size_request(-1, 52); // row height like in the launcher
            if i == selected {
                row.add_css_class("selected");
            }
            let icon = if info.icon.starts_with('/') {
                gtk::Image::from_file(&info.icon)
            } else {
                gtk::Image::from_icon_name(&info.icon)
            };
            icon.set_pixel_size(ICON);
            row.append(&icon);
            // like the original list in the launcher: Name · title · workspace
            let mut parts = vec![info.name.clone()];
            if !title.is_empty() {
                parts.push(title);
            }
            parts.push(o.workspace.clone());
            let text = gtk::Label::new(Some(&parts.join("  ·  ")));
            text.set_xalign(0.0);
            text.set_hexpand(true);
            text.set_ellipsize(gtk::pango::EllipsizeMode::End);
            row.append(&text);
            let click = gtk::GestureClick::new();
            let p = self.clone();
            click.connect_released(move |_, _, _, _| {
                p.selected.set(i);
                p.confirm();
            });
            row.add_controller(click);
            self.list.append(&row);
        }
    }

    fn offset(self: &Rc<Self>, step: i32) {
        let n = self.windows.len() as i32;
        self.selected.set(((self.selected.get() as i32 + step).rem_euclid(n)) as usize);
        self.draw();
    }

    fn confirm(self: &Rc<Self>) {
        if self.done.replace(true) {
            return;
        }
        self.window.set_visible(false);
        sway::command(&format!("[con_id={}] focus", self.windows[self.selected.get()].id));
        self.end();
    }

    /// Esc: close and give the focus back to the window you were in (Sway doesn't do that by itself).
    fn close(self: &Rc<Self>) {
        if self.done.replace(true) {
            return;
        }
        self.window.set_visible(false);
        if let Some(original) = self.windows.iter().find(|o| o.active).or(self.windows.first()) {
            sway::command(&format!("[con_id={}] focus", original.id));
        }
        self.end();
    }

    fn end(&self) {
        OPEN.with(|o| o.replace(None));
        // break the reference cycles (rows and controllers hold an Rc to the switcher), otherwise it stays in memory
        while let Some(d) = self.list.first_child() {
            self.list.remove(&d);
        }
        crate::common::remove_controls(&self.window);
        self.window.destroy();
        crate::common::return_memory();
    }
}
