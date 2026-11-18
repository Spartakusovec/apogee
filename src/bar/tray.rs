//! Tray (StatusNotifierItem): app icons in the bar (LocalSend, Discord, Steam…).
//! The daemon acts as the “watcher” (org.kde.StatusNotifierWatcher) that apps register with, and
//! draws the icons too. Click = Activate, middle = SecondaryActivate, right = the app's menu
//! (com.canonical.dbusmenu), scroll = Scroll.
//!
//! If another program (waybar) is the watcher, the daemon waits in the queue and takes over when it ends;
//! the apps then register again by themselves.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const INTERFACE_ITEMS: &str = "org.kde.StatusNotifierItem";
const SIZE: i32 = 18;

const WATCHER_XML: &str = r#"<node><interface name="org.place.StatusNotifierWatcher">
<method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
<method name="RegisterStatusNotifierHost"><arg type="s" direction="in"/></method>
<property name="RegisteredStatusNotifierItems" type="as" access="read"/>
<property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
<property name="ProtocolVersion" type="i" access="read"/>
<signal name="StatusNotifierItemRegistered"><arg type="s"/></signal>
<signal name="StatusNotifierItemUnregistered"><arg type="s"/></signal>
<signal name="StatusNotifierHostRegistered"/>
<signal name="StatusNotifierHostUnregistered"/>
</interface></node>"#;

struct Item {
    /// the app's unique name on the bus (:1.42)
    service: String,
    path: String,
    wrapper: gtk::Box,
    image: gtk::Image,
    menu: Option<String>,
    menu_only: bool,
    _subscription: gio::SignalSubscription,
}

thread_local! {
    static ITEMS: RefCell<Vec<Item>> = const { RefCell::new(Vec::new()) };
    static CONTAINER: RefCell<Option<gtk::Box>> = const { RefCell::new(None) };
    static BUS: RefCell<Option<gio::DBusConnection>> = const { RefCell::new(None) };
}

fn id(p: &Item) -> String {
    format!("{}{}", p.service, p.path)
}

fn bus() -> Option<gio::DBusConnection> {
    BUS.with(|s| s.borrow().clone())
}

/// The bar element with the icons (hidden while there are none) and starting the watcher.
pub fn widget() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    b.set_widget_name("tray");
    b.add_css_class("module");
    b.set_visible(false);
    CONTAINER.with(|k| k.replace(Some(b.clone())));
    if let Ok(s) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        BUS.with(|x| x.replace(Some(s.clone())));
        start_watchers(&s);
    }
    b
}

fn start_watchers(s: &gio::DBusConnection) {
    let Ok(node) = gio::DBusNodeInfo::for_xml(WATCHER_XML) else { return };
    let Some(interface) = node.lookup_interface(WATCHER) else { return };
    let registration = s
        .register_object("/StatusNotifierWatcher", &interface)
        .method_call(|_, sender, _, _, method, params, invocation| {
            if method == "RegisterStatusNotifierItem" {
                if let (Some(sender), Some((service,))) = (sender, params.get::<(String,)>()) {
                    // “/path” (libappindicator) or a bus name
                    let path = if service.starts_with('/') { service.clone() } else { "/StatusNotifierItem".into() };
                    add(sender.to_string(), path);
                }
            }
            invocation.return_value(None);
        })
        .property(|_, _, _, _, property| match property {
            "RegisteredStatusNotifierItems" => ITEMS.with(|p| p.borrow().iter().map(id).collect::<Vec<_>>()).to_variant(),
            "IsStatusNotifierHostRegistered" => true.to_variant(),
            _ => 0i32.to_variant(),
        })
        .build();
    if registration.is_err() {
        return;
    }
    // waits in the queue while waybar is the watcher
    let id_owner = gio::bus_own_name_on_connection(s, WATCHER, gio::BusNameOwnerFlags::ALLOW_REPLACEMENT | gio::BusNameOwnerFlags::REPLACE, |s, _| {
        let _ = s.emit_signal(None, "/StatusNotifierWatcher", WATCHER, "StatusNotifierHostRegistered", None);
    }, |_, _| {});
    std::mem::forget(id_owner);
    // the app ended: remove the icon
    let subscription = s.subscribe_to_signal(Some("org.freedesktop.DBus"), Some("org.freedesktop.DBus"), Some("NameOwnerChanged"), Some("/org/freedesktop/DBus"), None, gio::DBusSignalFlags::NONE, |sig| {
        if let Some((name, _, new)) = sig.parameters.get::<(String, String, String)>() {
            if new.is_empty() && name.starts_with(':') {
                remove(&name);
            }
        }
    });
    std::mem::forget(subscription);
}

