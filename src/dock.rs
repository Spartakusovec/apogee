//! The dock at the bottom of the screen: pinned and running apps (dots = number of windows) and a button
//! with the app grid. It replaced a patched nwg-dock.
//!
//! Click: start / switch to the app's window (another click cycles through its windows),
//! middle click: new window, right click: menu (windows, new window, pin, close).
//!
//! Smart hiding: the dock is visible until a window on the current workspace covers it. When hidden it shows
//! when the mouse rests at the bottom screen edge for a moment (also over a fullscreen window).
//! It doesn't hide while the mouse or an open menu is over it. Super+X shows / hides it.
//! Pinned apps: ~/.config/apogee/settings.json (“dock”).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use swayipc::{Connection, Node, NodeType};

use crate::common::*;

const ICON: i32 = 40;
/// gap between the dock and the bottom screen edge
const MARGIN: i32 = 8;

#[derive(Clone)]
struct Window {
    id: i64,
    title: String,
    active: bool,
    /// minimized (in Sway's scratchpad)
    hidden: bool,
}

#[derive(Clone)]
struct Item {
    /// .desktop file name without the extension (org.mozilla.firefox), app_id for unknown apps
    key: String,
    apps: Option<gio::AppInfo>,
    /// most recently used first
    windows: Vec<Window>,
}

struct Dock {
    window: gtk::Window,
    rada: gtk::Box,
    /// strip at the bottom edge while the dock is hidden (a new one every time, see show_zone)
    hot_zone: RefCell<Option<gtk::Window>>,
    /// no window covers the dock
    free: Cell<bool>,
    mouse_v_dock: Cell<bool>,
    menu: Cell<bool>,
    /// shown by hand (mouse at the edge, Super+X), even though a window covers it
    manual: Cell<bool>,
    hide: RefCell<Option<glib::SourceId>>,
    refresh: RefCell<Option<glib::SourceId>>,
    /// keys of the dock items (rebuilt only when they change; otherwise only the dots are recoloured)
    state: RefCell<Vec<String>>,
    /// current items (click and menu look here, not at the state from when the button was made)
    items: RefCell<Vec<Item>>,
    /// button and dots of each item
    buttons: RefCell<HashMap<String, (gtk::Button, gtk::Box)>>,
    /// apps by key (gio::AppInfo::all() is slow to call on every event)
    apps: RefCell<HashMap<String, Option<gio::AppInfo>>>,
    /// window app_id → item key
    keys: RefCell<HashMap<String, String>>,
}

thread_local! {
    static DOCK: RefCell<Option<Rc<Dock>>> = const { RefCell::new(None) };
}

/// A notice from other threads (Sway events): something changed, refresh the dock.
static NOTIFIER: OnceLock<async_channel::Sender<()>> = OnceLock::new();

pub fn notify() {
    if let Some(s) = NOTIFIER.get() {
        let _ = s.try_send(());
    }
}

fn dock() -> Option<Rc<Dock>> {
    DOCK.with(|d| d.borrow().clone())
}

pub fn pinned() -> Vec<String> {
    match settings("dock").as_array() {
        Some(a) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        // the first time: common apps (and save them right away, so the list stays stable)
        None => {
            let default: Vec<String> = ["org.mozilla.firefox", "org.gnome.Nautilus", "org.gnome.Ptyxis"].map(String::from).to_vec();
            save_pinned(&default);
            default
        }
    }
}

fn save_pinned(list: &[String]) {
    save_settings("dock", serde_json::json!(list));
}

/// New order / list of pinned apps from the Settings app.
pub fn set_pinned(list: &[String]) {
    save_pinned(list);
    if let Some(d) = dock() {
        d.state.replace(Vec::new());
        d.refresh();
    }
}

/// Dock hiding: "smart" (when it's in a window's way, the default), "never", "always".
pub fn hiding() -> String {
    settings("dock_hiding").as_str().unwrap_or("smart").to_string()
}

