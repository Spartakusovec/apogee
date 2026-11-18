//! Tray (StatusNotifierItem): ikony aplikací v liště (LocalSend, Discord, Steam…).
//! Démon dělá „hlídače“ (org.kde.StatusNotifierWatcher), u kterého se aplikace hlásí, a ikony
//! zároveň kreslí. Klik = Activate, prostřední = SecondaryActivate, pravý = nabídka aplikace
//! (com.canonical.dbusmenu), kolečko = Scroll.
//!
//! Když hlídače dělá jiný program (waybar), démon čeká ve frontě a převezme to, až skončí;
//! aplikace se pak samy přihlásí znovu.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;

const HLIDAC: &str = "org.kde.StatusNotifierWatcher";
const ROZHRANI_POLOZKY: &str = "org.kde.StatusNotifierItem";
const VELIKOST: i32 = 18;

const HLIDAC_XML: &str = r#"<node><interface name="org.kde.StatusNotifierWatcher">
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

struct Polozka {
    /// unikátní název aplikace na sběrnici (:1.42)
    sluzba: String,
    cesta: String,
    obal: gtk::Box,
    obrazek: gtk::Image,
    menu: Option<String>,
    jen_menu: bool,
    _odber: gio::SignalSubscription,
}

thread_local! {
    static POLOZKY: RefCell<Vec<Polozka>> = const { RefCell::new(Vec::new()) };
    static KONTEJNER: RefCell<Option<gtk::Box>> = const { RefCell::new(None) };
    static SBERNICE: RefCell<Option<gio::DBusConnection>> = const { RefCell::new(None) };
}

fn id(p: &Polozka) -> String {
    format!("{}{}", p.sluzba, p.cesta)
}

fn sbernice() -> Option<gio::DBusConnection> {
    SBERNICE.with(|s| s.borrow().clone())
}

/// Prvek lišty s ikonami (schovaný, dokud žádná není) a spuštění hlídače.
pub fn widget() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    b.set_widget_name("tray");
    b.add_css_class("modul");
    b.set_visible(false);
    KONTEJNER.with(|k| k.replace(Some(b.clone())));
    if let Ok(s) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        SBERNICE.with(|x| x.replace(Some(s.clone())));
        spustit_hlidace(&s);
    }
    b
}

fn spustit_hlidace(s: &gio::DBusConnection) {
    let Ok(uzel) = gio::DBusNodeInfo::for_xml(HLIDAC_XML) else { return };
    let Some(rozhrani) = uzel.lookup_interface(HLIDAC) else { return };
    let registrace = s
        .register_object("/StatusNotifierWatcher", &rozhrani)
        .method_call(|_, odesilatel, _, _, metoda, parametry, volani| {
            if metoda == "RegisterStatusNotifierItem" {
                if let (Some(odesilatel), Some((sluzba,))) = (odesilatel, parametry.get::<(String,)>()) {
                    // „/cesta“ (libappindicator) nebo název na sběrnici
                    let cesta = if sluzba.starts_with('/') { sluzba.clone() } else { "/StatusNotifierItem".into() };
                    pridat(odesilatel.to_string(), cesta);
                }
            }
            volani.return_value(None);
        })
        .property(|_, _, _, _, vlastnost| match vlastnost {
            "RegisteredStatusNotifierItems" => POLOZKY.with(|p| p.borrow().iter().map(id).collect::<Vec<_>>()).to_variant(),
            "IsStatusNotifierHostRegistered" => true.to_variant(),
            _ => 0i32.to_variant(),
        })
        .build();
    if registrace.is_err() {
        return;
    }
    // čeká ve frontě, když hlídače zatím dělá waybar
    let id_vlastnika = gio::bus_own_name_on_connection(s, HLIDAC, gio::BusNameOwnerFlags::ALLOW_REPLACEMENT | gio::BusNameOwnerFlags::REPLACE, |s, _| {
        let _ = s.emit_signal(None, "/StatusNotifierWatcher", HLIDAC, "StatusNotifierHostRegistered", None);
    }, |_, _| {});
    std::mem::forget(id_vlastnika);
    // aplikace skončila: ikonu pryč
    let odber = s.subscribe_to_signal(Some("org.freedesktop.DBus"), Some("org.freedesktop.DBus"), Some("NameOwnerChanged"), Some("/org/freedesktop/DBus"), None, gio::DBusSignalFlags::NONE, |sig| {
        if let Some((jmeno, _, novy)) = sig.parameters.get::<(String, String, String)>() {
            if novy.is_empty() && jmeno.starts_with(':') {
                odebrat(&jmeno);
            }
        }
    });
    std::mem::forget(odber);
}

