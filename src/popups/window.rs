//! Menu of the active window (click on the app name in the bar, Super + M, Super + right click):
//! move to a workspace (each shows what is on it), maximize, fullscreen,
//! tiling / floating, on all workspaces, close. It replaced a fuzzel script.

use std::collections::BTreeMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use swayipc::{Connection, Node, NodeType};

use crate::common::*;

fn app_id(n: &Node) -> String {
    n.app_id
        .clone()
        .or_else(|| n.window_properties.as_ref().and_then(|w| w.class.clone()))
        .unwrap_or_else(|| "?".into())
}

fn windows(n: &Node, v: &mut Vec<String>) {
    if n.pid.is_some() {
        v.push(app_id(n));
    }
    for d in n.nodes.iter().chain(n.floating_nodes.iter()) {
        windows(d, v);
    }
}

/// Workspaces (number → names of the apps on it) and the active window with its workspace.
fn detect() -> Option<(BTreeMap<String, Vec<String>>, Node, String, bool)> {
    let strom = Connection::new().ok()?.get_tree().ok()?;
    let mut workspaces = BTreeMap::new();
    let mut active = None;
    fn search<'a>(n: &'a Node) -> Option<&'a Node> {
        if n.focused && n.pid.is_some() {
            return Some(n);
        }
        n.nodes.iter().chain(n.floating_nodes.iter()).find_map(search)
    }
    for output in &strom.nodes {
        for ws in output.nodes.iter().filter(|w| w.node_type == NodeType::Workspace) {
            let name = ws.name.clone().unwrap_or_default();
            if name == "__i3_scratch" {
                continue;
            }
            let mut v = Vec::new();
            windows(ws, &mut v);
            let mut names: Vec<String> = v.iter().map(|a| crate::apps::info(a).name).collect();
            names.sort();
            names.dedup();
            if let Some(o) = search(ws) {
                // maximized = fills the whole workspace, title bar included
                let r = &o.rect;
                let d = o.deco_rect.height;
                let max = (r.x, r.y - d, r.width, r.height + d) == (ws.rect.x, ws.rect.y, ws.rect.width, ws.rect.height);
                active = Some((o.clone(), name.clone(), max));
            }
            workspaces.insert(name, names);
        }
    }
    let (window, workspace, max) = active?;
    Some((workspaces, window, workspace, max))
}

pub fn fill(o: &Rc<Popup>) {
    let Some((workspaces, window, workspace, maximized)) = detect() else {
        o.state("No window is active");
        return;
    };
    // keyboard works too (Super + M): arrows, Enter, Esc
    o.window.set_keyboard_mode(KeyboardMode::Exclusive);
    let id = window.id;
    let floating = window.node_type == NodeType::FloatingCon;
    let mut caption = window.name.clone().unwrap_or_default();
    if caption.chars().count() > 45 {
        caption = caption.chars().take(44).collect::<String>() + "…";
    }
    o.panel.append(&title(&crate::apps::info(&app_id(&window)).name, &caption));

    let slaby = Rc::downgrade(o);
    let action = move |command: String| {
        let slaby = slaby.clone();
        move || {
            if let Some(o) = slaby.upgrade() {
                o.close();
            }
            crate::sway::command(&format!("[con_id={id}] {command}"));
        }
    };

    // workspaces 1–4 always, others only when they exist; sort numerically
    let mut digits: Vec<i64> = (1..=4).collect();
    digits.extend(workspaces.keys().filter_map(|k| k.parse::<i64>().ok()).filter(|n| *n > 4));
    digits.sort();
    digits.dedup();
    let mut first: Option<gtk::Button> = None;
    for n in &digits {
        let name = n.to_string();
        let content = workspaces.get(&name).map(|a| a.join(", ")).unwrap_or_default();
        let b = if name == workspace {
            let b = row("\u{f111}", &format!("Workspace {n}"), "current", true, || {});
            b.set_sensitive(false);
            b
        } else {
            let b = row("\u{f061}", &format!("Move to workspace {n}"), &content, false, action(format!("move container to workspace number {n}")));
            first.get_or_insert(b.clone());
            b
        };
        o.panel.append(&b);
    }
    let free = digits.last().copied().unwrap_or(4).max(4) + 1;
    o.panel.append(&row("\u{f067}", &format!("Move to a new workspace ({free})"), "", false, action(format!("move container to workspace number {free}"))));
    o.panel.append(&separator());

    o.panel.append(&row("\u{f2d1}", "Minimize", "", false, action("minimize".into())));
    if floating {
        let (char, text, shortcut) = if maximized { ("\u{f422}", "Restore size", "Super + ↓") } else { ("\u{f424}", "Maximize", "Super + ↑") };
        o.panel.append(&row(char, text, shortcut, false, action("maximize toggle".into())));
    }
    o.panel.append(&row("\u{f065}", "Fullscreen", "Super + F", false, action("fullscreen toggle".into())));
    if floating {
        o.panel.append(&row("\u{f009}", "Tile", "", false, action("floating disable".into())));
        if window.sticky {
            o.panel.append(&row("\u{f08d}", "Only on this workspace", "", false, action("sticky disable".into())));
        } else {
            o.panel.append(&row("\u{f08d}", "Show on all workspaces", "", false, action("sticky enable".into())));
        }
    } else {
        o.panel.append(&row("\u{f24d}", "Float", "", false, action("floating enable".into())));
    }
    o.panel.append(&row("\u{f00d}", "Close", "", false, action("kill".into())));
    if let Some(b) = first {
        b.grab_focus();
    }
}
