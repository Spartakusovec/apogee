//! Spotlight (Super+mezerník): jedno pole nahoře uprostřed obrazovky a pod ním výsledky.
//!
//!   aplikace      – podle názvu, popisu a názvu programu; často spouštěné výš
//!   kalkulačka    – „2+3*4“, „200 + 10%“, „sqrt(2)“; Enter zkopíruje výsledek
//!   převody       – „10 km to mi“, „100 eur na kč“ (kurzy ECB)
//!   příkazy       – „lock“, „restart“, „wifi“, „tapeta“… (anglicky i česky)
//!   soubory       – naposledy otevřené a ze složky domov (plocate); Ctrl+Enter ukáže soubor ve složce.
//!                   Vlastní index domova (bez skrytých složek, ~/.cache/apogee/files.db) se obnoví
//!                   při otevření, když je starší než 2 minuty (systémový se obnovuje jen jednou denně).
//!   web           – poslední řádek: hledat na webu (Firefox, výchozí vyhledávač)
//!
//! Šipky vybírají, Enter spustí, Esc nebo klik mimo zavře. Okno se sestaví jednou a pak se jen
//! ukazuje, takže je hned na obrazovce.

mod jednotky;
mod kalkulacka;
mod prikazy;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::json;

use crate::spolecne::*;

const IKONA: i32 = 38;
const NEJVIC_APLIKACI: usize = 6;
const NEJVIC_SOUBORU: usize = 6;

/// Co udělá řádek (true = s Ctrl).
type Akce = Rc<dyn Fn(bool)>;

struct Spotlight {
    okno: gtk::Window,
    panel: gtk::Box,
    hledat: gtk::Entry,
    posuv: gtk::ScrolledWindow,
    seznam: gtk::ListBox,
    /// vybratelné řádky a co dělají
    radky: RefCell<Vec<(gtk::ListBoxRow, Akce)>>,
    aplikace: RefCell<Vec<(gio::AppInfo, String, String)>>,
    /// výsledky hledání souborů k dotazu (dotaz, cesty)
    soubory: RefCell<(String, Vec<PathBuf>)>,
    hledani_souboru: RefCell<Option<glib::SourceId>>,
}

thread_local! {
    static SPOTLIGHT: RefCell<Option<Rc<Spotlight>>> = const { RefCell::new(None) };
}