fn kontejner() -> Option<gtk::Box> {
    KONTEJNER.with(|k| k.borrow().clone())
}

fn pridat(sluzba: String, cesta: String) {
    if POLOZKY.with(|p| p.borrow().iter().any(|x| x.sluzba == sluzba && x.cesta == cesta)) {
        return;
    }
    let (Some(s), Some(k)) = (sbernice(), kontejner()) else { return };
    let obal = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    obal.add_css_class("tray-polozka");
    let obrazek = gtk::Image::new();
    obrazek.set_pixel_size(VELIKOST);
    obal.append(&obrazek);
    k.append(&obal);
    k.set_visible(true);

    let (sl, ce) = (sluzba.clone(), cesta.clone());
    let klik = gtk::GestureClick::new();
    klik.set_button(0);
    klik.connect_released(move |g, _, _, _| kliknuti(&sl, &ce, g.current_button()));
    obal.add_controller(klik);
    let (sl, ce) = (sluzba.clone(), cesta.clone());
    let kolecko = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE);
    kolecko.connect_scroll(move |_, _, dy| {
        volat(&sl, &ce, "Scroll", Some((dy as i32, "vertical").to_variant()));
        glib::Propagation::Stop
    });
    obal.add_controller(kolecko);

    // nová ikona, stav, titulek… → načíst znovu
    let (sl, ce) = (sluzba.clone(), cesta.clone());
    let odber = s.subscribe_to_signal(Some(&sluzba), Some(ROZHRANI_POLOZKY), None, Some(&cesta), None, gio::DBusSignalFlags::NONE, move |_| nacist(&sl, &ce));
    POLOZKY.with(|p| p.borrow_mut().push(Polozka { sluzba: sluzba.clone(), cesta: cesta.clone(), obal, obrazek, menu: None, jen_menu: false, _odber: odber }));
    let _ = s.emit_signal(None, "/StatusNotifierWatcher", HLIDAC, "StatusNotifierItemRegistered", Some(&(format!("{sluzba}{cesta}"),).to_variant()));
    nacist(&sluzba, &cesta);
}

fn odebrat(sluzba: &str) {
    let pryc: Vec<Polozka> = POLOZKY.with(|p| {
        let mut p = p.borrow_mut();
        let (pryc, zbyva): (Vec<Polozka>, Vec<Polozka>) = p.drain(..).partition(|x| x.sluzba == sluzba);
        *p = zbyva;
        pryc
    });
    let Some(k) = kontejner() else { return };
    for x in &pryc {
        k.remove(&x.obal);
        if let Some(s) = sbernice() {
            let _ = s.emit_signal(None, "/StatusNotifierWatcher", HLIDAC, "StatusNotifierItemUnregistered", Some(&(id(x),).to_variant()));
        }
    }
    k.set_visible(POLOZKY.with(|p| !p.borrow().is_empty()));
}

fn volat(sluzba: &str, cesta: &str, metoda: &str, parametry: Option<Variant>) {
    if let Some(s) = sbernice() {
        s.call(Some(sluzba), cesta, ROZHRANI_POLOZKY, metoda, parametry.as_ref(), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE, |_| {});
    }
}