pub fn set_hiding(mode: &str) {
    save_settings("dock_hiding", serde_json::json!(mode));
    if let Some(d) = dock() {
        d.refresh();
    }
}

/// Name and icon of a pinned app (for Settings).
pub fn info_apps(key: &str) -> Option<gio::AppInfo> {
    apps(key)
}

fn apps(key: &str) -> Option<gio::AppInfo> {
    // gio keeps the app list in memory and watches it, this is fast
    let id = format!("{key}.desktop");
    gio::AppInfo::all().into_iter().find(|a| a.id().is_some_and(|i| i == id))
}

/// The strip at the bottom edge: resting the mouse on it = show the hidden dock.
fn create_hot_zone() -> gtk::Window {
    let window = gtk::Window::new();
    window.add_css_class("apogee");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_namespace(Some("dock-zone"));
    window.set_exclusive_zone(-1);
    for k in [Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(k, true);
    }
    let fill = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    fill.set_size_request(-1, 2);
    // a fully transparent strip doesn't catch the mouse
    fill.add_css_class("strip");
    window.set_child(Some(&fill));
    let mouse = gtk::EventControllerMotion::new();
    let wait: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let c = wait.clone();
    mouse.connect_enter(move |_, _, _| {
        let id = glib::timeout_add_local_once(Duration::from_millis(150), {
            let c = c.clone();
            move || {
                c.replace(None);
                if let Some(d) = dock() {
                    d.manual.set(true);
                    d.show();
                    // if the mouse doesn't move onto the dock, hide it again
                    d.schedule_hide(2500);
                }
            }
        });
        if let Some(stare) = c.replace(Some(id)) {
            stare.remove();
        }
    });
    mouse.connect_leave(move |_| {
        if let Some(id) = wait.take() {
            id.remove();
        }
    });
    window.add_controller(mouse);
    window
}

pub fn start() {
    crate::style::apply();
    let window = gtk::Window::new();
    window.add_css_class("apogee");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_namespace(Some("dock"));
    window.set_anchor(Edge::Bottom, true);
    window.set_margin(Edge::Bottom, MARGIN);
    let rada = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    rada.set_widget_name("dock");
    window.set_child(Some(&rada));

    let d = Rc::new(Dock {
        window: window.clone(),
        rada,
        hot_zone: RefCell::new(None),
        free: Cell::new(true),
        mouse_v_dock: Cell::new(false),
        menu: Cell::new(false),
        manual: Cell::new(false),
        hide: RefCell::new(None),
        refresh: RefCell::new(None),
        state: RefCell::new(Vec::new()),
        items: RefCell::new(Vec::new()),
        buttons: RefCell::new(HashMap::new()),
        apps: RefCell::new(HashMap::new()),
        keys: RefCell::new(HashMap::new()),
    });

    let mouse = gtk::EventControllerMotion::new();
    mouse.connect_enter(|_, _, _| {
        if let Some(d) = dock() {
            d.mouse_v_dock.set(true);
            d.cancel_hide();
        }
    });
    mouse.connect_leave(|_| {
        if let Some(d) = dock() {
            d.mouse_v_dock.set(false);
            d.schedule_hide(1000);
        }
    });
    window.add_controller(mouse);
    DOCK.with(|x| x.replace(Some(d.clone())));

    let (tx, rx) = async_channel::bounded::<()>(1);
    let _ = NOTIFIER.set(tx);
    glib::spawn_future_local(async move {
        while rx.recv().await.is_ok() {
            if let Some(d) = dock() {
                d.schedule_refresh();
            }
        }
    });
    d.refresh();
    d.window.present();
}

/// Super+X, three-finger swipe up: show / hide.
pub fn toggle() {
    let Some(d) = dock() else { return };
    if d.window.is_visible() {
        d.manual.set(false);
        d.cancel_hide();
        d.window.set_visible(false);
        d.show_zone(true);
    } else {
        d.manual.set(true);
        d.show();
        d.schedule_hide(2500);
    }
}

