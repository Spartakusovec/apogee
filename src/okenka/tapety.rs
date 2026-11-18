//! Výběr tapety: náhledy obrázků z ~/Pictures/Wallpapers, klik nastaví tapetu a podle ní barvy
//! celého prostředí (set-wallpaper). Náhledy se ukládají do ~/.cache/apogee/wallpapers.

use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;

use crate::spolecne::*;

const SIRKA: i32 = 172;
const VYSKA: i32 = 97;

pub fn slozka_tapet() -> PathBuf {
    let obrazky = prikaz("xdg-user-dir", &["PICTURES"]);
    let obrazky = if obrazky.is_empty() { domov().join("Pictures") } else { PathBuf::from(obrazky) };
    obrazky.join("Tapety")
}

pub fn tapety() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(slozka_tapet())
        .map(|d| {
            d.flatten()
                .map(|s| s.path())
                .filter(|p| {
                    let pripona = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    matches!(pripona.as_str(), "jpg" | "jpeg" | "png" | "webp")
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Náhled (vytvoří ho, když ještě není; pomalé, volat mimo GTK).
pub fn nahled(tapeta: &PathBuf) -> Option<PathBuf> {
    let zmena = std::fs::metadata(tapeta).and_then(|m| m.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let slozka = domov().join(".cache/apogee/wallpapers");
    let soubor = slozka.join(format!("{}-{zmena}.jpg", tapeta.file_stem()?.to_string_lossy()));
    if !soubor.exists() {
        let _ = std::fs::create_dir_all(&slozka);
        let obrazek = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(tapeta, SIRKA * 2, -1, true).ok()?;
        obrazek.savev(&soubor, "jpeg", &[("quality", "88")]).ok()?;
    }
    Some(soubor)
}

pub fn naplnit(o: &Rc<Okenko>) {
    o.panel.append(&nadpis("Wallpaper", "The colors of the whole desktop follow the wallpaper"));
    let seznam = tapety();
    if seznam.is_empty() {
        o.stav("No pictures in Pictures → Wallpapers yet");
    }
    let aktualni = std::fs::canonicalize(domov().join(".config/theme/wallpaper")).ok();
    let mrizka = gtk::Grid::new();
    mrizka.set_row_spacing(10);
    mrizka.set_column_spacing(10);
    let mut obrazky = Vec::new();
    for (i, tapeta) in seznam.iter().enumerate() {
        let b = gtk::Button::new();
        b.add_css_class("tapeta");
        b.set_overflow(gtk::Overflow::Hidden);
        if aktualni.as_ref() == std::fs::canonicalize(tapeta).ok().as_ref() {
            b.add_css_class("aktivni-tapeta");
        }
        let obrazek = gtk::Picture::new();
        obrazek.set_content_fit(gtk::ContentFit::Cover);
        obrazek.set_size_request(SIRKA, VYSKA);
        b.set_child(Some(&obrazek));
        let (cesta, slabe, mrizka2) = (tapeta.to_string_lossy().to_string(), Rc::downgrade(o), mrizka.clone());
        b.connect_clicked(move |b| {
            let Some(o) = slabe.upgrade() else { return };
            // přebarvení trvá chvilku (rozmazaná tapeta pro zámek, nové barvy, reload Swaye)
            let mut d = mrizka2.first_child();
            while let Some(w) = d {
                w.remove_css_class("aktivni-tapeta");
                d = w.next_sibling();
            }
            b.add_css_class("aktivni-tapeta");
            mrizka2.set_sensitive(false);
            o.drzet.set(true);
            o.stav("Applying the wallpaper and colors…");
            let cesta = cesta.clone();
            let program = domov().join(".local/bin/set-wallpaper").to_string_lossy().to_string();
            let slabe = Rc::downgrade(&o);
            na_pozadi(
                move || prikaz(&program, &[&cesta]),
                move |_| {
                    if let Some(o) = slabe.upgrade() {
                        o.zavrit();
                    }
                },
            );
        });
        mrizka.attach(&b, (i % 2) as i32, (i / 2) as i32, 1, 1);
        obrazky.push((tapeta.clone(), obrazek));
    }
    o.panel.append(&mrizka);

    let slozka = gtk::Button::with_label("Open the wallpaper folder…");
    slozka.add_css_class("odkaz");
    slozka.set_halign(gtk::Align::Start);
    let slabe = Rc::downgrade(o);
    slozka.connect_clicked(move |_| {
        let s = slozka_tapet();
        let _ = std::fs::create_dir_all(&s);
        pozadi("xdg-open", &[&s.to_string_lossy()]);
        if let Some(o) = slabe.upgrade() {
            o.zavrit();
        }
    });
    o.panel.append(&slozka);

    // náhledy na pozadí, ať se okénko otevře hned
    let cesty: Vec<PathBuf> = obrazky.iter().map(|(t, _)| t.clone()).collect();
    na_pozadi(
        move || cesty.iter().map(nahled).collect::<Vec<_>>(),
        move |nahledy| {
            for ((_, obrazek), nahled) in obrazky.iter().zip(nahledy) {
                if let Some(n) = nahled {
                    obrazek.set_filename(Some(&n));
                }
            }
        },
    );
}
