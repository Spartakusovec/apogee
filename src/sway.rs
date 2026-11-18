//! Talking to Sway: order of the most recently used windows (for Alt+Tab), window list
//! and bar modules that depend on Sway (name of the active app, tiling state).

use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;
use swayipc::{Connection, Event, EventType, Node, NodeType, WindowChange};

use crate::panel::{escape, publish};

/// window IDs, most recently used first
static ORDER: Mutex<Vec<i64>> = Mutex::new(Vec::new());

/// Apps whose new windows stay floating even on a tiled workspace (dialogs, small windows)
const ALWAYS_FLOATING: [&str; 5] = ["pavucontrol", "blueman", "nm-connection-editor", "Calculator", "lxpolkit"];

/// Watches windows and workspaces in the background: window order, bar modules and new windows on
/// tiled workspaces (replaced several shell scripts).
pub fn watch() {
    std::thread::spawn(|| loop {
        let events = Connection::new().and_then(|c| c.subscribe([EventType::Window, EventType::Workspace, EventType::Tick, EventType::Mode, EventType::Input]));
        if let (Ok(events), Ok(mut sway)) = (events, Connection::new()) {
            apps_to_panel(&mut sway);
            tiling_to_panel(&mut sway);
            workspaces_to_panel(&mut sway);
            layouts_to_panel(&mut sway);
            publish("mode", json!({"text": ""}));
            for event in events.flatten() {
                match event {
                    Event::Window(w) => {
                        match w.change {
                            WindowChange::Focus => {
                                let mut order = ORDER.lock().unwrap();
                                order.retain(|&id| id != w.container.id);
                                order.insert(0, w.container.id);
                            }
                            WindowChange::Close => ORDER.lock().unwrap().retain(|&id| id != w.container.id),
                            WindowChange::New => new_window(&mut sway, &w.container),
                            // titles change all the time, the app name doesn't
                            WindowChange::Title | WindowChange::Mark | WindowChange::Urgent => continue,
                            _ => {}
                        }
                        apps_to_panel(&mut sway);
                        // app icons next to the workspaces (focus only switches the active workspace, which has its own event)
                        if matches!(w.change, WindowChange::New | WindowChange::Close | WindowChange::Move) {
                            workspaces_to_panel(&mut sway);
                        }
                        crate::dock::notify();
                    }
                    // keyboard layout switched (or a new one from Settings)
                    Event::Input(i) if matches!(i.change, swayipc::InputChange::XkbLayout | swayipc::InputChange::XkbKeymap) => layouts_to_panel(&mut sway),
                    Event::Mode(m) => {
                        // Sway mode (resize): visible only when it isn't the default
                        let text = if m.change == "default" { String::new() } else { escape(&m.change) };
                        publish("mode", json!({"text": text}));
                    }
                    Event::Workspace(_) => {
                        crate::dock::notify();
                        workspaces_to_panel(&mut sway);
                        apps_to_panel(&mut sway);
                        tiling_to_panel(&mut sway);
                    }
                    // the tiling button switched the workspace mode (toggle_tiling)
                    Event::Tick(t) if t.payload == "tiling" => tiling_to_panel(&mut sway),
                    // right click on an app's title bar (GTK): the patched Sway sends “window-menu”
                    // a window is being dragged (patched Sway, at most 10× a second): the dock checks whether it's in the way
                    Event::Tick(t) if t.payload == "window-drag" => crate::dock::notify(),
                    Event::Tick(t) if t.payload == "window-menu" => {
                        gtk::glib::MainContext::default().invoke(|| crate::popups::toggle("window"));
                    }
                    _ => {}
                }
            }
        }
        // Sway restarted or the connection dropped: try again
        std::thread::sleep(Duration::from_secs(1));
    });
}

/// Keyboard layout in the bar (only with more than one): the active one's code, e.g. “CZ”, “US”.
fn layouts_to_panel(sway: &mut Connection) {
    let Ok(inputs) = sway.get_inputs() else { return };
    let Some(k) = inputs.iter().find(|i| i.input_type == "keyboard" && i.xkb_layout_names.len() > 1) else {
        publish("layouts", json!({"text": ""}));
        return;
    };
    let i = k.xkb_active_layout_index.unwrap_or(0).max(0) as usize;
    let codes = crate::common::settings("layouts");
    let code = codes[i][0].as_str().map(|s| s.to_uppercase()).unwrap_or_else(|| k.xkb_layout_names.get(i).map(|n| n.chars().take(2).collect::<String>().to_uppercase()).unwrap_or_default());
    let name = k.xkb_layout_names.get(i).cloned().unwrap_or_default();
    publish("layouts", json!({"text": format!("\u{f11c}  {code}"), "tooltip": escape(&name)}));
}

