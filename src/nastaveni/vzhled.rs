//! Stránka Appearance: tapeta (a podle ní barvy celého prostředí, set-wallpaper), vlastní
//! obrázek, styl aplikací (tmavý / světlý), velikost textu, velikost kurzoru a tlačítka oken.

use std::path::PathBuf;
use gtk::prelude::*;

use super::sway_conf;
use super::ui::*;
use crate::okenka::tapety::{nahled, slozka_tapet, tapety};
use crate::spolecne::{domov, na_pozadi, prikaz};

const ROZHRANI: &str = "org.gnome.desktop.interface";

fn gsettings(schema: &str, klic: &str) -> String {
    prikaz("gsettings", &["get", schema, klic]).trim().trim_matches('\'').to_string()
}

fn nastavit_gsettings(schema: &str, klic: &str, hodnota: &str) {
    crate::spolecne::pozadi("gsettings", &["set", schema, klic, hodnota]);
}

/// Nastaví tapetu (barvy, zámek, reload Swaye trvá pár sekund).
fn pouzit_tapetu(cesta: PathBuf) {
    pouzit(cesta, "Wallpaper changed");
}

fn pouzit(cesta: PathBuf, hotovo: &'static str) {
    super::oznamit("Applying the colors…");
    let program = domov().join(".local/bin/set-wallpaper").to_string_lossy().to_string();
    na_pozadi(
        move || prikaz(&program, &[&cesta.to_string_lossy()]),
        move |_| {
            super::oznamit(hotovo);
            super::obnovit("vzhled");
        },
    );
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Appearance");

    // tapety
    let obal = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let n = gtk::Label::new(Some("Wallpaper"));
    n.add_css_class("nadpis-skupiny");
    n.set_xalign(0.0);
    obal.append(&n);
    let p = gtk::Label::new(Some("The colors of the panel, dock, windows and lock screen follow the wallpaper."));
    p.add_css_class("popis");
    p.set_xalign(0.0);
    obal.append(&p);
    let mrizka = gtk::FlowBox::new();
    mrizka.set_selection_mode(gtk::SelectionMode::None);
    mrizka.set_min_children_per_line(3);
    mrizka.set_max_children_per_line(3);
    mrizka.set_row_spacing(10);
    mrizka.set_column_spacing(10);
    mrizka.set_homogeneous(true);
    let aktualni = std::fs::canonicalize(domov().join(".config/theme/wallpaper")).ok();
    let seznam = tapety();
    let mut obrazky = Vec::new();
    for t in &seznam {
        let b = gtk::Button::new();
        b.add_css_class("tapeta");
        b.set_overflow(gtk::Overflow::Hidden);
        b.set_tooltip_text(t.file_stem().map(|x| x.to_string_lossy().to_string()).as_deref());
        if aktualni.as_ref() == std::fs::canonicalize(t).ok().as_ref() {
            b.add_css_class("aktivni-tapeta");
        }
        let obrazek = gtk::Picture::new();
        obrazek.set_content_fit(gtk::ContentFit::Cover);
        obrazek.set_size_request(184, 104);
        b.set_child(Some(&obrazek));
        let (cesta, m) = (t.clone(), mrizka.clone());
        b.connect_clicked(move |b| {
            let mut d = m.first_child();
            while let Some(w) = d {
                if let Some(tl) = w.first_child() {
                    tl.remove_css_class("aktivni-tapeta");
                }
                d = w.next_sibling();
            }
            b.add_css_class("aktivni-tapeta");
            pouzit_tapetu(cesta.clone());
        });
        mrizka.append(&b);
        obrazky.push(obrazek);
    }
    obal.append(&mrizka);
    let tlacitka = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    tlacitka.append(&tlacitko("Add a picture…", true, || {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Choose a wallpaper");
        let filtr = gtk::FileFilter::new();
        filtr.set_name(Some("Pictures"));
        filtr.add_mime_type("image/*");
        let filtry = gtk::gio::ListStore::new::<gtk::FileFilter>();
        filtry.append(&filtr);
        dialog.set_filters(Some(&filtry));
        dialog.open(super::okno().as_ref(), gtk::gio::Cancellable::NONE, |v| {
            let Some(zdroj) = v.ok().and_then(|f| f.path()) else { return };
            // zkopírovat do složky tapet, ať je v nabídce i příště
            let slozka = slozka_tapet();
            let _ = std::fs::create_dir_all(&slozka);
            let cil = slozka.join(zdroj.file_name().unwrap_or_default());
            if cil != zdroj && std::fs::copy(&zdroj, &cil).is_err() {
                super::oznamit("Couldn't copy the picture");
                return;
            }
            pouzit_tapetu(cil);
        });
    }));
    tlacitka.append(&tlacitko("Open the wallpaper folder", false, || {
        let s = slozka_tapet();
        let _ = std::fs::create_dir_all(&s);
        crate::spolecne::pozadi("xdg-open", &[&s.to_string_lossy()]);
    }));
    obal.append(&tlacitka);
    s.append(&obal);
    let cesty = seznam.clone();
    na_pozadi(
        move || cesty.iter().map(nahled).collect::<Vec<_>>(),
        move |nahledy| {
            for (o, n) in obrazky.iter().zip(nahledy) {
                if let Some(n) = n {
                    o.set_filename(Some(&n));
                }
            }
        },
    );

    // aplikace
    let karta = skupina(&s, "Look", "Colors always come from the wallpaper, in a dark or a light version. Apps like Files and Firefox use the wallpaper's accent color too.");
    let tmavy = gsettings(ROZHRANI, "color-scheme") == "prefer-dark";
    let styl = gtk::DropDown::from_strings(&["Dark", "Light"]);
    styl.set_selected(!tmavy as u32);
    styl.connect_selected_notify(|d| {
        let tmavy = d.selected() == 0;
        // hned (ne na pozadí): theme.py si režim přečte z gsettings
        prikaz("gsettings", &["set", ROZHRANI, "color-scheme", if tmavy { "prefer-dark" } else { "default" }]);
        // aplikace GTK 3 mají vlastní téma
        if gsettings(ROZHRANI, "gtk-theme").starts_with("adw-gtk3") {
            nastavit_gsettings(ROZHRANI, "gtk-theme", if tmavy { "adw-gtk3-dark" } else { "adw-gtk3" });
        }
        // ikony (Papirus-Wallpaper / -Dark) přepne set-wallpaper přes scripts/icons.sh
        // panel, dok, okna, zámek i tohle okno: nová paleta z tapety (tmavá / světlá)
        if let Ok(t) = std::fs::canonicalize(domov().join(".config/theme/wallpaper")) {
            pouzit(t, if tmavy { "Dark style on" } else { "Light style on" });
        }
    });
    radek(&karta, "\u{f042}", "Style", "The whole desktop and the apps", Some(styl.upcast_ref()));

    let velikosti = [("100 %", 1.0), ("110 %", 1.1), ("125 %", 1.25), ("150 %", 1.5)];
    let ted: f64 = gsettings(ROZHRANI, "text-scaling-factor").parse().unwrap_or(1.0);
    let text = gtk::DropDown::from_strings(&velikosti.iter().map(|v| v.0).collect::<Vec<_>>());
    text.set_selected(velikosti.iter().position(|v| (v.1 - ted).abs() < 0.01).unwrap_or(0) as u32);
    text.connect_selected_notify(move |d| {
        nastavit_gsettings(ROZHRANI, "text-scaling-factor", &format!("{}", velikosti[d.selected() as usize].1));
        super::oznamit("Text size changed – some apps need to be reopened");
    });
    radek(&karta, "\u{f031}", "Text size", "Bigger text in apps without making everything bigger", Some(text.upcast_ref()));

    let wm = "org.gnome.desktop.wm.preferences";
    let rozlozeni = [("Minimize, maximize and close", "appmenu:minimize,maximize,close"), ("Close only", "appmenu:close")];
    let ted = gsettings(wm, "button-layout");
    let tl = gtk::DropDown::from_strings(&rozlozeni.iter().map(|v| v.0).collect::<Vec<_>>());
    tl.set_selected(rozlozeni.iter().position(|v| v.1 == ted).unwrap_or(0) as u32);
    tl.connect_selected_notify(move |d| {
        nastavit_gsettings(wm, "button-layout", rozlozeni[d.selected() as usize].1);
        super::oznamit("Saved");
    });
    radek(&karta, "\u{f2d0}", "Window buttons", "In apps that draw their own title bar (Files, Settings…)", Some(tl.upcast_ref()));

    // kurzor
    let karta = skupina(&s, "Pointer", "");
    let kurzory = [("Normal", 24), ("Large", 32), ("Larger", 40), ("Largest", 48)];
    let ted: i64 = gsettings(ROZHRANI, "cursor-size").parse().unwrap_or(24);
    let k = gtk::DropDown::from_strings(&kurzory.iter().map(|v| v.0).collect::<Vec<_>>());
    k.set_selected(kurzory.iter().position(|v| v.1 == ted).unwrap_or(0) as u32);
    k.connect_selected_notify(move |d| {
        let v = kurzory[d.selected() as usize].1;
        nastavit_gsettings(ROZHRANI, "cursor-size", &v.to_string());
        let tema = gsettings(ROZHRANI, "cursor-theme");
        sway_conf::nastavit("seat seat0 xcursor_theme", &format!("{tema} {v}"));
        super::oznamit("Pointer size changed – some apps need to be reopened");
    });
    radek(&karta, "\u{f245}", "Pointer size", "", Some(k.upcast_ref()));

    s.upcast()
}
