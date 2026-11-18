//! Power menu (⏻ in the bar, Super+Shift+E): settings, lock, log out, suspend, restart, power off.
//! It replaced a fuzzel script.

use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};

use crate::common::*;

pub fn fill(o: &Rc<Popup>) {
    // keyboard works too: arrows, Enter, Esc
    o.window.set_keyboard_mode(KeyboardMode::Exclusive);
    let mut first: Option<gtk::Button> = None;
    let items: [(&str, &str, fn()); 6] = [
        ("\u{f013}", "Settings", || { gtk::glib::idle_add_local_once(|| crate::settings::open("")); }),
        ("\u{f023}", "Lock", || background("gtklock", &["-d"])),
        ("\u{f2f5}", "Log out", || crate::sway::command("exit")),
        ("\u{f186}", "Suspend", || background("systemctl", &["suspend"])),
        ("\u{f2f9}", "Restart", || background("systemctl", &["reboot"])),
        ("\u{f011}", "Shut down", || background("systemctl", &["poweroff"])),
    ];
    for (char, text, action) in items {
        let slaby = Rc::downgrade(o);
        let b = row(char, text, "", false, move || {
            if let Some(o) = slaby.upgrade() {
                o.close();
            }
            action();
        });
        first.get_or_insert(b.clone());
        o.panel.append(&b);
    }
    if let Some(b) = first {
        b.grab_focus();
    }
}
