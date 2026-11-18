//! Stránka Panel & Dock: které moduly panel ukazuje, formát hodin, kdy se dok schovává
//! a pořadí připnutých aplikací v doku.

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use serde_json::json;

use super::ui::*;
use crate::spolecne::{nastaveni, ulozit_nastaveni};

/// moduly, které jde schovat: (id v panelu, název, popis)
const MODULY: &[(&str, &str, &str)] = &[
    ("dlazdice", "Tiling button", "Switches the workspace between floating and tiled windows"),
    ("pomodoro", "Pomodoro timer", "Only while a timer runs"),
    ("hudba", "Music", "What is playing, with previous / next"),
    ("pocasi", "Weather", ""),
    ("zvuk", "Volume", ""),
    ("jas", "Brightness", ""),
    ("sit", "Wi-Fi", ""),
    ("baterie", "Battery", ""),
    ("aktualizace", "Updates", "Only when updates are waiting"),
    ("notifikace", "Notifications", ""),
];

fn skryte() -> Vec<String> {
    nastaveni("panel_skryte").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}

fn hodiny_prepinac(karta: &gtk::ListBox, klic: &'static str, vychozi: bool, nazev: &str, popis: &str) {
    let zapnuto = nastaveni("hodiny")[klic].as_bool().unwrap_or(vychozi);
    radek_prepinac(karta, nazev, popis, zapnuto, move |z| {
        let mut h = nastaveni("hodiny");
        if !h.is_object() {
            h = json!({});
        }
        h[klic] = json!(z);
        ulozit_nastaveni("hodiny", h);
        crate::lista::zdroje::zmenit_format();
    });
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Panel & Dock");

    let karta = skupina(&s, "Clock", "");
    hodiny_prepinac(&karta, "datum", true, "Show the date", "");
    hodiny_prepinac(&karta, "sekundy", true, "Show seconds", "");
    hodiny_prepinac(&karta, "h12", false, "12-hour time", "3:45 PM instead of 15:45");

    let karta = skupina(&s, "Panel", "The app name, workspaces, clock and power button are always there.");
    let schovane = skryte();
    for (id, nazev, popis) in MODULY {
        radek_prepinac(&karta, nazev, popis, !schovane.iter().any(|x| x == id), move |z| {
            let mut v = skryte();
            v.retain(|x| x != id);
            if !z {
                v.push(id.to_string());
            }
            ulozit_nastaveni("panel_skryte", json!(v));
            crate::lista::obnovit_moduly();
        });
    }

    let karta = skupina(&s, "Dock", "");
    let rezimy = [
        ("chytre", "When a window is in the way"),
        ("nikdy", "Never – always visible"),
        ("vzdy", "Always – show with Super + X or the bottom edge"),
    ];
    let ted = crate::dok::skryvani();
    let d = gtk::DropDown::from_strings(&rezimy.iter().map(|r| r.1).collect::<Vec<_>>());
    d.set_selected(rezimy.iter().position(|r| r.0 == ted).unwrap_or(0) as u32);
    d.connect_selected_notify(move |d| {
        crate::dok::nastavit_skryvani(rezimy[d.selected() as usize].0);
        super::oznamit("Saved");
    });
    radek(&karta, "\u{f150}", "Hide the dock", "", Some(d.upcast_ref()));

    let karta = skupina(&s, "Pinned apps", "Pin more apps by right-clicking them in the dock or in the app grid.");
    let pripnute = crate::dok::pripnute();
    if pripnute.is_empty() {
        radek(&karta, "", "No pinned apps", "", None);
    }
    for (i, klic) in pripnute.iter().enumerate() {
        let info = crate::dok::info_aplikace(klic);
        let nazev = info.as_ref().map(|a| a.display_name().to_string()).unwrap_or_else(|| klic.clone());
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        r.add_css_class("radek");
        let ikona = gtk::Image::new();
        ikona.set_pixel_size(28);
        if let Some(i) = info.as_ref().and_then(|a| a.icon()) {
            ikona.set_from_gicon(&i);
        }
        r.append(&ikona);
        let l = gtk::Label::new(Some(&nazev));
        l.set_hexpand(true);
        l.set_xalign(0.0);
        r.append(&l);
        let posun = |smer: i32| {
            let seznam = pripnute.clone();
            move || {
                let mut s = seznam.clone();
                let j = i as i32 + smer;
                if j >= 0 && (j as usize) < s.len() {
                    s.swap(i, j as usize);
                    crate::dok::nastavit_pripnute(&s);
                    glib::timeout_add_local_once(Duration::from_millis(100), || super::obnovit("panel_dok"));
                }
            }
        };
        let nahoru = tlacitko("\u{2191}", false, posun(-1));
        nahoru.set_sensitive(i > 0);
        nahoru.set_tooltip_text(Some("Move left"));
        let dolu = tlacitko("\u{2193}", false, posun(1));
        dolu.set_sensitive(i + 1 < pripnute.len());
        dolu.set_tooltip_text(Some("Move right"));
        let seznam = pripnute.clone();
        let odepnout = tlacitko("Unpin", false, move || {
            let mut s = seznam.clone();
            s.remove(i);
            crate::dok::nastavit_pripnute(&s);
            glib::timeout_add_local_once(Duration::from_millis(100), || super::obnovit("panel_dok"));
        });
        r.append(&nahoru);
        r.append(&dolu);
        r.append(&odepnout);
        let row = gtk::ListBoxRow::new();
        row.set_activatable(false);
        row.set_child(Some(&r));
        karta.append(&row);
    }
    s.upcast()
}
