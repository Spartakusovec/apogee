//! Stránka Windows & Workspaces: plovoucí okna nebo dlaždice jako výchozí, mezery mezi okny,
//! zaměření oken myší, přehled přichytávání a ploch.

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use super::sway_conf;
use super::ui::*;
use crate::spolecne::prikaz;

/// Hodnota ze Swaye (get_config ukazuje hlavní config, ne nastaveni.conf; ten má přednost).
fn ze_swaye(klic: &str, vychozi: &str) -> String {
    if let Some(v) = sway_conf::hodnota(klic) {
        return v;
    }
    let konfigurace = std::fs::read_to_string(crate::spolecne::domov().join(".config/sway/config")).unwrap_or_default();
    konfigurace
        .lines()
        .find_map(|r| r.trim().strip_prefix(klic).and_then(|z| z.strip_prefix(' ')).map(|z| z.trim().to_string()))
        .unwrap_or_else(|| vychozi.to_string())
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Windows & Workspaces");

    let karta = skupina(&s, "New workspaces", "Each workspace can still be switched with the button left of the workspaces in the panel.");
    let dlazdice = crate::sway::vychozi_dlazdice();
    for (hodnota, ikona, nazev, popis) in [
        (false, "\u{f2d2}", "Floating windows", "Windows open where you want them and can overlap, like on Windows or macOS"),
        (true, "\u{f009}", "Tiled", "Windows share the screen side by side and never overlap"),
    ] {
        radek_akce(&karta, if hodnota == dlazdice { "\u{f00c}" } else { ikona }, nazev, popis, "", move || {
            crate::sway::zmenit_vychozi_dlazdice(hodnota);
            super::oznamit(if hodnota { "Workspaces are tiled" } else { "Windows float" });
            glib::timeout_add_local_once(Duration::from_millis(200), || super::obnovit("okna"));
        });
    }

    let karta = skupina(&s, "Windows", "");
    let mezera: f64 = ze_swaye("gaps inner", "8").parse().unwrap_or(8.0);
    let posuvnik = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 24.0, 1.0);
    posuvnik.set_value(mezera);
    posuvnik.set_size_request(220, -1);
    posuvnik.set_draw_value(false);
    posuvnik.connect_value_changed(|p| {
        let v = p.value().round();
        sway_conf::ulozit("gaps inner", &format!("{v}"));
        crate::sway::prikaz(&format!("gaps inner all set {v}"));
    });
    radek(&karta, "\u{f0db}", "Gaps between tiled windows", "", Some(posuvnik.upcast_ref()));
    radek_prepinac(&karta, "No gaps with a single window", "One tiled window fills the whole screen", ze_swaye("smart_gaps", "on") == "on", |z| {
        sway_conf::nastavit("smart_gaps", if z { "on" } else { "off" });
    });
    radek_prepinac(&karta, "Focus follows the mouse", "The window under the pointer becomes active without clicking", ze_swaye("focus_follows_mouse", "no") == "yes", |z| {
        sway_conf::nastavit("focus_follows_mouse", if z { "yes" } else { "no" });
    });

    let karta = skupina(&s, "Moving and snapping", "");
    for (co, jak) in [
        ("Snap to half of the screen", "Super + ← / →, or drag a window to the left or right edge"),
        ("Maximize", "Super + ↑, double-click the title bar, or drag to the top edge"),
        ("Move or resize", "Hold Super and drag with the left button anywhere in the window; drag an edge to resize"),
        ("Window menu", "Right-click the title bar, or Super + M"),
    ] {
        radek(&karta, "\u{f0b2}", co, jak, None);
    }

    let karta = skupina(&s, "Workspaces", "");
    let pocet = prikaz("swaymsg", &["-t", "get_workspaces", "-r"]).matches("\"num\"").count();
    for (co, jak) in [
        ("Switch workspace", "Super + 1–9, four fingers left / right, or scroll over the workspaces in the panel"),
        ("Move a window to a workspace", "Super + Shift + 1–9, or the window menu"),
    ] {
        radek(&karta, "\u{f24d}", co, jak, None);
    }
    radek(&karta, "", "Workspaces in use right now", "", Some(gtk::Label::new(Some(&pocet.to_string())).upcast_ref()));
    s.upcast()
}