/// Načte ikonu, stav a nabídku položky.
fn nacist(sluzba: &str, cesta: &str) {
    let Some(s) = sbernice() else { return };
    let (sl, ce) = (sluzba.to_string(), cesta.to_string());
    s.call(Some(sluzba), cesta, "org.freedesktop.DBus.Properties", "GetAll", Some(&(ROZHRANI_POLOZKY,).to_variant()), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE, move |v| {
        let Some((m,)) = v.ok().and_then(|v| v.get::<(HashMap<String, Variant>,)>()) else { return };
        let text = |k: &str| m.get(k).and_then(|v| v.str().map(String::from)).unwrap_or_default();
        let stav = text("Status");
        let pozornost = stav == "NeedsAttention";
        let jmeno_ikony = if pozornost && !text("AttentionIconName").is_empty() { text("AttentionIconName") } else { text("IconName") };
        let cesta_motivu = text("IconThemePath");
        let menu = m.get("Menu").and_then(|v| v.str().map(String::from)).filter(|c| c != "/");
        let jen_menu = m.get("ItemIsMenu").and_then(|v| v.get::<bool>()).unwrap_or(false);
        let titulek = text("Title");
        let pixmapy = m.get(if pozornost { "AttentionIconPixmap" } else { "IconPixmap" }).or_else(|| m.get("IconPixmap")).and_then(|v| v.get::<Vec<(i32, i32, Vec<u8>)>>());
        POLOZKY.with(|p| {
            let mut p = p.borrow_mut();
            let Some(x) = p.iter_mut().find(|x| x.sluzba == sl && x.cesta == ce) else { return };
            x.menu = menu;
            x.jen_menu = jen_menu;
            x.obal.set_visible(stav != "Passive");
            if pozornost {
                x.obal.add_css_class("needs-attention");
            } else {
                x.obal.remove_css_class("needs-attention");
            }
            x.obal.set_tooltip_text(if titulek.is_empty() { None } else { Some(&titulek) });
            if !cesta_motivu.is_empty() {
                if let Some(d) = gtk::gdk::Display::default() {
                    let motiv = gtk::IconTheme::for_display(&d);
                    if !motiv.search_path().iter().any(|c| c.to_string_lossy() == cesta_motivu) {
                        motiv.add_search_path(&cesta_motivu);
                    }
                }
            }
            if !jmeno_ikony.is_empty() && (jmeno_ikony.starts_with('/') || gtk::gdk::Display::default().is_some_and(|d| gtk::IconTheme::for_display(&d).has_icon(&jmeno_ikony))) {
                if jmeno_ikony.starts_with('/') {
                    x.obrazek.set_from_file(Some(&jmeno_ikony));
                } else {
                    x.obrazek.set_icon_name(Some(&jmeno_ikony));
                }
            } else if let Some(t) = pixmapy.as_deref().and_then(textura) {
                x.obrazek.set_paintable(Some(&t));
            } else if !jmeno_ikony.is_empty() {
                x.obrazek.set_icon_name(Some(&jmeno_ikony));
            } else {
                x.obrazek.set_icon_name(Some("application-x-executable"));
            }
            x.obrazek.set_pixel_size(VELIKOST);
        });
    });
}

/// IconPixmap (ARGB32 v pořadí sítě) → textura; vybere velikost nejblíž ikoně v liště.
fn textura(pixmapy: &[(i32, i32, Vec<u8>)]) -> Option<gtk::gdk::MemoryTexture> {
    let (s, v, data) = pixmapy.iter().filter(|(s, v, d)| *s > 0 && *v > 0 && d.len() >= (*s * *v * 4) as usize).min_by_key(|(s, _, _)| (s - VELIKOST * 2).abs())?;
    let mut rgba = Vec::with_capacity(data.len());
    for px in data.chunks_exact(4) {
        rgba.extend_from_slice(&[px[1], px[2], px[3], px[0]]);
    }
    Some(gtk::gdk::MemoryTexture::new(*s, *v, gtk::gdk::MemoryFormat::R8g8b8a8, &glib::Bytes::from_owned(rgba), (*s * 4) as usize))
}

fn kliknuti(sluzba: &str, cesta: &str, tlacitko: u32) {
    let (menu, jen_menu, obal) = POLOZKY.with(|p| {
        p.borrow().iter().find(|x| x.sluzba == sluzba && x.cesta == cesta).map(|x| (x.menu.clone(), x.jen_menu, x.obal.clone())).unwrap_or((None, false, gtk::Box::new(gtk::Orientation::Horizontal, 0)))
    });
    match tlacitko {
        1 if !jen_menu => volat(sluzba, cesta, "Activate", Some((0i32, 0i32).to_variant())),
        2 => volat(sluzba, cesta, "SecondaryActivate", Some((0i32, 0i32).to_variant())),
        1 | 3 => match menu {
            Some(m) => ukazat_menu(sluzba, &m, &obal),
            None => volat(sluzba, cesta, "ContextMenu", Some((0i32, 0i32).to_variant())),
        },
        _ => {}
    }
}

