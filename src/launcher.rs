//! App grid (button in the dock): a dimmed screen with all apps and their icons in the middle.
//! Typing searches (name, description, program name). Focus stays in the search; arrows (and PageUp /
//! PageDown) move the selection in the grid, Enter starts the selected one (or the first), a click starts,
//! Esc or a click outside closes. Six columns, height in whole rows (the last one isn't cut off).
//!
//! The window is built once (in the background after the daemon starts) and then only shown and hidden,
//! so it opens instantly. The app list is refreshed when something is installed or removed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

const ICON: i32 = 56;
/// columns in the grid and the highest height of the icon part (it scrolls after that)
const COLUMNS: i32 = 6;
const HEIGHT: i32 = 560;

struct Launcher {
    window: gtk::Window,
    panel: gtk::Box,
    search: gtk::SearchEntry,
    grid: gtk::FlowBox,
    scroll: gtk::ScrolledWindow,
    /// columns in a row (depends on the screen)
    columns: Cell<i32>,
    /// row pitch measured from the drawn grid (0 = not measured yet)
    pitch: Cell<i32>,
    /// tile, search text (lower case), app
    apps: RefCell<Vec<(gtk::FlowBoxChild, String, gio::AppInfo)>>,
    /// IDs of the apps the grid is built from
    built_z: RefCell<Vec<String>>,
}

thread_local! {
    static LAUNCHER: RefCell<Option<Rc<Launcher>>> = const { RefCell::new(None) };
}

fn all() -> Vec<gio::AppInfo> {
    let mut v: Vec<gio::AppInfo> = gio::AppInfo::all().into_iter().filter(|a| a.should_show()).collect();
    v.sort_by_key(|a| a.display_name().to_lowercase());
    v
}

/// Builds the window in the background shortly after the daemon starts, so the first opening is instant.
pub fn prepare() {
    glib::timeout_add_local_once(Duration::from_secs(3), || {
        // draw it once invisibly so the icons load (otherwise the first opening is slower)
        let s = get();
        s.window.set_opacity(0.01);
        s.window.set_visible(true);
        glib::timeout_add_local_once(Duration::from_millis(400), || {
            if let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) {
                s.fit();
                s.measure_row();
                s.window.set_visible(false);
                s.window.set_opacity(1.0);
            }
        });
    });
}