fn container() -> Option<gtk::Box> {
    CONTAINER.with(|k| k.borrow().clone())
}

fn add(service: String, path: String) {
    if ITEMS.with(|p| p.borrow().iter().any(|x| x.service == service && x.path == path)) {
        return;
    }
    let (Some(s), Some(k)) = (bus(), container()) else { return };
    let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    wrapper.add_css_class("tray-item");
    let image = gtk::Image::new();
    image.set_pixel_size(SIZE);
    wrapper.append(&image);
    k.append(&wrapper);
    k.set_visible(true);

    let (sl, ce) = (service.clone(), path.clone());
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.connect_released(move |g, _, _, _| on_click(&sl, &ce, g.current_button()));
    wrapper.add_controller(click);
    let (sl, ce) = (service.clone(), path.clone());
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE);
    scroll.connect_scroll(move |_, _, dy| {
        call(&sl, &ce, "Scroll", Some((dy as i32, "vertical").to_variant()));
        glib::Propagation::Stop
    });
    wrapper.add_controller(scroll);

    // new icon, status, title… → load again
    let (sl, ce) = (service.clone(), path.clone());
    let subscription = s.subscribe_to_signal(Some(&service), Some(INTERFACE_ITEMS), None, Some(&path), None, gio::DBusSignalFlags::NONE, move |_| load(&sl, &ce));
    ITEMS.with(|p| p.borrow_mut().push(Item { service: service.clone(), path: path.clone(), wrapper, image, menu: None, menu_only: false, _subscription: subscription }));
    let _ = s.emit_signal(None, "/StatusNotifierWatcher", WATCHER, "StatusNotifierItemRegistered", Some(&(format!("{service}{path}"),).to_variant()));
    load(&service, &path);
}

fn remove(service: &str) {
    let gone: Vec<Item> = ITEMS.with(|p| {
        let mut p = p.borrow_mut();
        let (gone, remaining): (Vec<Item>, Vec<Item>) = p.drain(..).partition(|x| x.service == service);
        *p = remaining;
        gone
    });
    let Some(k) = container() else { return };
    for x in &gone {
        k.remove(&x.wrapper);
        if let Some(s) = bus() {
            let _ = s.emit_signal(None, "/StatusNotifierWatcher", WATCHER, "StatusNotifierItemUnregistered", Some(&(id(x),).to_variant()));
        }
    }
    k.set_visible(ITEMS.with(|p| !p.borrow().is_empty()));
}

fn call(service: &str, path: &str, method: &str, params: Option<Variant>) {
    if let Some(s) = bus() {
        s.call(Some(service), path, INTERFACE_ITEMS, method, params.as_ref(), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE, |_| {});
    }
}