/// Nabídka aplikace (dbusmenu) jako běžná nabídka GTK.
fn ukazat_menu(sluzba: &str, cesta_menu: &str, obal: &gtk::Box) {
    let Some(s) = sbernice() else { return };
    let (sl, cm, obal) = (sluzba.to_string(), cesta_menu.to_string(), obal.clone());
    let s2 = s.clone();
    s.call(Some(sluzba), cesta_menu, "com.canonical.dbusmenu", "AboutToShow", Some(&(0i32,).to_variant()), None, gio::DBusCallFlags::NONE, 1000, gio::Cancellable::NONE, move |_| {
        let (sl2, cm2) = (sl.clone(), cm.clone());
        s2.call(Some(&sl), &cm, "com.canonical.dbusmenu", "GetLayout", Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE, move |v| {
            let Ok(v) = v else { return };
            // (u, (ia{sv}av))
            let koren = v.child_value(1);
            let akce = gio::SimpleActionGroup::new();
            let menu = sestavit(&koren, &akce, &sl2, &cm2);
            let pop = gtk::PopoverMenu::from_model(Some(&menu));
            pop.add_css_class("tray-nabidka");
            pop.set_has_arrow(false);
            pop.insert_action_group("tray", Some(&akce));
            pop.set_parent(&obal);
            pop.connect_closed(|p| {
                let p = p.clone();
                glib::idle_add_local_once(move || p.unparent());
            });
            pop.popup();
        });
    });
}

/// Položka dbusmenu (ia{sv}av) → gio::Menu; oddělovače dělí nabídku na sekce.
fn sestavit(uzel: &Variant, akce: &gio::SimpleActionGroup, sluzba: &str, cesta: &str) -> gio::Menu {
    let menu = gio::Menu::new();
    let mut sekce = gio::Menu::new();
    let deti = uzel.child_value(2);
    for i in 0..deti.n_children() {
        let dite = deti.child_value(i);
        let dite = dite.as_variant().unwrap_or(dite);
        let id: i32 = dite.child_value(0).get().unwrap_or(0);
        let vlastnosti: HashMap<String, Variant> = dite.child_value(1).get().unwrap_or_default();
        let text = |k: &str| vlastnosti.get(k).and_then(|v| v.str().map(String::from)).unwrap_or_default();
        if vlastnosti.get("visible").and_then(|v| v.get::<bool>()) == Some(false) {
            continue;
        }
        if text("type") == "separator" {
            if sekce.n_items() > 0 {
                menu.append_section(None, &sekce);
                sekce = gio::Menu::new();
            }
            continue;
        }
        // „_Soubor“ → „Soubor“ (podtržítko označuje klávesovou zkratku, „__“ je podtržítko)
        let popisek = text("label").replace("__", "\u{0}").replace('_', "").replace('\u{0}', "_");
        let nazev_akce = format!("p{id}");
        let povoleno = vlastnosti.get("enabled").and_then(|v| v.get::<bool>()).unwrap_or(true);
        if text("children-display") == "submenu" {
            sekce.append_submenu(Some(&popisek), &sestavit(&dite, akce, sluzba, cesta));
            continue;
        }
        let prepinac = !text("toggle-type").is_empty();
        let a = if prepinac {
            let zapnuto = vlastnosti.get("toggle-state").and_then(|v| v.get::<i32>()).unwrap_or(0) == 1;
            gio::SimpleAction::new_stateful(&nazev_akce, None, &zapnuto.to_variant())
        } else {
            gio::SimpleAction::new(&nazev_akce, None)
        };
        a.set_enabled(povoleno);
        let (sl, ce) = (sluzba.to_string(), cesta.to_string());
        a.connect_activate(move |_, _| {
            if let Some(s) = sbernice() {
                let cas = glib::monotonic_time() as u32;
                let parametry = (id, "clicked", Variant::from_variant(&0i32.to_variant()), cas).to_variant();
                s.call(Some(&sl), &ce, "com.canonical.dbusmenu", "Event", Some(&parametry), None, gio::DBusCallFlags::NONE, 2000, gio::Cancellable::NONE, |_| {});
            }
        });
        akce.add_action(&a);
        sekce.append(Some(&popisek), Some(&format!("tray.{nazev_akce}")));
    }
    if sekce.n_items() > 0 {
        menu.append_section(None, &sekce);
    }
    menu
}