impl Dock {
    fn show(&self) {
        self.cancel_hide();
        if !self.window.is_visible() {
            self.window.set_visible(true);
        }
        self.show_zone(false);
    }

    /// The strip is created anew each time and then destroyed (not just hidden): if it hides under the cursor,
    /// GTK never learns the cursor left and won't report its next arrival (the dock would only show the second time).
    fn show_zone(&self, show: bool) {
        let mut z = self.hot_zone.borrow_mut();
        if show {
            if z.is_none() {
                let window = create_hot_zone();
                window.present();
                *z = Some(window);
            }
        } else if let Some(window) = z.take() {
            window.destroy();
        }
    }

    fn cancel_hide(&self) {
        if let Some(id) = self.hide.take() {
            id.remove();
        }
    }

    /// Hide after a moment if something still covers the dock and neither the mouse nor a menu is over it.
    fn schedule_hide(&self, ms: u64) {
        self.cancel_hide();
        let id = glib::timeout_add_local_once(Duration::from_millis(ms), || {
            if let Some(d) = dock() {
                d.hide.replace(None);
                d.manual.set(false);
                d.hide_when_in_the_way();
            }
        });
        self.hide.replace(Some(id));
    }

    fn hide_when_in_the_way(&self) {
        if self.free.get() || self.mouse_v_dock.get() || self.menu.get() || self.manual.get() {
            return;
        }
        self.window.set_visible(false);
        self.show_zone(true);
    }

    /// Several Sway events in a row (opening a window = several events) → refresh once.
    fn schedule_refresh(&self) {
        if self.refresh.borrow().is_some() {
            return;
        }
        let id = glib::timeout_add_local_once(Duration::from_millis(40), || {
            if let Some(d) = dock() {
                d.refresh.replace(None);
                d.refresh();
            }
        });
        self.refresh.replace(Some(id));
    }

    fn key_windows(&self, app_id: &str) -> String {
        if let Some(k) = self.keys.borrow().get(app_id) {
            return k.clone();
        }
        let k = crate::apps::path(app_id)
            .and_then(|c| c.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| app_id.to_string());
        self.keys.borrow_mut().insert(app_id.to_string(), k.clone());
        k
    }