/// Loads the item's icon, status and menu.
fn load(service: &str, path: &str) {
    let Some(s) = bus() else { return };
    let (sl, ce) = (service.to_string(), path.to_string());
    s.call(Some(service), path, "org.freedesktop.DBus.Properties", "GetAll", Some(&(INTERFACE_ITEMS,).to_variant()), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE, move |v| {
        let Some((m,)) = v.ok().and_then(|v| v.get::<(HashMap<String, Variant>,)>()) else { return };
        let text = |k: &str| m.get(k).and_then(|v| v.str().map(String::from)).unwrap_or_default();
        let state = text("Status");
        let attention = state == "NeedsAttention";
        let name_icons = if attention && !text("AttentionIconName").is_empty() { text("AttentionIconName") } else { text("IconName") };
        let path_theme = text("IconThemePath");
        let menu = m.get("Menu").and_then(|v| v.str().map(String::from)).filter(|c| c != "/");
        let menu_only = m.get("ItemIsMenu").and_then(|v| v.get::<bool>()).unwrap_or(false);
        let title = text("Title");
        let pixmaps = m.get(if attention { "AttentionIconPixmap" } else { "IconPixmap" }).or_else(|| m.get("IconPixmap")).and_then(|v| v.get::<Vec<(i32, i32, Vec<u8>)>>());
        ITEMS.with(|p| {
            let mut p = p.borrow_mut();
            let Some(x) = p.iter_mut().find(|x| x.service == sl && x.path == ce) else { return };
            x.menu = menu;
            x.menu_only = menu_only;
            x.wrapper.set_visible(state != "Passive");
            if attention {
                x.wrapper.add_css_class("needs-attention");
            } else {
                x.wrapper.remove_css_class("needs-attention");
            }
            x.wrapper.set_tooltip_text(if title.is_empty() { None } else { Some(&title) });
            if !path_theme.is_empty() {
                if let Some(d) = gtk::gdk::Display::default() {
                    let theme = gtk::IconTheme::for_display(&d);
                    if !theme.search_path().iter().any(|c| c.to_string_lossy() == path_theme) {
                        theme.add_search_path(&path_theme);
                    }
                }
            }
            if !name_icons.is_empty() && (name_icons.starts_with('/') || gtk::gdk::Display::default().is_some_and(|d| gtk::IconTheme::for_display(&d).has_icon(&name_icons))) {
                if name_icons.starts_with('/') {
                    x.image.set_from_file(Some(&name_icons));
                } else {
                    x.image.set_icon_name(Some(&name_icons));
                }
            } else if let Some(t) = pixmaps.as_deref().and_then(texture) {
                x.image.set_paintable(Some(&t));
            } else if !name_icons.is_empty() {
                x.image.set_icon_name(Some(&name_icons));
            } else {
                x.image.set_icon_name(Some("application-x-executable"));
            }
            x.image.set_pixel_size(SIZE);
        });
    });
}

/// IconPixmap (ARGB32 in network order) → texture; picks the size closest to the bar icon.
fn texture(pixmaps: &[(i32, i32, Vec<u8>)]) -> Option<gtk::gdk::MemoryTexture> {
    let (s, v, data) = pixmaps.iter().filter(|(s, v, d)| *s > 0 && *v > 0 && d.len() >= (*s * *v * 4) as usize).min_by_key(|(s, _, _)| (s - SIZE * 2).abs())?;
    let mut rgba = Vec::with_capacity(data.len());
    for px in data.chunks_exact(4) {
        rgba.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
    }
    Some(gtk::gdk::MemoryTexture::new(*s, *v, gtk::gdk::MemoryFormat::R8g8b8a8, &glib::Bytes::from_owned(rgba), (*s * 4) as usize))
}

fn on_click(service: &str, path: &str, button: u32) {
    let (menu, menu_only, wrapper) = ITEMS.with(|p| {
        p.borrow().iter().find(|x| x.service == service && x.path == path).map(|x| (x.menu.clone(), x.menu_only, x.wrapper.clone())).unwrap_or((None, false, gtk::Box::new(gtk::Orientation::Horizontal, 0)))
    });
    match button {
        1 if !menu_only => call(service, path, "Activate", Some((0i32, 0i32).to_variant())),
        2 => call(service, path, "SecondaryActivate", Some((0i32, 0i32).to_variant())),
        1 | 3 => match menu {
            Some(m) => show_menu(service, &m, &wrapper),
            None => call(service, path, "ContextMenu", Some((0i32, 0i32).to_variant())),
        },
        _ => {}
    }
}