fn app_id(node: &Node) -> String {
    node.app_id
        .clone()
        .or_else(|| node.window_properties.as_ref().and_then(|w| w.class.clone()))
        .unwrap_or_default()
}

fn find<'a>(node: &'a Node, condition: &dyn Fn(&Node) -> bool) -> Option<&'a Node> {
    if condition(node) {
        return Some(node);
    }
    node.nodes.iter().chain(node.floating_nodes.iter()).find_map(|d| find(d, condition))
}

/// Name of the active app at the left of the bar, like on a Mac (click = window menu).
/// Hidden on an empty workspace.
fn apps_to_panel(sway: &mut Connection) {
    let Ok(strom) = sway.get_tree() else { return };
    match find(&strom, &|n| n.focused && n.pid.is_some()) {
        Some(window) => {
            let name = crate::apps::info(&app_id(window)).name;
            publish(
                "apps",
                json!({
                    "text": format!("{}  <span size=\"smaller\" alpha=\"55%\">\u{f078}</span>", escape(&name)),
                }),
            );
        }
        None => publish("apps", json!({"text": ""})),
    }
}

/// App icon next to the workspace number in the bar (Font Awesome)
fn icon_on_workspace(app: &str) -> &'static str {
    let a = app.to_lowercase();
    let is = |words: &[&str]| words.iter().any(|s| a.contains(s));
    if is(&["firefox"]) {
        "\u{f269}"
    } else if is(&["brave", "chrom"]) {
        "\u{f268}"
    } else if is(&["discord", "vesktop"]) {
        "\u{f392}"
    } else if is(&["spotify"]) {
        "\u{f1bc}"
    } else if is(&["telegram"]) {
        "\u{f2c6}"
    } else if is(&["stremio", "vlc", "player", "mpv", "celluloid"]) {
        "\u{f008}"
    } else if is(&["ptyxis", "terminal", "kitty", "alacritty", "foot"]) {
        "\u{f120}"
    } else if is(&["nautilus", "cosmic-files", "files"]) {
        "\u{f07b}"
    } else if is(&["obsidian"]) {
        "\u{f249}"
    } else if is(&["code", "zed"]) {
        "\u{f121}"
    } else if is(&["onlyoffice", "papers", "inkscape", "gimp", "libreoffice"]) {
        "\u{f15b}"
    } else if is(&["minecraft"]) {
        "\u{f11b}"
    } else if is(&["anki"]) {
        "\u{f02d}"
    } else {
        "\u{f2d0}"
    }
}

/// Workspaces as in the bar: 1–4 always, others only when they exist, sorted by number.
fn names_ploch(workspaces: &[swayipc::Workspace]) -> Vec<String> {
    let mut names: Vec<String> = (1..=4).map(|n| n.to_string()).collect();
    for p in workspaces {
        if !names.contains(&p.name) {
            names.push(p.name.clone());
        }
    }
    let order = |j: &str| j.parse::<i64>().unwrap_or(i64::MAX);
    names.sort_by_key(|j| (order(j), j.clone()));
    names
}

/// Scrolling over the workspaces: the neighbouring workspace in the bar (empty 1–4 too), no wrapping around.
/// (Sway's `workspace next` only knows existing workspaces, with a single one open it would do nothing.)
pub fn workspace_next_to(next: bool) {
    let Ok(mut sway) = Connection::new() else { return };
    let Ok(workspaces) = sway.get_workspaces() else { return };
    let Some(now) = workspaces.iter().find(|p| p.focused) else { return };
    let names = names_ploch(&workspaces);
    let Some(i) = names.iter().position(|j| *j == now.name) else { return };
    let target = if next { names.get(i + 1) } else { i.checked_sub(1).and_then(|i| names.get(i)) };
    if let Some(target) = target {
        let _ = sway.run_command(format!("workspace \"{}\"", target.replace('"', "\\\"")));
    }
}