    /// Loads the windows from Sway, redraws the dock (if it changed) and shows / hides it depending on the windows.
    fn refresh(self: &Rc<Self>) {
        let Ok(mut sway) = Connection::new() else { return };
        let (Ok(strom), Ok(workspaces), Ok(outputs)) = (sway.get_tree(), sway.get_workspaces(), sway.get_outputs()) else { return };

        // windows by app, most recently used first
        let mut windows: Vec<(String, Window, i64)> = Vec::new();
        fn walk(n: &Node, v: &mut Vec<(String, Window, i64)>, hidden: bool) {
            // minimized windows too (scratchpad): a click in the dock brings them back
            let hidden = hidden || (n.node_type == NodeType::Workspace && n.name.as_deref() == Some("__i3_scratch"));
            if n.pid.is_some() && matches!(n.node_type, NodeType::Con | NodeType::FloatingCon) {
                let app = n.app_id.clone().or_else(|| n.window_properties.as_ref().and_then(|w| w.class.clone())).unwrap_or_default();
                v.push((app, Window { id: n.id, title: n.name.clone().unwrap_or_default(), active: n.focused, hidden }, n.id));
            }
            for d in n.nodes.iter().chain(n.floating_nodes.iter()) {
                walk(d, v, hidden);
            }
        }
        walk(&strom, &mut windows, false);
        let order = crate::sway::order();
        let mut by_apps: Vec<(String, Vec<(Window, i64)>)> = Vec::new();
        windows.sort_by_key(|(_, _, id)| *id); // running unpinned apps in the order they were opened
        for (app, o, id) in windows {
            let key = self.key_windows(&app);
            match by_apps.iter_mut().find(|(k, _)| *k == key) {
                Some((_, v)) => v.push((o, id)),
                None => by_apps.push((key, vec![(o, id)])),
            }
        }
        let mut items: Vec<Item> = Vec::new();
        let pinned = pinned();
        for key in pinned.iter().chain(by_apps.iter().map(|(k, _)| k).filter(|k| !pinned.contains(k))) {
            let mut o: Vec<Window> = by_apps.iter().find(|(k, _)| k == key).map(|(_, v)| v.iter().map(|(o, _)| o.clone()).collect()).unwrap_or_default();
            o.sort_by_key(|x| order.iter().position(|&id| id == x.id).unwrap_or(usize::MAX));
            if items.iter().any(|p| &p.key == key) {
                continue;
            }
            let app = self.apps.borrow_mut().entry(key.clone()).or_insert_with(|| apps(key)).clone();
            items.push(Item { key: key.clone(), apps: app, windows: o });
        }

        let keys: Vec<String> = items.iter().map(|p| p.key.clone()).collect();
        if *self.state.borrow() != keys {
            self.rebuild(&items);
            self.state.replace(keys);
        } else {
            // recolour only: active app and number of windows
            for p in &items {
                if let Some((b, dots)) = self.buttons.borrow().get(&p.key) {
                    let active = p.windows.iter().any(|o| o.active);
                    if active != b.has_css_class("active") {
                        if active {
                            b.add_css_class("active");
                        } else {
                            b.remove_css_class("active");
                        }
                    }
                    set_dots(dots, &p.windows);
                }
            }
        }
        self.items.replace(items);

        // does a window on the workspace below cover the dock?
        let Some(workspace) = workspaces.iter().find(|p| p.focused) else { return };
        let Some(output) = outputs.iter().find(|o| o.name == workspace.output) else { return };
        let (width, height) = (self.window.width().max(1), self.window.height().max(1));
        let (width, height) = if self.window.is_visible() { (width, height) } else { (self.rada.preferred_size().1.width(), self.rada.preferred_size().1.height()) };
        let r = &output.rect;
        let oblast = (r.x + (r.width - width) / 2, r.y + r.height - height - MARGIN, width, height);
        let ws = workspaces.iter().filter(|p| p.output == workspace.output && p.visible).map(|p| p.name.clone()).collect::<Vec<_>>();
        fn overlaps(n: &Node, oblast: (i32, i32, i32, i32)) -> bool {
            let r = &n.rect;
            if n.pid.is_some() && n.visible != Some(false) && r.x < oblast.0 + oblast.2 && r.x + r.width > oblast.0 && r.y < oblast.1 + oblast.3 && r.y + r.height > oblast.1 {
                return true;
            }
            n.nodes.iter().chain(n.floating_nodes.iter()).any(|d| overlaps(d, oblast))
        }
        fn workspaces_node<'a>(n: &'a Node, names: &[String], v: &mut Vec<&'a Node>) {
            if n.node_type == NodeType::Workspace && n.name.as_ref().is_some_and(|j| names.contains(j)) {
                v.push(n);
                return;
            }
            for d in &n.nodes {
                workspaces_node(d, names, v);
            }
        }
        let mut nodes = Vec::new();
        workspaces_node(&strom, &ws, &mut nodes);
        let covered = match hiding().as_str() {
            "never" => false,
            "always" => true,
            _ => nodes.iter().any(|w| overlaps(w, oblast)),
        };
        self.free.set(!covered);
        if covered {
            self.hide_when_in_the_way();
        } else {
            self.show();
        }
    }

    fn rebuild(self: &Rc<Self>, items: &[Item]) {
        while let Some(d) = self.rada.first_child() {
            self.rada.remove(&d);
        }
        self.buttons.borrow_mut().clear();
        for p in items {
            self.rada.append(&self.button(p));
        }
        // app grid
        let grid = gtk::Button::new();
        grid.add_css_class("dock-item");
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // 3×3 rounded squares, the middle one in the accent colour (like nwg-dock had)
        let squares = gtk::Grid::new();
        squares.add_css_class("dock-grid");
        squares.set_row_spacing(3);
        squares.set_column_spacing(3);
        squares.set_halign(gtk::Align::Center);
        squares.set_valign(gtk::Align::Center);
        for i in 0..9 {
            let c = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            c.add_css_class("square");
            if i == 4 {
                c.add_css_class("center");
            }
            squares.attach(&c, i % 3, i / 3, 1, 1);
        }
        let place = gtk::Box::new(gtk::Orientation::Vertical, 0);
        place.set_size_request(ICON, ICON);
        place.append(&squares);
        squares.set_vexpand(true);
        content.append(&place);
        // empty space as tall as the dots under the apps, so the grid sits at the same height as the icons
        let place_dots = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        place_dots.add_css_class("dots");
        content.append(&place_dots);
        grid.set_child(Some(&content));
        grid.set_tooltip_text(Some("All apps"));
        grid.connect_clicked(|_| crate::launcher::toggle());
        self.rada.append(&grid);
        // otherwise the window keeps its old (bigger) width: a black strip would stay after an app closed
        self.window.set_default_size(1, 1);
    }

    fn button(self: &Rc<Self>, p: &Item) -> gtk::Button {
        let b = gtk::Button::new();
        b.add_css_class("dock-item");
        if p.windows.iter().any(|o| o.active) {
            b.add_css_class("active");
        }
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let image = match p.apps.as_ref().and_then(|a| a.icon()) {
            Some(i) => gtk::Image::from_gicon(&i),
            None => gtk::Image::from_icon_name(&crate::apps::info(&p.key).icon),
        };
        image.set_pixel_size(ICON);
        content.append(&image);
        let dots = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dots.set_halign(gtk::Align::Center);
        dots.add_css_class("dots");
        set_dots(&dots, &p.windows);
        content.append(&dots);
        self.buttons.borrow_mut().insert(p.key.clone(), (b.clone(), dots.clone()));
        b.set_child(Some(&content));
        let name = p.apps.as_ref().map(|a| a.name().to_string()).unwrap_or_else(|| crate::apps::info(&p.key).name);
        b.set_tooltip_text(Some(&name));

        // click and menu take the item's current state (the windows may have changed meanwhile)
        let key = p.key.clone();
        b.connect_clicked(move |_| {
            if let Some(p) = current(&key) {
                click(&p);
            }
        });
        let middle = gtk::GestureClick::new();
        middle.set_button(2);
        let key = p.key.clone();
        middle.connect_released(move |_, _, _, _| {
            if let Some(p) = current(&key) {
                start_apps(&p);
            }
        });
        b.add_controller(middle);
        let right = gtk::GestureClick::new();
        right.set_button(3);
        let (key, slaby) = (p.key.clone(), b.downgrade());
        right.connect_released(move |_, _, _, _| {
            if let (Some(b), Some(p)) = (slaby.upgrade(), current(&key)) {
                let pinned = pinned().contains(&p.key);
                menu(&b, &p, pinned);
            }
        });
        b.add_controller(right);
        b
    }
}

