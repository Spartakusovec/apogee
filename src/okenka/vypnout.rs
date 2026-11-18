//! Vypnutí (tlačítko ⏻ v panelu, Super+Shift+E): nastavení, zamknout, odhlásit, uspat, restartovat, vypnout.
//! Dřív vypnout.sh přes fuzzel.

use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};

use crate::spolecne::*;

pub fn naplnit(o: &Rc<Okenko>) {
    // ovládání i klávesnicí: šipky, Enter, Esc
    o.okno.set_keyboard_mode(KeyboardMode::Exclusive);
    let mut prvni: Option<gtk::Button> = None;
    let polozky: [(&str, &str, fn()); 6] = [
        ("\u{f013}", "Settings", || { gtk::glib::idle_add_local_once(|| crate::nastaveni::otevrit("")); }),
        ("\u{f023}", "Lock", || pozadi("gtklock", &["-d"])),
        ("\u{f2f5}", "Log out", || crate::sway::prikaz("exit")),
        ("\u{f186}", "Suspend", || pozadi("systemctl", &["suspend"])),
        ("\u{f2f9}", "Restart", || pozadi("systemctl", &["reboot"])),
        ("\u{f011}", "Shut down", || pozadi("systemctl", &["poweroff"])),
    ];
    for (znak, text, akce) in polozky {
        let slaby = Rc::downgrade(o);
        let b = radek(znak, text, "", false, move || {
            if let Some(o) = slaby.upgrade() {
                o.zavrit();
            }
            akce();
        });
        prvni.get_or_insert(b.clone());
        o.panel.append(&b);
    }
    if let Some(b) = prvni {
        b.grab_focus();
    }
}