fn get() -> Rc<Launcher> {
    if let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) {
        return s;
    }
    crate::style::apply();
    let window = gtk::Window::new();
    window.add_css_class("apogee");
    window.add_css_class("launcher");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("launcher"));
    window.set_exclusive_zone(-1);
    for k in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(k, true);
    }
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 14);
    panel.set_widget_name("panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Center);
    panel.set_size_request(860, -1);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search apps…"));
    // filter right away (otherwise an arrow pressed just after typing would lose against a delayed filter)
    search.set_search_delay(0);
    panel.append(&search);
    let grid = gtk::FlowBox::new();
    grid.add_css_class("apps");
    grid.set_homogeneous(true);
    grid.set_min_children_per_line(COLUMNS as u32);
    grid.set_max_children_per_line(COLUMNS as u32);
    grid.set_selection_mode(gtk::SelectionMode::Single);
    grid.set_activate_on_single_click(true);
    grid.set_row_spacing(4);
    // otherwise with few results the row stretches to the full height
    grid.set_valign(gtk::Align::Start);
    grid.set_column_spacing(4);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_size_request(-1, HEIGHT);
    scroll.set_child(Some(&grid));
    panel.append(&scroll);
    // dimming over the whole screen (GTK doesn't draw the window background itself)
    let background = gtk::Box::new(gtk::Orientation::Vertical, 0);
    background.add_css_class("dim");
    background.set_hexpand(true);
    background.set_vexpand(true);
    panel.set_vexpand(true);
    background.append(&panel);
    window.set_child(Some(&background));

    let s = Rc::new(Launcher {
        window: window.clone(),
        panel,
        search: search.clone(),
        grid: grid.clone(),
        scroll: scroll.clone(),
        columns: Cell::new(COLUMNS),
        pitch: Cell::new(0),
        apps: RefCell::new(Vec::new()),
        built_z: RefCell::new(Vec::new()),
    });

    // search
    search.connect_search_changed(|h| {
        if let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) {
            s.filter(&h.text().to_lowercase());
        }
    });
    search.connect_activate(|_| {
        if let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) {
            let selected = s.grid.selected_children().into_iter().next();
            let target = selected.or_else(|| s.apps.borrow().iter().find(|(c, _, _)| c.is_visible()).map(|(c, _, _)| c.clone()));
            if let Some(c) = target {
                s.start(&c);
            }
        }
    });
    search.connect_stop_search(|_| hide());
    // arrows move the selection in the grid, focus stays in the search (before the field moves the text cursor)
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(|_, k, _, _| {
        use gtk::gdk::Key;
        let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) else { return glib::Propagation::Proceed };
        let row = s.columns.get();
        let page = row * ((s.scroll.height() / s.height_rows().max(1)).max(1));
        let offset = match k {
            Key::Right => 1,
            Key::Left => -1,
            Key::Down => row,
            Key::Up => -row,
            Key::Page_Down => page,
            Key::Page_Up => -page,
            _ => return glib::Propagation::Proceed,
        };
        s.move_choice(offset);
        glib::Propagation::Stop
    });
    search.add_controller(keys);
    // Esc in the grid too
    let esc = gtk::EventControllerKey::new();
    esc.connect_key_pressed(|_, k, _, _| {
        if k == gtk::gdk::Key::Escape {
            hide();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(esc);
    grid.connect_child_activated(|_, c| {
        if let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) {
            s.start(c);
        }
    });
    // a click outside the panel closes it
    let click = gtk::GestureClick::new();
    click.connect_released(|g, _, x, y| {
        if let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) {
            let inside = s.window.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|w| w == *s.panel.upcast_ref::<gtk::Widget>() || w.is_ancestor(&s.panel));
            if !inside {
                g.set_state(gtk::EventSequenceState::Claimed);
                hide();
            }
        }
    });
    window.add_controller(click);

    LAUNCHER.with(|x| x.replace(Some(s.clone())));
    s.refresh_list();
    s
}

impl Launcher {
    /// Rebuilds the tiles only when the app list changed.
    fn refresh_list(&self) {
        let vse = all();
        let id: Vec<String> = vse.iter().map(|a| a.id().map(|i| i.to_string()).unwrap_or_default()).collect();
        if *self.built_z.borrow() == id {
            return;
        }
        while let Some(c) = self.grid.first_child() {
            self.grid.remove(&c);
        }
        let mut apps = Vec::new();
        for a in vse {
            let tiling = gtk::Box::new(gtk::Orientation::Vertical, 6);
            tiling.add_css_class("apps-tiling");
            let image = match a.icon() {
                Some(i) => gtk::Image::from_gicon(&i),
                None => gtk::Image::from_icon_name("application-x-executable"),
            };
            image.set_pixel_size(ICON);
            tiling.append(&image);
            let name = gtk::Label::new(Some(&a.display_name()));
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_max_width_chars(14);
            name.set_justify(gtk::Justification::Center);
            tiling.append(&name);
            let c = gtk::FlowBoxChild::new();
            c.set_child(Some(&tiling));
            self.grid.append(&c);
            // search in the name, description and program name
            let text = format!("{} {} {}", a.display_name(), a.description().unwrap_or_default(), a.executable().to_string_lossy());
            apps.push((c, text.to_lowercase(), a));
        }
        self.apps.replace(apps);
        self.built_z.replace(id);
    }

    fn filter(&self, searched: &str) {
        let mut first = None;
        for (c, text, _) in self.apps.borrow().iter() {
            let yes = searched.split_whitespace().all(|word| text.contains(word));
            c.set_visible(yes);
            if yes && first.is_none() {
                first = Some(c.clone());
            }
        }
        match first {
            Some(c) if !searched.is_empty() => self.grid.select_child(&c),
            _ => self.grid.unselect_all(),
        }
    }