pub fn bez_diakritiky(text: &str) -> String {
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

fn normalizovat(text: &str) -> String {
    bez_diakritiky(&text.to_lowercase())
}

fn ted() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn spotlight() -> Option<Rc<Spotlight>> {
    SPOTLIGHT.with(|s| s.borrow().clone())
}

/// Sestaví okno chvíli po startu démona a jednou ho neviditelně vykreslí.
pub fn pripravit() {
    glib::timeout_add_local_once(Duration::from_secs(4), || {
        let s = ziskat();
        s.okno.set_opacity(0.01);
        s.okno.set_visible(true);
        glib::timeout_add_local_once(Duration::from_millis(300), || {
            if let Some(s) = spotlight() {
                s.okno.set_visible(false);
                s.okno.set_opacity(1.0);
            }
        });
    });
}

fn ziskat() -> Rc<Spotlight> {
    if let Some(s) = spotlight() {
        return s;
    }
    crate::styl::pouzit();
    let okno = gtk::Window::new();
    okno.add_css_class("apogee");
    okno.add_css_class("spotlight");
    okno.init_layer_shell();
    okno.set_layer(Layer::Overlay);
    okno.set_namespace(Some("spotlight"));
    okno.set_exclusive_zone(-1);
    for k in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        okno.set_anchor(k, true);
    }
    okno.set_keyboard_mode(KeyboardMode::Exclusive);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_widget_name("panel");
    panel.set_halign(gtk::Align::Center);
    panel.set_valign(gtk::Align::Start);
    panel.set_margin_top(150);
    panel.set_size_request(780, -1);
    let pole = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    pole.add_css_class("spotlight-pole");
    pole.append(&ikona("\u{f002}", "spotlight-lupa"));
    let hledat = gtk::Entry::new();
    hledat.set_placeholder_text(Some("Search apps, files, calculate, convert…"));
    hledat.set_hexpand(true);
    pole.append(&hledat);
    panel.append(&pole);
    let seznam = gtk::ListBox::new();
    seznam.add_css_class("spotlight-vysledky");
    seznam.set_selection_mode(gtk::SelectionMode::Single);
    let posuv = gtk::ScrolledWindow::new();
    posuv.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    posuv.set_propagate_natural_height(true);
    posuv.set_max_content_height(600);
    posuv.set_child(Some(&seznam));
    posuv.set_visible(false);
    panel.append(&posuv);
    let pozadi = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pozadi.set_hexpand(true);
    pozadi.set_vexpand(true);
    pozadi.append(&panel);
    okno.set_child(Some(&pozadi));

    let s = Rc::new(Spotlight {
        okno: okno.clone(),
        panel,
        hledat: hledat.clone(),
        posuv,
        seznam: seznam.clone(),
        radky: RefCell::new(Vec::new()),
        aplikace: RefCell::new(Vec::new()),
        soubory: RefCell::new((String::new(), Vec::new())),
        hledani_souboru: RefCell::new(None),
    });

    hledat.connect_changed(|_| {
        if let Some(s) = spotlight() {
            s.vykreslit();
            s.naplanovat_hledani_souboru();
        }
    });
    // klávesy dřív, než je dostane pole: šipky vybírají, Enter spustí, Esc zavře
    let klavesy = gtk::EventControllerKey::new();
    klavesy.set_propagation_phase(gtk::PropagationPhase::Capture);
    klavesy.connect_key_pressed(|_, k, _, mod_| {
        use gtk::gdk::Key;
        let Some(s) = spotlight() else { return glib::Propagation::Proceed };
        match k {
            Key::Escape => schovat(),
            Key::Down | Key::Tab => s.posunout(1),
            Key::Up | Key::ISO_Left_Tab => s.posunout(-1),
            Key::Return | Key::KP_Enter => s.spustit_vybrany(mod_.contains(gtk::gdk::ModifierType::CONTROL_MASK)),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    okno.add_controller(klavesy);
    seznam.connect_row_activated(|_, r| {
        if let Some(s) = spotlight() {
            s.seznam.select_row(Some(r));
            s.spustit_vybrany(false);
        }
    });
    // klik mimo panel zavře
    let klik = gtk::GestureClick::new();
    klik.connect_released(|g, _, x, y| {
        if let Some(s) = spotlight() {
            let uvnitr = s.okno.pick(x, y, gtk::PickFlags::DEFAULT).is_some_and(|w| w == *s.panel.upcast_ref::<gtk::Widget>() || w.is_ancestor(&s.panel));
            if !uvnitr {
                g.set_state(gtk::EventSequenceState::Claimed);
                schovat();
            }
        }
    });
    okno.add_controller(klik);

    SPOTLIGHT.with(|x| x.replace(Some(s.clone())));
    s.nacist_aplikace();
    s
}

/// Otevřít / zavřít; s textem otevře rovnou s tímhle hledáním.
pub fn prepnout(text: &str) {
    let s = ziskat();
    if s.okno.is_visible() && text.is_empty() {
        schovat();
        return;
    }
    crate::okenka::zavrit();
    s.nacist_aplikace();
    obnovit_index();
    s.hledat.set_text(text);
    s.vykreslit();
    s.okno.set_visible(true);
    // kurzor na konec, text neoznačovat (ať se dá rovnou psát dál)
    s.hledat.grab_focus_without_selecting();
    s.hledat.set_position(-1);
    // kurzy měn na pozadí, ať jsou po ruce
    if jednotky::kurzy_zastarale() {
        na_pozadi(jednotky::stahnout_kurzy, |_| {});
    }
}

fn schovat() {
    if let Some(s) = spotlight() {
        s.okno.set_visible(false);
        if let Some(id) = s.hledani_souboru.take() {
            id.remove();
        }
    }
}

/// Kolikrát a kdy naposledy byla aplikace spuštěná ze spotlightu: {"id": [počet, čas]}.
fn pouzivani() -> serde_json::Value {
    nastaveni("spotlight")["pouziti"].clone()
}

fn zapsat_pouziti(id: &str) {
    let mut v = nastaveni("spotlight");
    if !v.is_object() {
        v = json!({});
    }
    let pocet = v["pouziti"][id][0].as_u64().unwrap_or(0) + 1;
    if !v["pouziti"].is_object() {
        v["pouziti"] = json!({});
    }
    v["pouziti"][id] = json!([pocet, ted()]);
    ulozit_nastaveni("spotlight", v);
}

/// Ikona souboru podle typu.
fn ikona_souboru(cesta: &Path) -> gtk::Image {
    let ikona = if cesta.is_dir() {
        gio::ThemedIcon::new("folder").upcast::<gio::Icon>()
    } else {
        let (typ, _) = gio::content_type_guess(Some(cesta), None);
        gio::content_type_get_icon(&typ)
    };
    let i = gtk::Image::from_gicon(&ikona);
    i.set_pixel_size(IKONA);
    i
}

fn zkratit(cesta: &Path) -> String {
    let d = domov();
    match cesta.strip_prefix(&d) {
        Ok(zbytek) => format!("~/{}", zbytek.display()),
        Err(_) => cesta.display().to_string(),
    }
}

/// Ukáže soubor ve správci souborů (Ctrl+Enter).
fn ukazat_ve_slozce(cesta: &Path) {
    let uri = gio::File::for_path(cesta).uri().to_string();
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

fn otevrit(cesta: &Path) {
    let uri = gio::File::for_path(cesta).uri();
    let kontext = gtk::gdk::Display::default().map(|d| d.app_launch_context());
    let _ = gio::AppInfo::launch_default_for_uri(&uri, kontext.as_ref());
}

fn kopirovat(text: &str) {
    if let Some(d) = gtk::gdk::Display::default() {
        d.clipboard().set_text(text);
    }
    // schránka Waylandu patří oknu, které ji nastavilo: ať přežije zavření spotlightu
    pozadi("wl-copy", &[text]);
}

fn hledat_na_webu(dotaz: &str) {
    let firefox = gio::AppInfo::all().into_iter().any(|a| a.id().is_some_and(|i| i == "org.mozilla.firefox.desktop"));
    if firefox {
        pozadi("flatpak", &["run", "org.mozilla.firefox", "--search", dotaz]);
    } else {
        let uri = format!("https://duckduckgo.com/?q={}", glib::Uri::escape_string(dotaz, None, false));
        pozadi("xdg-open", &[&uri]);
    }
}

fn index_souboru() -> PathBuf {
    domov().join(".cache/apogee/files.db")
}

thread_local! {
    static INDEXUJE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Obnoví index domova na pozadí (nejvýš jednou za 2 minuty); pak zopakuje rozepsané hledání,
/// ať se ukáže i soubor stažený před chvílí.
fn obnovit_index() {
    let db = index_souboru();
    let cerstvy = std::fs::metadata(&db)
        .and_then(|m| m.modified())
        .is_ok_and(|t| t.elapsed().is_ok_and(|e| e < Duration::from_secs(120)));
    if cerstvy || INDEXUJE.with(|i| i.replace(true)) {
        return;
    }
    na_pozadi(
        move || {
            let domov = domov();
            // skryté složky Spotlight neukazuje, nemusí ani do indexu
            let skryte: Vec<String> = std::fs::read_dir(&domov)
                .map(|d| {
                    d.flatten()
                        .filter(|e| e.file_name().to_string_lossy().starts_with('.') && e.file_type().is_ok_and(|t| t.is_dir()))
                        .map(|e| e.path().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let _ = std::fs::create_dir_all(db.parent().unwrap_or(&domov));
            let (domov, db) = (domov.to_string_lossy().to_string(), db.to_string_lossy().to_string());
            prikaz("nice", &["-n", "19", "ionice", "-c", "3", "updatedb", "-l", "0", "-U", &domov, "-o", &db, "-e", &skryte.join(" ")]);
        },
        |_| {
            INDEXUJE.with(|i| i.set(false));
            if let Some(s) = spotlight() {
                if s.okno.is_visible() {
                    s.naplanovat_hledani_souboru();
                }
            }
        },
    );
}

/// Soubory z domovské složky (plocate) a naposledy otevřené; pomalé, volat mimo GTK.
fn najit_soubory(dotaz: &str) -> Vec<PathBuf> {
    let slova: Vec<String> = dotaz.split_whitespace().map(str::to_string).collect();
    if slova.is_empty() || dotaz.chars().count() < 2 {
        return Vec::new();
    }
    let domov = domov();
    let mut args = vec!["-i".to_string(), "-b".into(), "-l".into(), "400".into()];
    // vlastní čerstvý index domova, jinak systémový
    let db = index_souboru();
    if db.exists() {
        args.extend(["-d".to_string(), db.to_string_lossy().to_string()]);
    }
    args.push("--".into());
    args.extend(slova.iter().cloned());
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let vynechat = ["node_modules", "target", "__pycache__", "site-packages", "venv"];
    let mut nalezene: Vec<PathBuf> = prikaz("plocate", &args)
        .lines()
        .map(PathBuf::from)
        .filter(|p| p.starts_with(&domov))
        .filter(|p| {
            p.strip_prefix(&domov).map_or(false, |z| {
                z.components().all(|c| {
                    let c = c.as_os_str().to_string_lossy();
                    !c.starts_with('.') && !vynechat.contains(&c.as_ref())
                })
            })
        })
        .filter(|p| p.exists())
        .collect();
    // názvy začínající hledaným textem první, pak kratší cesty
    let prvni = normalizovat(&slova[0]);
    nalezene.sort_by_key(|p| {
        let jmeno = normalizovat(&p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        (!jmeno.starts_with(&prvni), p.components().count(), jmeno.len())
    });
    nalezene.truncate(NEJVIC_SOUBORU * 2);
    nalezene
}

impl Spotlight {
    fn nacist_aplikace(&self) {
        let mut v: Vec<(gio::AppInfo, String, String)> = gio::AppInfo::all()
            .into_iter()
            .filter(|a| a.should_show())
            .map(|a| {
                let jmeno = normalizovat(&a.display_name());
                let ostatni = normalizovat(&format!("{} {}", a.description().unwrap_or_default(), a.executable().to_string_lossy()));
                (a, jmeno, ostatni)
            })
            .collect();
        v.sort_by(|a, b| a.1.cmp(&b.1));
        self.aplikace.replace(v);
    }

    fn naplanovat_hledani_souboru(&self) {
        if let Some(id) = self.hledani_souboru.take() {
            id.remove();
        }
        let dotaz = self.hledat.text().trim().to_string();
        if dotaz.chars().count() < 2 || kalkulacka::spocitat(&dotaz).is_some() || jednotky::prevest(&dotaz).is_some() {
            return;
        }
        // až se chvilku nepíše
        let id = glib::timeout_add_local_once(Duration::from_millis(120), move || {
            if let Some(s) = spotlight() {
                s.hledani_souboru.replace(None);
            }
            let d = dotaz.clone();
            na_pozadi(
                move || najit_soubory(&d),
                move |nalezene| {
                    if let Some(s) = spotlight() {
                        if s.hledat.text().trim() == dotaz && s.okno.is_visible() {
                            s.soubory.replace((dotaz, nalezene));
                            s.vykreslit();
                        }
                    }
                },
            );
        });
        self.hledani_souboru.replace(Some(id));
    }

    /// Nadpis skupiny výsledků (nedá se vybrat).
    fn nadpis(&self, text: &str) {
        let r = gtk::ListBoxRow::new();
        r.set_selectable(false);
        r.set_activatable(false);
        r.set_can_focus(false);
        r.add_css_class("skupina");
        r.set_child(Some(&popisek(text, &["dim"])));
        self.seznam.append(&r);
    }

    fn radek(&self, ikona_widget: gtk::Widget, nazev: &str, popis: &str, akce: Akce) {
        let obsah = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        ikona_widget.set_size_request(IKONA, IKONA);
        ikona_widget.set_valign(gtk::Align::Center);
        obsah.append(&ikona_widget);
        let texty = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texty.set_valign(gtk::Align::Center);
        texty.set_hexpand(true);
        let n = popisek(nazev, &["vysledek"]);
        n.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texty.append(&n);
        if !popis.is_empty() {
            let p = popisek(popis, &["dim"]);
            p.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            texty.append(&p);
        }
        obsah.append(&texty);
        let r = gtk::ListBoxRow::new();
        r.set_child(Some(&obsah));
        self.seznam.append(&r);
        self.radky.borrow_mut().push((r, akce));
    }

    fn fa(znak: &str) -> gtk::Widget {
        ikona(znak, "spotlight-ikona").upcast()
    }

    fn vykreslit(self: &Rc<Self>) {
        while let Some(r) = self.seznam.first_child() {
            self.seznam.remove(&r);
        }
        self.radky.borrow_mut().clear();
        let dotaz = self.hledat.text().trim().to_string();
        if dotaz.is_empty() {
            self.posuv.set_visible(false);
            return;
        }
        let d = normalizovat(&dotaz);

        // kalkulačka a převody
        if let Some(x) = kalkulacka::spocitat(&dotaz) {
            let v = kalkulacka::formatovat(x, dotaz.contains(','));
            let kopie = v.clone();
            self.radek(Self::fa("\u{f1ec}"), &format!("= {v}"), "Calculator · Enter copies the result", Rc::new(move |_| kopirovat(&kopie)));
        } else if let Some(p) = jednotky::prevest(&dotaz) {
            let kopie = p.vysledek.clone();
            self.radek(Self::fa("\u{f362}"), &p.nadpis, &p.popis, Rc::new(move |_| {
                if !kopie.is_empty() {
                    kopirovat(&kopie)
                }
            }));
            if p.ceka_na_kurzy {
                na_pozadi(jednotky::stahnout_kurzy, |ok| {
                    if let (true, Some(s)) = (ok, spotlight()) {
                        s.vykreslit();
                    }
                });
            }
        }

        // aplikace: shoda na začátku názvu > na začátku slova > kdekoli > v popisu; často spouštěné výš
        let pouziti = pouzivani();
        let mut aplikace: Vec<(i64, gio::AppInfo)> = self
            .aplikace
            .borrow()
            .iter()
            .filter_map(|(a, jmeno, ostatni)| {
                let shoda = if jmeno.starts_with(&d) {
                    300
                } else if jmeno.split(|c: char| !c.is_alphanumeric()).any(|s| s.starts_with(&d)) {
                    200
                } else if jmeno.contains(&d) {
                    120
                } else if d.split_whitespace().all(|s| jmeno.contains(s) || ostatni.contains(s)) {
                    40
                } else {
                    return None;
                };
                let id = a.id().map(|i| i.to_string()).unwrap_or_default();
                let pocet = pouziti[&id][0].as_u64().unwrap_or(0) as f64;
                let nedavno = ted().saturating_sub(pouziti[&id][1].as_u64().unwrap_or(0)) < 7 * 86400;
                Some((shoda + (30.0 * (1.0 + pocet).ln()) as i64 + if nedavno { 20 } else { 0 }, a.clone()))
            })
            .collect();
        aplikace.sort_by(|a, b| b.0.cmp(&a.0));
        if !aplikace.is_empty() {
            self.nadpis("Apps");
        }
        for (_, a) in aplikace.into_iter().take(NEJVIC_APLIKACI) {
            let obrazek = match a.icon() {
                Some(i) => gtk::Image::from_gicon(&i),
                None => gtk::Image::from_icon_name("application-x-executable"),
            };
            obrazek.set_pixel_size(IKONA);
            let popis = a.description().map(|d| d.to_string()).unwrap_or_default();
            let a2 = a.clone();
            self.radek(obrazek.upcast(), &a.display_name(), &popis, Rc::new(move |_| {
                if let Some(id) = a2.id() {
                    zapsat_pouziti(&id);
                }
                let kontext = gtk::gdk::Display::default().map(|d| d.app_launch_context());
                let _ = a2.launch(&[], kontext.as_ref());
            }));
        }

        // příkazy
        let prikazy = prikazy::najit(&dotaz);
        if !prikazy.is_empty() {
            self.nadpis("System");
        }
        for p in prikazy.into_iter().take(4) {
            let akce = p.akce;
            self.radek(Self::fa(p.ikona), p.nazev, p.popis, Rc::new(move |_| akce()));
        }

        // soubory: naposledy otevřené a z domovské složky
        let mut soubory: Vec<PathBuf> = Vec::new();
        if d.chars().count() >= 2 {
            let nedavne = gtk::RecentManager::default().items();
            for i in nedavne.iter().take(300) {
                let uri = i.uri();
                if !uri.starts_with("file://") {
                    continue;
                }
                let jmeno = normalizovat(&i.display_name());
                if d.split_whitespace().all(|s| jmeno.contains(s)) {
                    if let Some(p) = gio::File::for_uri(&uri).path().filter(|p| p.exists()) {
                        if !soubory.contains(&p) {
                            soubory.push(p);
                        }
                    }
                }
                if soubory.len() >= 3 {
                    break;
                }
            }
            let (pro_dotaz, nalezene) = &*self.soubory.borrow();
            if *pro_dotaz == dotaz {
                for p in nalezene {
                    if !soubory.contains(p) {
                        soubory.push(p.clone());
                    }
                }
            }
        }
        if !soubory.is_empty() {
            self.nadpis("Files");
        }
        for p in soubory.into_iter().take(NEJVIC_SOUBORU) {
            let nazev = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let slozka = p.parent().map(zkratit).unwrap_or_default();
            let p2 = p.clone();
            self.radek(ikona_souboru(&p).upcast(), &nazev, &format!("{slozka} · Ctrl+Enter shows it in the folder"), Rc::new(move |ctrl| {
                if ctrl {
                    ukazat_ve_slozce(&p2)
                } else {
                    otevrit(&p2)
                }
            }));
        }

        // web
        self.nadpis("Web");
        let d2 = dotaz.clone();
        self.radek(Self::fa("\u{f0ac}"), &format!("Search the web for “{dotaz}”"), "Firefox, default search engine", Rc::new(move |_| hledat_na_webu(&d2)));

        self.posuv.set_visible(true);
        if let Some((r, _)) = self.radky.borrow().first() {
            self.seznam.select_row(Some(r));
        }
        self.posuv.vadjustment().set_value(0.0);
    }

    fn posunout(&self, krok: i32) {
        let radky = self.radky.borrow();
        if radky.is_empty() {
            return;
        }
        let ted = self.seznam.selected_row().and_then(|v| radky.iter().position(|(r, _)| *r == v)).unwrap_or(0) as i32;
        let novy = (ted + krok).rem_euclid(radky.len() as i32) as usize;
        let r = &radky[novy].0;
        self.seznam.select_row(Some(r));
        // ať je vybraný řádek vidět
        if let Some(b) = r.compute_bounds(&self.seznam) {
            let adj = self.posuv.vadjustment();
            let (y, v) = (b.y() as f64, b.height() as f64);
            if y < adj.value() {
                adj.set_value(y - 30.0);
            } else if y + v > adj.value() + adj.page_size() {
                adj.set_value(y + v - adj.page_size());
            }
        }
    }

    fn spustit_vybrany(&self, ctrl: bool) {
        let akce = {
            let radky = self.radky.borrow();
            let vybrany = self.seznam.selected_row();
            vybrany.and_then(|v| radky.iter().find(|(r, _)| *r == v).map(|(_, a)| a.clone())).or_else(|| radky.first().map(|(_, a)| a.clone()))
        };
        if let Some(a) = akce {
            schovat();
            a(ctrl);
        }
    }
}
