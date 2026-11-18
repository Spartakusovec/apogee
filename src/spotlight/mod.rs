//! Spotlight (Super+Space): one field at the top centre of the screen with the results under it.
//!
//!   apps         – by name, description and program name; often started ones higher
//!   calculator   – “2+3*4”, “200 + 10%”, “sqrt(2)”; Enter copies the result
//!   conversions  – “10 km to mi”, “100 eur to czk” (ECB rates)
//!   commands     – “lock”, “restart”, “wifi”, “wallpaper”… (in English and Czech)
//!   files        – recently opened and from the home folder (plocate); Ctrl+Enter shows the file in its folder.
//!                   Its own index of the home folder (without hidden folders, ~/.cache/apogee/files.db) is refreshed
//!                   on opening when it's older than 2 minutes (the system one is refreshed only once a day).
//!   web          – the last row: search the web (Firefox, default search engine)
//!
//! Arrows select, Enter runs, Esc or a click outside closes. The window is built once and then only
//! shown, so it's on screen instantly.

mod units;
mod calculator;
mod commands;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::json;

use crate::common::*;

const ICON: i32 = 38;
const MAX_APPS: usize = 6;
const MAX_FILES: usize = 6;

/// What a row does (true = with Ctrl).
type Action = Rc<dyn Fn(bool)>;

struct Spotlight {
    window: gtk::Window,
    panel: gtk::Box,
    search: gtk::Entry,
    scroll: gtk::ScrolledWindow,
    list: gtk::ListBox,
    /// selectable rows and what they do
    rows: RefCell<Vec<(gtk::ListBoxRow, Action)>>,
    apps: RefCell<Vec<(gio::AppInfo, String, String)>>,
    /// file search results for a query (query, paths)
    files: RefCell<(String, Vec<PathBuf>)>,
    search_files: RefCell<Option<glib::SourceId>>,
}

thread_local! {
    static SPOTLIGHT: RefCell<Option<Rc<Spotlight>>> = const { RefCell::new(None) };
}

pub fn without_diacritics(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'á' => 'a',
            'č' => 'c',
            'ď' => 'd',
            'é' | 'ě' => 'e',
            'í' => 'i',
            'ň' => 'n',
            'ó' => 'o',
            'ř' => 'r',
            'š' => 's',
            'ť' => 't',
            'ú' | 'ů' => 'u',
            'ý' => 'y',
            'ž' => 'z',
            c => c,
        })
        .collect()
}