    fn visible(&self) -> Vec<gtk::FlowBoxChild> {
        self.apps.borrow().iter().filter(|(c, _, _)| c.is_visible()).map(|(c, _, _)| c.clone()).collect()
    }

    /// Row pitch of the grid. Measured from the drawn grid (tiles one under another); until then, an estimate
    /// from the tile's natural height (which comes out a few pixels smaller than the real row).
    fn height_rows(&self) -> i32 {
        if self.pitch.get() > 0 {
            return self.pitch.get();
        }
        let tiling = self.visible().first().map(|c| c.measure(gtk::Orientation::Vertical, -1).1).unwrap_or(110);
        tiling + self.grid.row_spacing() as i32
    }

    /// Measures the row pitch from the drawn grid; if it differs from the one used, adjusts the size.
    fn measure_row(&self) {
        let v = self.visible();
        let Some(first) = v.first().and_then(|c| c.compute_bounds(&self.grid)) else { return };
        // the first tile one row lower (the layout may still have the old number of columns)
        let Some(next) = v.iter().filter_map(|c| c.compute_bounds(&self.grid)).map(|b| b.y()).find(|y| *y > first.y() + 1.0) else { return };
        let pitch = (next - first.y()).round() as i32;
        if pitch > 0 && pitch != self.pitch.get() {
            self.pitch.set(pitch);
            self.fit();
        }
    }

    /// Moves the selection by `o` tiles (a row = the number of columns); with no selection, selects the first.
    fn move_choice(&self, o: i32) {
        let v = self.visible();
        if v.is_empty() {
            return;
        }
        let now = self.grid.selected_children().first().and_then(|s| v.iter().position(|c| c == s));
        let nova = match now {
            None => 0,
            Some(i) => {
                let n = i as i32 + o;
                // stay put past the top / bottom edge (up a row from the first row does nothing)
                if n < 0 {
                    if o.abs() >= self.columns.get() && i as i32 >= self.columns.get() { 0 } else { i }
                } else if n >= v.len() as i32 {
                    if o.abs() >= self.columns.get() { v.len() - 1 } else { i }
                } else {
                    n as usize
                }
            }
        };
        self.grid.select_child(&v[nova]);
        // the selected tile is always fully in view
        if let Some(b) = v[nova].compute_bounds(&self.grid) {
            let a = self.scroll.vadjustment();
            let (y, h) = (b.y() as f64, b.height() as f64);
            if y < a.value() {
                a.set_value(y);
            } else if y + h > a.value() + a.page_size() {
                a.set_value(y + h - a.page_size());
            }
        }
    }

    /// Height of the icon part in whole rows (at most HEIGHT), so the last row isn't cut off.
    fn fit(&self) {
        let row = self.height_rows();
        let rows = (HEIGHT / row).max(2);
        self.scroll.set_size_request(-1, rows * row - self.grid.row_spacing() as i32);
    }

    fn start(&self, c: &gtk::FlowBoxChild) {
        let apps = self.apps.borrow().iter().find(|(x, _, _)| x == c).map(|(_, _, a)| a.clone());
        hide();
        if let Some(a) = apps {
            let context = gtk::gdk::Display::default().map(|d| d.app_launch_context());
            let _ = a.launch(&[], context.as_ref());
        }
    }
}

pub fn toggle() {
    let s = get();
    if s.window.is_visible() {
        hide();
        return;
    }
    crate::popups::close();
    s.refresh_list();
    s.search.set_text("");
    s.filter("");
    s.fit();
    s.scroll.vadjustment().set_value(0.0);
    glib::timeout_add_local_once(Duration::from_millis(60), || {
        if let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) {
            s.measure_row();
        }
    });
    s.window.set_visible(true);
    s.search.grab_focus();
}

fn hide() {
    if let Some(s) = LAUNCHER.with(|s| s.borrow().clone()) {
        s.window.set_visible(false);
    }
}