/// Dots under the icon: one per window (at most 3), minimized ones as a ring; changed only when
/// needed. Visible windows first.
fn set_dots(dots: &gtk::Box, windows: &[Window]) {
    let mut hidden: Vec<bool> = windows.iter().map(|o| o.hidden).collect();
    hidden.sort();
    hidden.truncate(3);
    let mut now = 0;
    let mut d = dots.first_child();
    while let Some(w) = d {
        now += 1;
        d = w.next_sibling();
    }
    while now < hidden.len() {
        let t = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        t.add_css_class("dot");
        dots.append(&t);
        now += 1;
    }
    while now > hidden.len() {
        if let Some(w) = dots.last_child() {
            dots.remove(&w);
        }
        now -= 1;
    }
    let mut d = dots.first_child();
    for s in hidden {
        let Some(w) = d else { break };
        if s != w.has_css_class("hidden") {
            if s {
                w.add_css_class("hidden");
            } else {
                w.remove_css_class("hidden");
            }
        }
        d = w.next_sibling();
    }
}

fn current(key: &str) -> Option<Item> {
    dock()?.items.borrow().iter().find(|p| p.key == key).cloned()
}

fn start_apps(p: &Item) {
    if let Some(a) = &p.apps {
        let context = gtk::gdk::Display::default().map(|d| d.app_launch_context());
        let _ = a.launch(&[], context.as_ref());
    }
}