fn normalize(text: &str) -> String {
    without_diacritics(&text.to_lowercase())
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn spotlight() -> Option<Rc<Spotlight>> {
    SPOTLIGHT.with(|s| s.borrow().clone())
}

/// Builds the window shortly after the daemon starts and draws it once invisibly.
pub fn prepare() {
    glib::timeout_add_local_once(Duration::from_secs(4), || {
        let s = get();
        s.window.set_opacity(0.01);
        s.window.set_visible(true);
        glib::timeout_add_local_once(Duration::from_millis(300), || {
            if let Some(s) = spotlight() {
                s.window.set_visible(false);
                s.window.set_opacity(1.0);
            }
        });
    });
}

fn get() -> Rc<Spotlight> {
    if let Some(s) = spotlight() {
        return s;
    }
    crate::style::apply();
    let window = gtk::Window::new();
    window.add_css_class("apogee");
    window.add_css_class("spotlight");
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_namespace(Some("spotlight"));
    window.set_exclusive_zone(-1);
    for k in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(k, true);
    }
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_widget_name("panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Start);
    panel.set_margin_top(150);
    panel.set_size_request(780, -1);
    let field = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    field.add_css_class("spotlight-field");
    field.append(&icon("\u{f002}", "spotlight-zoom"));
    let search = gtk::Entry::new();
    search.set_placeholder_text(Some("Search apps, files, calculate, convert…"));
    search.set_hexpand(true);
    field.append(&search);
    panel.append(&field);
    let list = gtk::ListBox::new();
    list.add_css_class("spotlight-results");
    list.set_selection_mode(gtk::SelectionMode::Single);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(600);
    scroll.set_child(Some(&list));
    scroll.set_visible(false);
    panel.append(&scroll);
    let background = gtk::Box::new(gtk::Orientation::Vertical, 0);
    background.set_hexpand(true);
    background.set_vexpand(true);
    background.append(&panel);
    window.set_child(Some(&background));

    let s = Rc::new(Spotlight {
        window: window.clone(),
        panel,
        search: search.clone(),
        scroll,
        list: list.clone(),
        rows: RefCell::new(Vec::new()),
        apps: RefCell::new(Vec::new()),
        files: RefCell::new((String::new(), Vec::new())),
        search_files: RefCell::new(None),
    });

    search.connect_changed(|_| {
        if let Some(s) = spotlight() {
            s.draw();
            s.schedule_search_files();
        }
    });
    // keys before the field gets them: arrows select, Enter runs, Esc closes
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(|_, k, _, mod_| {
        use gtk::gdk::Key;
        let Some(s) = spotlight() else { return glib::Propagation::Proceed };
        match k {
            Key::Escape => hide(),
            Key::Down | Key::Tab => s.move_(1),
            Key::Up | Key::ISO_Left_Tab => s.move_(-1),
            Key::Return | Key::KP_Enter => s.start_selected(mod_.contains(gtk::gdk::ModifierType::CONTROL_MASK)),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    window.add_controller(keys);
    list.connect_row_activated(|_, r| {
        if let Some(s) = spotlight() {
            s.list.select_row(Some(r));
            s.start_selected(false);
        }
    });
    // a click outside the panel closes it
    let click = gtk::GestureClick::new();
    click.connect_released(|g, _, x, y| {
        if let Some(s) = spotlight() {
            let inside = s.window.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|w| w == *s.panel.upcast_ref::<gtk::Widget>() || w.is_ancestor(&s.panel));
            if !inside {
                g.set_state(gtk::EventSequenceState::Claimed);
                hide();
            }
        }
    });
    window.add_controller(click);

    SPOTLIGHT.with(|x| x.replace(Some(s.clone())));
    s.load_apps();
    s
}

/// Open / close; with a text, opens right away with that search.
pub fn toggle(text: &str) {
    let s = get();
    if s.window.is_visible() && text.is_empty() {
        hide();
        return;
    }
    crate::popups::close();
    s.load_apps();
    refresh_index();
    s.search.set_text(text);
    s.draw();
    s.window.set_visible(true);
    // cursor to the end, don't select the text (so you can keep typing)
    s.search.grab_focus_without_selecting();
    s.search.set_position(-1);
    // currency rates in the background, so they're at hand
    if units::rates_stale() {
        in_background(units::download_rates, |_| {});
    }
}

fn hide() {
    if let Some(s) = spotlight() {
        s.window.set_visible(false);
        if let Some(id) = s.search_files.take() {
            id.remove();
        }
    }
}

/// How many times and when an app was last started from Spotlight: {"id": [count, time]}.
fn usage() -> serde_json::Value {
    settings("spotlight")["usage"].clone()
}

fn write_usage(id: &str) {
    let mut v = settings("spotlight");
    if !v.is_object() {
        v = json!({});
    }
    let count = v["usage"][id][0].as_u64().unwrap_or(0) + 1;
    if !v["usage"].is_object() {
        v["usage"] = json!({});
    }
    v["usage"][id] = json!([count, now()]);
    save_settings("spotlight", v);
}

/// Ikona souboru podle typu.
fn icon_files(path: &Path) -> gtk::Image {
    let icon = if path.is_dir() {
        gio::ThemedIcon::new("folder").upcast::<gio::Icon>()
    } else {
        let (typ, _) = gio::content_type_guess(Some(path), None);
        gio::content_type_get_icon(&typ)
    };
    let i = gtk::Image::from_gicon(&icon);
    i.set_pixel_size(ICON);
    i
}

fn shorten(path: &Path) -> String {
    let d = home();
    match path.strip_prefix(&d) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Shows the file in the file manager (Ctrl+Enter).
fn show_ve_folder(path: &Path) {
    let uri = gio::File::for_path(path).uri().to_string();
    if let Ok(s) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        s.call(
            Some("org.freedesktop.FileManager1"),
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1",
            "ShowItems",
            Some(&(vec![uri], "").to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
            |_| {},
        );
    }
}

fn open(path: &Path) {
    let uri = gio::File::for_path(path).uri();
    let context = gtk::gdk::Display::default().map(|d| d.app_launch_context());
    let _ = gio::AppInfo::launch_default_for_uri(&uri, context.as_ref());
}

fn copy(text: &str) {
    if let Some(d) = gtk::gdk::Display::default() {
        d.clipboard().set_text(text);
    }
    // the Wayland clipboard belongs to the window that set it: make it survive Spotlight closing
    background("wl-copy", &[text]);
}

fn search_on_web(query: &str) {
    let firefox = gio::AppInfo::all().into_iter().any(|a| a.id().is_some_and(|i| i == "org.mozilla.firefox.desktop"));
    if firefox {
        background("flatpak", &["run", "org.mozilla.firefox", "--search", query]);
    } else {
        let uri = format!("https://duckduckgo.com/?q={}", glib::Uri::escape_string(query, None, false));
        background("xdg-open", &[&uri]);
    }
}

fn index_files() -> PathBuf {
    home().join(".cache/apogee/files.db")
}

thread_local! {
    static INDEXUJE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Refreshes the home index in the background (at most every 2 minutes); then repeats the search being typed,
/// so a file downloaded a moment ago shows up too.
fn refresh_index() {
    let db = index_files();
    let cerstvy = std::fs::metadata(&db)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t.elapsed().is_ok_and(|e| e < Duration::from_secs(120)));
    if cerstvy || INDEXUJE.with(|i| i.replace(true)) {
        return;
    }
    in_background(
        move || {
            let home = home();
            // Spotlight doesn't show hidden folders, so they needn't be in the index either
            let hidden: Vec<String> = std::fs::read_dir(&home)
                .map(|d| {
                    d.flatten()
                        .filter(|e| e.file_name().to_string_lossy().starts_with('.') && e.file_type().is_ok_and(|t| t.is_dir()))
                        .map(|e| e.path().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let _ = std::fs::create_dir_all(db.parent().unwrap_or(&home));
            let (home, db) = (home.to_string_lossy().to_string(), db.to_string_lossy().to_string());
            command("nice", &["-n", "19", "ionice", "-c", "3", "updatedb", "-l", "0", "-U", &home, "-o", &db, "-e", &hidden.join(" ")]);
        },
        |_| {
            INDEXUJE.with(|i| i.set(false));
            if let Some(s) = spotlight() {
                if s.window.is_visible() {
                    s.schedule_search_files();
                }
            }
        },
    );
}

/// Files from the home folder (plocate) and recently opened ones; slow, call outside GTK.
fn find_files(query: &str) -> Vec<PathBuf> {
    let words: Vec<String> = query.split_whitespace().map(str::to_string).collect();
    if words.is_empty() || query.chars().count() < 2 {
        return Vec::new();
    }
    let home = home();
    let mut args = vec!["-i".to_string(), "-b".into(), "-l".into(), "400".into()];
    // our own fresh home index, otherwise the system one
    let db = index_files();
    if db.exists() {
        args.extend(["-d".to_string(), db.to_string_lossy().to_string()]);
    }
    args.push("--".into());
    args.extend(words.iter().cloned());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let vynechat = ["node_modules", "target", "__pycache__", "site-packages", "venv"];
    let mut found: Vec<PathBuf> = command("plocate", &args)
        .lines()
        .map(PathBuf::from)
        .filter(|p| p.starts_with(&home))
        .filter(|p| {
            p.strip_prefix(&home).map_or(false, |z| {
                z.components().all(|c| {
                    let c = c.as_os_str().to_string_lossy();
                    !c.starts_with('.') && !vynechat.contains(&c.as_ref())
                })
            })
        })
        .filter(|p| p.exists())
        .collect();
    // names starting with the searched text first, then shorter paths
    let first = normalize(&words[0]);
    found.sort_by_key(|p| {
        let name = normalize(&p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        (!name.starts_with(&first), p.components().count(), name.len())
    });
    found.truncate(MAX_FILES * 2);
    found
}

impl Spotlight {
    fn load_apps(&self) {
        let mut v: Vec<(gio::AppInfo, String, String)> = gio::AppInfo::all()
            .into_iter()
            .filter(|a| a.should_show())
            .map(|a| {
                let name = normalize(&a.display_name());
                let ostatni = normalize(&format!("{} {}", a.description().unwrap_or_default(), a.executable().to_string_lossy()));
                (a, name, ostatni)
            })
            .collect();
        v.sort_by(|a, b| a.1.cmp(&b.1));
        self.apps.replace(v);
    }

    fn schedule_search_files(&self) {
        if let Some(id) = self.search_files.take() {
            id.remove();
        }
        let query = self.search.text().trim().to_string();
        if query.chars().count() < 2 || calculator::compute(&query).is_some() || units::convert(&query).is_some() {
            return;
        }
        // once typing pauses for a moment
        let id = glib::timeout_add_local_once(Duration::from_millis(120), move || {
            if let Some(s) = spotlight() {
                s.search_files.replace(None);
            }
            let d = query.clone();
            in_background(
                move || find_files(&d),
                move |found| {
                    if let Some(s) = spotlight() {
                        if s.search.text().trim() == query && s.window.is_visible() {
                            s.files.replace((query, found));
                            s.draw();
                        }
                    }
                },
            );
        });
        self.search_files.replace(Some(id));
    }

    /// Heading of a result group (can't be selected).
    fn title(&self, text: &str) {
        let r = gtk::ListBoxRow::new();
        r.set_selectable(false);
        r.set_activatable(false);
        r.set_can_focus(false);
        r.add_css_class("group");
        r.set_child(Some(&label(text, &["dim"])));
        self.list.append(&r);
    }

    fn row(&self, icon_widget: gtk::Widget, name: &str, description: &str, action: Action) {
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        icon_widget.set_size_request(ICON, ICON);
        icon_widget.set_valign(gtk::Align::Center);
        content.append(&icon_widget);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_valign(gtk::Align::Center);
        texts.set_hexpand(true);
        let n = label(name, &["result"]);
        n.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texts.append(&n);
        if !description.is_empty() {
            let p = label(description, &["dim"]);
            p.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            texts.append(&p);
        }
        content.append(&texts);
        let r = gtk::ListBoxRow::new();
        r.set_child(Some(&content));
        self.list.append(&r);
        self.rows.borrow_mut().push((r, action));
    }

    fn fa(char: &str) -> gtk::Widget {
        icon(char, "spotlight-icon").upcast()
    }

    fn draw(self: &Rc<Self>) {
        while let Some(r) = self.list.first_child() {
            self.list.remove(&r);
        }
        self.rows.borrow_mut().clear();
        let query = self.search.text().trim().to_string();
        if query.is_empty() {
            self.scroll.set_visible(false);
            return;
        }
        let d = normalize(&query);

        // calculator and conversions
        if let Some(x) = calculator::compute(&query) {
            let v = calculator::format(x, query.contains(','));
            let to_copy = v.clone();
            self.row(Self::fa("\u{f1ec}"), &format!("= {v}"), "Calculator · Enter copies the result", Rc::new(move |_| copy(&to_copy)));
        } else if let Some(p) = units::convert(&query) {
            let to_copy = p.result.clone();
            self.row(Self::fa("\u{f362}"), &p.title, &p.description, Rc::new(move |_| {
                if !to_copy.is_empty() {
                    copy(&to_copy)
                }
            }));
            if p.ceka_on_rates {
                in_background(units::download_rates, |ok| {
                    if let (true, Some(s)) = (ok, spotlight()) {
                        s.draw();
                    }
                });
            }
        }

        // apps: match at the start of the name > at the start of a word > anywhere > in the description; often started ones higher
        let usage = usage();
        let mut apps: Vec<(i64, gio::AppInfo)> = self
            .apps
            .borrow()
            .iter()
            .filter_map(|(a, name, ostatni)| {
                let shoda = if name.starts_with(&d) {
                    300
                } else if name.split(|c: char| !c.is_alphanumeric()).any(|s| s.starts_with(&d)) {
                    200
                } else if name.contains(&d) {
                    120
                } else if d.split_whitespace().all(|s| name.contains(s) || ostatni.contains(s)) {
                    40
                } else {
                    return None;
                };
                let id = a.id().map(|i| i.to_string()).unwrap_or_default();
                let count = usage[&id][0].as_u64().unwrap_or(0) as f64;
                let nedavno = now().saturating_sub(usage[&id][1].as_u64().unwrap_or(0)) < 7 * 86400;
                Some((shoda + (30.0 * (1.0 + count).ln()) as i64 + if nedavno { 20 } else { 0 }, a.clone()))
            })
            .collect();
        apps.sort_by(|a, b| b.0.cmp(&a.0));
        if !apps.is_empty() {
            self.title("Apps");
        }
        for (_, a) in apps.into_iter().take(MAX_APPS) {
            let image = match a.icon() {
                Some(i) => gtk::Image::from_gicon(&i),
                None => gtk::Image::from_icon_name("application-x-executable"),
            };
            image.set_pixel_size(ICON);
            let description = a.description().map(|d| d.to_string()).unwrap_or_default();
            let a2 = a.clone();
            self.row(image.upcast(), &a.display_name(), &description, Rc::new(move |_| {
                if let Some(id) = a2.id() {
                    write_usage(&id);
                }
                let context = gtk::gdk::Display::default().map(|d| d.app_launch_context());
                let _ = a2.launch(&[], context.as_ref());
            }));
        }

        // commands
        let commands = commands::find(&query);
        if !commands.is_empty() {
            self.title("System");
        }
        for p in commands.into_iter().take(4) {
            let action = p.action;
            self.row(Self::fa(p.icon), p.name, p.description, Rc::new(move |_| action()));
        }

        // files: recently opened and from the home folder
        let mut files: Vec<PathBuf> = Vec::new();
        if d.chars().count() >= 2 {
            let nedavne = gtk::RecentManager::default().items();
            for i in nedavne.iter().take(300) {
                let uri = i.uri();
                if !uri.starts_with("file://") {
                    continue;
                }
                let name = normalize(&i.display_name());
                if d.split_whitespace().all(|s| name.contains(s)) {
                    if let Some(p) = gio::File::for_uri(&uri).path().filter(|p| p.exists()) {
                        if !files.contains(&p) {
                            files.push(p);
                        }
                    }
                }
                if files.len() >= 3 {
                    break;
                }
            }
            let (pro_query, found) = &*self.files.borrow();
            if *pro_query == query {
                for p in found {
                    if !files.contains(p) {
                        files.push(p.clone());
                    }
                }
            }
        }
        if !files.is_empty() {
            self.title("Files");
        }
        for p in files.into_iter().take(MAX_FILES) {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let folder = p.parent().map(shorten).unwrap_or_default();
            let p2 = p.clone();
            self.row(icon_files(&p).upcast(), &name, &format!("{folder} · Ctrl+Enter shows it in the folder"), Rc::new(move |ctrl| {
                if ctrl {
                    show_ve_folder(&p2)
                } else {
                    open(&p2)
                }
            }));
        }

        // web
        self.title("Web");
        let d2 = query.clone();
        self.row(Self::fa("\u{f0ac}"), &format!("Search the web for “{query}”"), "Firefox, default search engine", Rc::new(move |_| search_on_web(&d2)));

        self.scroll.set_visible(true);
        if let Some((r, _)) = self.rows.borrow().first() {
            self.list.select_row(Some(r));
        }
        self.scroll.vadjustment().set_value(0.0);
    }

    fn move_(&self, step: i32) {
        let rows = self.rows.borrow();
        if rows.is_empty() {
            return;
        }
        let now = self.list.selected_row().and_then(|v| rows.iter().position(|(r, _)| *r == v)).unwrap_or(0) as i32;
        let new = (now + step).rem_euclid(rows.len() as i32) as usize;
        let r = &rows[new].0;
        self.list.select_row(Some(r));
        // keep the selected row visible
        if let Some(b) = r.compute_bounds(&self.list) {
            let adj = self.scroll.vadjustment();
            let (y, v) = (b.y() as f64, b.height() as f64);
            if y < adj.value() {
                adj.set_value(y - 30.0);
            } else if y + v > adj.value() + adj.page_size() {
                adj.set_value(y + v - adj.page_size());
            }
        }
    }

    fn start_selected(&self, ctrl: bool) {
        let action = {
            let rows = self.rows.borrow();
            let selected = self.list.selected_row();
            selected.and_then(|v| rows.iter().find(|(r, _)| *r == v).map(|(_, a)| a.clone())).or_else(|| rows.first().map(|(_, a)| a.clone()))
        };
        if let Some(a) = action {
            hide();
            a(ctrl);
        }
    }
}
