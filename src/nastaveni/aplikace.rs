//! Stránka Apps: výchozí aplikace (prohlížeč, soubory, video, hudba, obrázky, PDF, text, e-mail)
//! a aplikace spouštěné po přihlášení (nastaveni.json `po_startu`, spouští je démon jednou
//! za sezení).

use gtk::gio;
use gtk::prelude::*;
use serde_json::json;

use super::ui::*;
use crate::spolecne::{nastaveni, run_dir, ulozit_nastaveni};

/// (název, typy, které se nastaví; první určuje seznam aplikací a aktuální volbu)
const VYCHOZI: &[(&str, &str, &[&str])] = &[
    ("Web browser", "\u{f0ac}", &["x-scheme-handler/https", "x-scheme-handler/http", "text/html", "application/xhtml+xml"]),
    ("Files", "\u{f07b}", &["inode/directory"]),
    ("Video", "\u{f03d}", &["video/mp4", "video/x-matroska", "video/webm", "video/quicktime", "video/x-msvideo", "video/mpeg"]),
    ("Music", "\u{f001}", &["audio/mpeg", "audio/flac", "audio/ogg", "audio/x-wav", "audio/mp4", "audio/x-vorbis+ogg"]),
    ("Pictures", "\u{f03e}", &["image/jpeg", "image/png", "image/webp", "image/gif", "image/heif", "image/bmp"]),
    ("PDF documents", "\u{f1c1}", &["application/pdf"]),
    ("Text files", "\u{f15c}", &["text/plain"]),
    ("Email", "\u{f0e0}", &["x-scheme-handler/mailto"]),
];

fn po_startu() -> Vec<String> {
    nastaveni("po_startu").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Démon po startu: spustí aplikace z `po_startu`, jen jednou za přihlášení (ne po restartu démona).
pub fn spustit_po_prihlaseni() {
    let znacka = run_dir().join("apogee-autostart-done");
    if znacka.exists() {
        return;
    }
    let _ = std::fs::write(&znacka, "");
    gtk::glib::timeout_add_seconds_local_once(3, || {
        for id in po_startu() {
            if let Some(a) = najit(&id) {
                let _ = a.launch(&[], gio::AppLaunchContext::NONE);
            }
        }
    });
}

fn najit(id: &str) -> Option<gio::AppInfo> {
    gio::AppInfo::all().into_iter().find(|a| a.id().is_some_and(|i| i == id))
}

/// Výběr aplikace s ikonou a názvem (v tlačítku i v rozbaleném seznamu).
fn vyber_aplikace(aplikace: &[gio::AppInfo]) -> gtk::DropDown {
    let seznam = gio::ListStore::new::<gio::AppInfo>();
    for a in aplikace {
        seznam.append(a);
    }
    let tovarna = gtk::SignalListItemFactory::new();
    tovarna.connect_setup(|_, polozka| {
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let i = gtk::Image::new();
        i.set_pixel_size(20);
        r.append(&i);
        r.append(&gtk::Label::new(None));
        polozka.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&r));
    });
    tovarna.connect_bind(|_, polozka| {
        let polozka = polozka.downcast_ref::<gtk::ListItem>().unwrap();
        let (Some(r), Some(a)) = (polozka.child(), polozka.item().and_downcast::<gio::AppInfo>()) else { return };
        let i = r.first_child().and_downcast::<gtk::Image>().unwrap();
        match a.icon() {
            Some(ikona) => i.set_from_gicon(&ikona),
            None => i.set_icon_name(Some("application-x-executable")),
        }
        r.last_child().and_downcast::<gtk::Label>().unwrap().set_text(&nazev(&a));
    });
    let d = gtk::DropDown::new(Some(seznam), None::<gtk::Expression>);
    d.set_factory(Some(&tovarna));
    d
}

fn nazev(a: &gio::AppInfo) -> String {
    a.display_name().to_string()
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Apps");

    let karta = skupina(&s, "Default apps", "What opens when you open a link or a file.");
    for (nazev_typu, ikona, typy) in VYCHOZI {
        let mut moznosti: Vec<gio::AppInfo> = gio::AppInfo::all_for_type(typy[0]).into_iter().filter(|a| a.should_show()).collect();
        moznosti.sort_by_key(|a| nazev(a).to_lowercase());
        moznosti.dedup_by_key(|a| a.id());
        let ted = gio::AppInfo::default_for_type(typy[0], false);
        if moznosti.is_empty() {
            radek(&karta, ikona, nazev_typu, "No app installed for this", None);
            continue;
        }
        let d = vyber_aplikace(&moznosti);
        let pozice = ted.as_ref().and_then(|t| moznosti.iter().position(|a| a.id() == t.id()));
        d.set_selected(pozice.map(|p| p as u32).unwrap_or(gtk::INVALID_LIST_POSITION));
        let typy: &'static [&'static str] = typy;
        let n: &'static str = nazev_typu;
        d.connect_selected_notify(move |d| {
            let Some(a) = moznosti.get(d.selected() as usize) else { return };
            let ok = typy.iter().all(|t| a.set_as_default_for_type(t).is_ok());
            super::oznamit(&if ok { format!("{n}: {}", nazev(a)) } else { "Couldn't change it".to_string() });
        });
        radek(&karta, ikona, nazev_typu, "", Some(d.upcast_ref()));
    }

    let karta = skupina(&s, "Start after login", "These apps open by themselves every time you log in.");
    let seznam = po_startu();
    if seznam.is_empty() {
        radek(&karta, "", "No apps yet", "", None);
    }
    for (i, id) in seznam.iter().enumerate() {
        let info = najit(id);
        let jmeno = info.as_ref().map(|a| a.display_name().to_string()).unwrap_or_else(|| id.clone());
        let s2 = seznam.clone();
        let odebrat = tlacitko("Remove", false, move || {
            let mut v = s2.clone();
            v.remove(i);
            ulozit_nastaveni("po_startu", json!(v));
            super::obnovit("aplikace");
        });
        let r = radek(&karta, "", &jmeno, "", Some(odebrat.upcast_ref()));
        if let Some(i) = info.and_then(|a| a.icon()) {
            let obr = gtk::Image::from_gicon(&i);
            obr.set_pixel_size(28);
            if let Some(b) = r.child().and_downcast::<gtk::Box>() {
                b.prepend(&obr);
            }
        }
    }
    let mut vsechny: Vec<gio::AppInfo> = gio::AppInfo::all().into_iter().filter(|a| a.should_show() && a.id().is_some_and(|i| !seznam.iter().any(|x| *x == i))).collect();
    vsechny.sort_by_key(|a| nazev(a).to_lowercase());
    let mut jmena = vec!["Choose…".to_string()];
    jmena.extend(vsechny.iter().map(nazev));
    let d = gtk::DropDown::from_strings(&jmena.iter().map(String::as_str).collect::<Vec<_>>());
    d.set_enable_search(true);
    d.set_expression(Some(gtk::PropertyExpression::new(gtk::StringObject::static_type(), None::<gtk::Expression>, "string")));
    d.connect_selected_notify(move |d| {
        let i = d.selected() as usize;
        if i == 0 {
            return;
        }
        let Some(id) = vsechny.get(i - 1).and_then(|a| a.id()) else { return };
        let mut v = po_startu();
        v.push(id.to_string());
        ulozit_nastaveni("po_startu", json!(v));
        super::oznamit("Added");
        super::obnovit("aplikace");
    });
    radek(&karta, "\u{f067}", "Add an app", "", Some(d.upcast_ref()));
    s.upcast()
}