/// Workspaces in the bar: 1–4 always, others only when they exist; each with the icons of its windows.
fn workspaces_to_panel(sway: &mut Connection) {
    let (Ok(strom), Ok(workspaces)) = (sway.get_tree(), sway.get_workspaces()) else { return };
    let mut nodes: Vec<&Node> = Vec::new();
    fn find<'a>(n: &'a Node, v: &mut Vec<&'a Node>) {
        if n.node_type == NodeType::Workspace {
            v.push(n);
            return;
        }
        for d in &n.nodes {
            find(d, v);
        }
    }
    find(&strom, &mut nodes);
    let mut list: Vec<serde_json::Value> = Vec::new();
    for name in names_ploch(&workspaces) {
        let p = workspaces.iter().find(|p| p.name == name);
        let mut icons: Vec<&str> = Vec::new();
        if let Some(node) = nodes.iter().find(|u| u.name.as_deref() == Some(name.as_str())) {
            fn windows<'a>(n: &'a Node, v: &mut Vec<String>) {
                if n.pid.is_some() {
                    v.push(n.app_id.clone().or_else(|| n.window_properties.as_ref().and_then(|w| w.class.clone())).unwrap_or_default());
                }
                for d in n.nodes.iter().chain(n.floating_nodes.iter()) {
                    windows(d, v);
                }
            }
            let mut v = Vec::new();
            windows(node, &mut v);
            icons = v.iter().map(|a| icon_on_workspace(a)).collect();
        }
        list.push(json!({
            "name": name,
            "icons": icons.join(" "),
            "active": p.is_some_and(|p| p.focused),
            "visible": p.is_some_and(|p| p.visible),
            "urgent": p.is_some_and(|p| p.urgent),
            "empty": icons.is_empty(),
        }));
    }
    publish("workspaces", json!({"workspaces": list}));
}

/// New workspaces tiled (Settings → Windows & Workspaces), otherwise floating windows.
pub fn default_tiling() -> bool {
    crate::common::settings("tiling_default").as_bool().unwrap_or(false)
}

/// The workspace is tiled: the default, unless the user switched it with the bar button
/// (the exception is a marker in run_dir: sway-tiling-N, or sway-floating-N when tiling is the default).
fn workspace_tiling(workspace: &str) -> bool {
    let d = crate::common::run_dir();
    if default_tiling() {
        !d.join(format!("sway-floating-{workspace}")).exists()
    } else {
        d.join(format!("sway-tiling-{workspace}")).exists()
    }
}

fn write_tiling(workspace: &str, tiling: bool) {
    let d = crate::common::run_dir();
    let (exception, enable) = if default_tiling() { (format!("sway-floating-{workspace}"), !tiling) } else { (format!("sway-tiling-{workspace}"), tiling) };
    if enable {
        let _ = std::fs::write(d.join(exception), "");
    } else {
        let _ = std::fs::remove_file(d.join(exception));
    }
}

/// The tiling button in the bar: tiling = highlighted grid, floating windows = dimmed icon.
fn tiling_to_panel(sway: &mut Connection) {
    let Ok(workspaces) = sway.get_workspaces() else { return };
    let Some(workspace) = workspaces.iter().find(|p| p.focused) else { return };
    let p = &workspace.name;
    publish(
        "tiling",
        if workspace_tiling(p) {
            json!({"text": "\u{f009}", "class": "tiling", "tooltip": format!("Workspace {p}: tiled\nClick: floating windows")})
        } else {
            json!({"text": "\u{f24d}", "class": "floating", "tooltip": format!("Workspace {p}: floating windows\nClick: tile them")})
        },
    );
}