/// Click (like Windows): nothing running = start; app not active = its last used window
/// (minimized ones come back); active app with one visible window = minimize, with more
/// windows = the next visible window.
fn click(p: &Item) {
    if p.windows.is_empty() {
        start_apps(p);
        return;
    }
    let visible: Vec<&Window> = p.windows.iter().filter(|o| !o.hidden).collect();
    match visible.iter().position(|o| o.active) {
        Some(i) if visible.len() > 1 => {
            let target = visible[(i + 1) % visible.len()];
            crate::sway::command(&format!("[con_id={}] focus", target.id));
        }
        Some(i) => crate::sway::command(&format!("[con_id={}] minimize", visible[i].id)),
        None => crate::sway::command(&format!("[con_id={}] focus", p.windows[0].id)),
    }
}

fn menu(button: &gtk::Button, p: &Item, is_pinned: bool) {
    let pop = gtk::Popover::new();
    pop.add_css_class("dock-menu");
    pop.set_has_arrow(false);
    pop.set_position(gtk::PositionType::Top);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
    content.set_widget_name("panel");
    let close = {
        let pop = pop.downgrade();
        move || {
            if let Some(p) = pop.upgrade() {
                p.popdown();
            }
        }
    };
    for o in &p.windows {
        let mut t = o.title.clone();
        if t.chars().count() > 40 {
            t = t.chars().take(39).collect::<String>() + "…";
        }
        let (id, z) = (o.id, close.clone());
        content.append(&row("\u{f2d0}", &t, "", o.active, move || {
            z();
            crate::sway::command(&format!("[con_id={id}] focus"));
        }));
    }
    if !p.windows.is_empty() {
        content.append(&separator());
    }
    if p.apps.is_some() {
        let (item, z) = (p.clone(), close.clone());
        let text = if p.windows.is_empty() { "Open" } else { "New window" };
        content.append(&row("\u{f067}", text, "", false, move || {
            z();
            start_apps(&item);
        }));
        let (key, z) = (p.key.clone(), close.clone());
        let (char, text) = if is_pinned { ("\u{f08d}", "Unpin from dock") } else { ("\u{f08d}", "Pin to dock") };
        content.append(&row(char, text, "", false, move || {
            z();
            let mut list = pinned();
            if is_pinned {
                list.retain(|k| *k != key);
            } else {
                list.push(key.clone());
            }
            save_pinned(&list);
            if let Some(d) = dock() {
                d.state.replace(Vec::new());
                d.refresh();
            }
        }));
    }
    if !p.windows.is_empty() {
        let (windows, z) = (p.windows.clone(), close.clone());
        let text = if p.windows.len() > 1 { "Close all windows" } else { "Close" };
        content.append(&row("\u{f00d}", text, "", false, move || {
            z();
            for o in &windows {
                crate::sway::command(&format!("[con_id={}] kill", o.id));
            }
        }));
    }
    pop.set_child(Some(&content));
    pop.set_parent(button);
    if let Some(d) = dock() {
        d.menu.set(true);
    }
    pop.connect_closed(|pop| {
        if let Some(d) = dock() {
            d.menu.set(false);
            if !d.mouse_v_dock.get() {
                d.schedule_hide(1000);
            }
        }
        // free the popover after it closes (only once the event is handled)
        let pop = pop.clone();
        glib::idle_add_local_once(move || pop.unparent());
    });
    pop.popup();
}