/// The app's menu (dbusmenu) as a normal GTK menu.
fn show_menu(service: &str, path_menu: &str, wrapper: &gtk::Box) {
    let Some(s) = bus() else { return };
    let (sl, cm, wrapper) = (service.to_string(), path_menu.to_string(), wrapper.clone());
    let s2 = s.clone();
    s.call(Some(service), path_menu, "com.canonical.dbusmenu", "AboutToShow", Some(&(0i32,).to_variant()), None, gio::DBusCallFlags::NONE, 1000, gio::Cancellable::NONE, move |_| {
        let (col2, cm2) = (sl.clone(), cm.clone());
        s2.call(Some(&sl), &cm, "com.canonical.dbusmenu", "GetLayout", Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE, move |v| {
            let Ok(v) = v else { return };
            // (u, (ia{sv}av))
            let koren = v.child_value(1);
            let action = gio::SimpleActionGroup::new();
            let menu = build(&koren, &action, &col2, &cm2);
            let pop = gtk::PopoverMenu::from_model(Some(&menu));
            pop.add_css_class("tray-menu");
            pop.set_has_arrow(false);
            pop.insert_action_group("tray", Some(&action));
            pop.set_parent(&wrapper);
            pop.connect_closed(|p| {
                let p = p.clone();
                glib::idle_add_local_once(move || p.unparent());
            });
            pop.popup();
        });
    });
}

/// dbusmenu item (ia{sv}av) → gio::Menu; separators split the menu into sections.
fn build(node: &Variant, action: &gio::SimpleActionGroup, service: &str, path: &str) -> gio::Menu {
    let menu = gio::Menu::new();
    let mut section = gio::Menu::new();
    let deti = node.child_value(2);
    for i in 0..deti.n_children() {
        let dite = deti.child_value(i);
        let dite = dite.as_variant().unwrap_or(dite);
        let id: i32 = dite.child_value(0).get().unwrap_or(0);
        let properties: HashMap<String, Variant> = dite.child_value(1).get().unwrap_or_default();
        let text = |k: &str| properties.get(k).and_then(|v| v.str().map(String::from)).unwrap_or_default();
        if properties.get("visible").and_then(|v| v.get::<bool>()) == Some(false) {
            continue;
        }
        if text("type") == "separator" {
            if section.n_items() > 0 {
                menu.append_section(None, &section);
                section = gio::Menu::new();
            }
            continue;
        }
        // “_File” → “File” (an underscore marks the keyboard shortcut, “__” is an underscore)
        let label = text("label").replace("__", "\u{0}").replace('_', "").replace('\u{0}', "_");
        let name_action = format!("p{id}");
        let allowed = properties.get("enabled").and_then(|v| v.get::<bool>()).unwrap_or(true);
        if text("children-display") == "submenu" {
            section.append_submenu(Some(&label), &build(&dite, action, service, path));
            continue;
        }
        let switcher = !text("toggle-type").is_empty();
        let a = if switcher {
            let enabled = properties.get("toggle-state").and_then(|v| v.get::<i32>()).unwrap_or(0) == 1;
            gio::SimpleAction::new_stateful(&name_action, None, &enabled.to_variant())
        } else {
            gio::SimpleAction::new(&name_action, None)
        };
        a.set_enabled(allowed);
        let (sl, ce) = (service.to_string(), path.to_string());
        a.connect_activate(move |_, _| {
            if let Some(s) = bus() {
                let time = glib::monotonic_time() as u32;
                let params = (id, "clicked", Variant::from_variant(&0i32.to_variant()), time).to_variant();
                s.call(Some(&sl), &ce, "com.canonical.dbusmenu", "Event", Some(&params), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE, |_| {});
            }
        });
        action.add_action(&a);
        section.append(Some(&label), Some(&format!("tray.{name_action}")));
    }
    if section.n_items() > 0 {
        menu.append_section(None, &section);
    }
    menu
}