/// A new window on a tiled workspace is tiled too. Exceptions: dialogs and small windows
/// and a second window of the same app (usually a dialog, e.g. a file chooser) stay floating.
fn new_window(sway: &mut Connection, window: &Node) {
    let Ok(strom) = sway.get_tree() else { return };
    let Some(workspace) = find(&strom, &|n| n.node_type == NodeType::Workspace && find(n, &|d| d.id == window.id).is_some()) else {
        return;
    };
    if !workspace_tiling(workspace.name.as_deref().unwrap_or("")) {
        return;
    }
    let app = app_id(window);
    let role = window.window_properties.as_ref().and_then(|w| w.window_role.clone()).unwrap_or_default();
    let title = window.name.as_deref().unwrap_or("");
    if ALWAYS_FLOATING.iter().any(|v| app.contains(v))
        || role == "pop-up"
        || role == "dialog"
        || title == "Picture-in-Picture"
        || title == "Obraz v obraze"
    {
        return;
    }
    if find(workspace, &|n| n.pid.is_some() && n.id != window.id && app_id(n) == app).is_some() {
        return;
    }
    let _ = sway.run_command(format!("[con_id={}] floating disable", window.id));
}

/// Window IDs, most recently used first (for the dock: which of the app's windows to switch to).
pub fn order() -> Vec<i64> {
    ORDER.lock().unwrap().clone()
}

pub struct Window {
    pub id: i64,
    pub app: String,
    pub title: String,
    pub workspace: String,
    pub active: bool,
}

/// Windows from all workspaces: the active one first, then by last use.
pub fn windows() -> Vec<Window> {
    let Ok(strom) = Connection::new().and_then(|mut c| c.get_tree()) else {
        return Vec::new();
    };
    let mut result = Vec::new();
    fn walk(node: &Node, workspace: &str, result: &mut Vec<Window>) {
        let workspace = if node.node_type == NodeType::Workspace {
            // minimized windows (scratchpad): Alt+Tab brings them back to their workspace
            if node.name.as_deref() == Some("__i3_scratch") {
                "minimized".to_string()
            } else {
                node.name.clone().unwrap_or_default()
            }
        } else {
            workspace.to_string()
        };
        if node.pid.is_some() && matches!(node.node_type, NodeType::Con | NodeType::FloatingCon) {
            result.push(Window {
                id: node.id,
                app: node
                    .app_id
                    .clone()
                    .or_else(|| node.window_properties.as_ref().and_then(|w| w.class.clone()))
                    .unwrap_or_else(|| "?".into()),
                title: node.name.clone().unwrap_or_default(),
                workspace: workspace.clone(),
                active: node.focused,
            });
        }
        for dite in node.nodes.iter().chain(node.floating_nodes.iter()) {
            walk(dite, &workspace, result);
        }
    }
    walk(&strom, "", &mut result);
    let order = ORDER.lock().unwrap().clone();
    result.sort_by_key(|o| (!o.active, order.iter().position(|&id| id == o.id).unwrap_or(usize::MAX)));
    result
}

pub fn command(command: &str) {
    if let Ok(mut c) = Connection::new() {
        let _ = c.run_command(command);
    }
}

/// The tiling button in the bar: the whole current workspace floating ↔ tiled (never mixed).
/// The workspace mode is remembered, new windows are then added in the same mode.
/// Settings changed the default mode: clear the exceptions, switch the current workspaces and refresh the bar.
pub fn change_default_tiling(tiling: bool) {
    crate::common::save_settings("tiling_default", json!(tiling));
    if let Ok(d) = std::fs::read_dir(crate::common::run_dir()) {
        for e in d.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("sway-tiling-") || n.starts_with("sway-floating-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let Ok(mut sway) = Connection::new() else { return };
    let _ = sway.run_command(if tiling { "[floating] floating disable" } else { "[tiling] floating enable" });
    let _ = sway.send_tick("tiling");
}

pub fn toggle_tiling() {
    let Ok(mut sway) = Connection::new() else { return };
    let Ok(workspaces) = sway.get_workspaces() else { return };
    let Some(workspace) = workspaces.into_iter().find(|p| p.focused) else { return };
    let n = workspace.name;
    if workspace_tiling(&n) {
        write_tiling(&n, false);
        let _ = sway.run_command(format!("[workspace=\"^{n}$\" tiling] floating enable"));
        crate::osd::show("\u{f24d}", &format!("Workspace {n}: floating windows"), None);
    } else {
        write_tiling(&n, true);
        let _ = sway.run_command(format!("[workspace=\"^{n}$\" floating] floating disable"));
        crate::osd::show("\u{f009}", &format!("Workspace {n}: tiled"), None);
    }
    // so the bar button is recoloured at once (the Sway event thread watches for this)
    let _ = sway.send_tick("tiling");
}
