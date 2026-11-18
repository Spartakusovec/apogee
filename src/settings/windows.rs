//! Windows & Workspaces page: floating windows or tiling as the default, gaps between windows,
//! focus follows the mouse, an overview of snapping and workspaces.

use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use super::sway_conf;
use super::ui::*;
use crate::common::command;

/// A value from Sway's config (get_config shows the main config, not settings.conf; that one wins).
fn from_sway(key: &str, default: &str) -> String {
    if let Some(v) = sway_conf::value(key) {
        return v;
    }
    let config = std::fs::read_to_string(crate::common::home().join(".config/sway/config")).unwrap_or_default();
    config
        .lines()
        .find_map(|r| r.trim().strip_prefix(key).and_then(|z| z.strip_prefix(' ')).map(|z| z.trim().to_string()))
        .unwrap_or_else(|| default.to_string())
}

pub fn create() -> gtk::Widget {
    let s = page("Windows & Workspaces");

    let card = group(&s, "New workspaces", "Each workspace can still be switched with the button left of the workspaces in the panel.");
    let tiling = crate::sway::default_tiling();
    for (value, icon, name, description) in [
        (false, "\u{f2d2}", "Floating windows", "Windows open where you want them and can overlap, like on Windows or macOS"),
        (true, "\u{f009}", "Tiled", "Windows share the screen side by side and never overlap"),
    ] {
        action_row(&card, if value == tiling { "\u{f00c}" } else { icon }, name, description, "", move || {
            crate::sway::change_default_tiling(value);
            super::notify(if value { "Workspaces are tiled" } else { "Windows float" });
            glib::timeout_add_local_once(Duration::from_millis(200), || super::refresh("windows"));
        });
    }

    let card = group(&s, "Windows", "");
    let gap: f64 = from_sway("gaps inner", "8").parse().unwrap_or(8.0);
    let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 24.0, 1.0);
    slider.set_value(gap);
    slider.set_size_request(220, -1);
    slider.set_draw_value(false);
    slider.connect_value_changed(|p| {
        let v = p.value().round();
        sway_conf::save("gaps inner", &format!("{v}"));
        crate::sway::command(&format!("gaps inner all set {v}"));
    });
    add_row(&card, "\u{f0db}", "Gaps between tiled windows", "", Some(slider.upcast_ref()));
    switch_row(&card, "No gaps with a single window", "One tiled window fills the whole screen", from_sway("smart_gaps", "on") == "on", |z| {
        sway_conf::set("smart_gaps", if z { "on" } else { "off" });
    });
    switch_row(&card, "Focus follows the mouse", "The window under the pointer becomes active without clicking", from_sway("focus_follows_mouse", "no") == "yes", |z| {
        sway_conf::set("focus_follows_mouse", if z { "yes" } else { "no" });
    });

    let card = group(&s, "Moving and snapping", "");
    for (co, how) in [
        ("Snap to half of the screen", "Super + ← / →, or drag a window to the left or right edge"),
        ("Maximize", "Super + ↑, double-click the title bar, or drag to the top edge"),
        ("Move or resize", "Hold Super and drag with the left button anywhere in the window; drag an edge to resize"),
        ("Window menu", "Right-click the title bar, or Super + M"),
    ] {
        add_row(&card, "\u{f0b2}", co, how, None);
    }

    let card = group(&s, "Workspaces", "");
    let count = command("swaymsg", &["-t", "get_workspaces", "-r"]).matches("\"num\"").count();
    for (co, how) in [
        ("Switch workspace", "Super + 1–9, four fingers left / right, or scroll over the workspaces in the panel"),
        ("Move a window to a workspace", "Super + Shift + 1–9, or the window menu"),
    ] {
        add_row(&card, "\u{f24d}", co, how, None);
    }
    add_row(&card, "", "Workspaces in use right now", "", Some(gtk::Label::new(Some(&count.to_string())).upcast_ref()));
    s.upcast()
}
