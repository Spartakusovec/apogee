//! Historie schránky (Super+V): psaním se filtruje, šipky a Enter (nebo klik) vybírají.
//! U obrázků je náhled (uložený v ~/.cache/apogee/clipboard). Dřív schranka.sh přes fuzzel.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};

use crate::spolecne::*;

const NAHLED: i32 = 64;
const NEJVIC: usize = 300;

fn slozka_nahledu() -> PathBuf {
    domov().join(".cache/apogee/clipboard")
}

/// Obsah záznamu (řádek z `cliphist list`).
fn dekodovat(radek: &str) -> Vec<u8> {
    let Ok(mut p) = Command::new("cliphist").arg("decode").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return Vec::new();
    };
    if let Some(mut vstup) = p.stdin.take() {
        let _ = vstup.write_all(format!("{radek}\n").as_bytes());
    }
    p.wait_with_output().map(|o| o.stdout).unwrap_or_default()
}

/// Náhled obrázku (vytvoří ho, když ještě není; pomalé, volat mimo GTK).
fn vytvorit_nahled(radek: &str, soubor: &PathBuf) -> bool {
    if soubor.exists() {
        return true;
    }
    let data = dekodovat(radek);
    let nacitac = gtk::gdk_pixbuf::PixbufLoader::new();
    if nacitac.write(&data).is_err() || nacitac.close().is_err() {
        return false;
    }
    let Some(obrazek) = nacitac.pixbuf() else { return false };
    let (s, v) = (obrazek.width() as f64, obrazek.height() as f64);
    let mira = (160.0 / s.max(v)).min(1.0);
    let Some(maly) = obrazek.scale_simple((s * mira).max(1.0) as i32, (v * mira).max(1.0) as i32, gtk::gdk_pixbuf::InterpType::Bilinear) else {
        return false;
    };
    let _ = std::fs::create_dir_all(slozka_nahledu());
    maly.savev(soubor, "png", &[]).is_ok()
}

/// „[[ binary data 1 MiB png 1920x1080 ]]“ → „Image · png 1920x1080 · 1 MiB“
fn popis_obrazku(obsah: &str) -> String {
    let vnitrek = obsah.trim_start_matches("[[ binary data ").trim_end_matches(" ]]");
    let casti: Vec<&str> = vnitrek.split_whitespace().collect();
    if casti.len() >= 3 {
        format!("Image  ·  {}  ·  {} {}", casti[2..].join(" "), casti[0], casti[1])
    } else {
        "Image".into()
    }
}

pub fn naplnit(o: &Rc<Okenko>) {
    // ovládá se klávesnicí: psaní filtruje, šipky, Enter; nezavírat samo
    o.okno.set_keyboard_mode(KeyboardMode::Exclusive);
    o.nezavirat_samo();
    let zaznamy: Vec<String> = prikaz("cliphist", &["list"]).lines().take(NEJVIC).map(String::from).collect();
    if zaznamy.is_empty() {
        o.stav("The clipboard history is empty");
        return;
    }

    let hledat = gtk::SearchEntry::new();
    hledat.set_placeholder_text(Some("Search the clipboard…"));
    // Esc si pole pro hledání nechá pro sebe (vymazání hledání), tak zavřít okénko tady
    let slaby = Rc::downgrade(o);
    hledat.connect_stop_search(move |_| {
        if let Some(o) = slaby.upgrade() {
            o.zavrit();
        }
    });
    o.panel.append(&hledat);
    let seznam = gtk::ListBox::new();
    seznam.add_css_class("schranka");
    seznam.set_selection_mode(gtk::SelectionMode::Browse);
    let posuv = gtk::ScrolledWindow::new();
    posuv.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    posuv.set_propagate_natural_height(true);
    posuv.set_max_content_height(480);
    posuv.set_child(Some(&seznam));
    o.panel.append(&posuv);

    let mut radky: Vec<(gtk::ListBoxRow, String)> = Vec::new();
    let mut chybi_nahled: Vec<(String, PathBuf, gtk::Image)> = Vec::new();
    for radek in &zaznamy {
        let (id, obsah) = radek.split_once('\t').unwrap_or(("", radek));
        let obsah_radku = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let text = if obsah.starts_with("[[ binary data") {
            let soubor = slozka_nahledu().join(format!("{id}.png"));
            let nahled = gtk::Image::new();
            nahled.set_pixel_size(NAHLED);
            if soubor.exists() {
                nahled.set_from_file(Some(&soubor));
            } else {
                chybi_nahled.push((radek.clone(), soubor, nahled.clone()));
            }
            obsah_radku.append(&nahled);
            popis_obrazku(obsah)
        } else {
            obsah.to_string()
        };
        let l = popisek(&text, &[]);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        l.set_hexpand(true);
        obsah_radku.append(&l);
        let r = gtk::ListBoxRow::new();
        r.set_child(Some(&obsah_radku));
        seznam.append(&r);
        radky.push((r, text.to_lowercase()));
    }
    let radky = Rc::new(radky);
    let zaznamy = Rc::new(zaznamy);

    // vybrat: zkopírovat do schránky a zavřít
    let slaby = Rc::downgrade(o);
    let z = zaznamy.clone();
    seznam.connect_row_activated(move |_, r| {
        let Some(radek) = z.get(r.index() as usize).cloned() else { return };
        if let Some(o) = slaby.upgrade() {
            o.zavrit();
        }
        std::thread::spawn(move || {
            let data = dekodovat(&radek);
            if let Ok(mut p) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
                if let Some(mut vstup) = p.stdin.take() {
                    let _ = vstup.write_all(&data);
                }
                let _ = p.wait();
            }
        });
    });

    // psaní filtruje, první viditelný řádek je vybraný
    let prvni_viditelny = {
        let radky = radky.clone();
        move || radky.iter().find(|(r, _)| r.is_visible()).map(|(r, _)| r.clone())
    };
    let (r2, s2, prvni) = (radky.clone(), seznam.downgrade(), prvni_viditelny.clone());
    hledat.connect_search_changed(move |h| {
        let hledane = h.text().to_lowercase();
        for (r, text) in r2.iter() {
            r.set_visible(hledane.is_empty() || text.contains(&hledane));
        }
        if let (Some(s), Some(r)) = (s2.upgrade(), prvni()) {
            s.select_row(Some(&r));
        }
    });
    // Enter v hledání vybere označený řádek, šipka dolů přejde do seznamu
    let s2 = seznam.downgrade();
    hledat.connect_activate(move |_| {
        if let Some(s) = s2.upgrade() {
            if let Some(r) = s.selected_row() {
                r.activate();
            }
        }
    });
    let klavesy = gtk::EventControllerKey::new();
    let (s2, prvni) = (seznam.downgrade(), prvni_viditelny.clone());
    klavesy.connect_key_pressed(move |_, k, _, _| {
        if k == gtk::gdk::Key::Down {
            if let (Some(s), Some(r)) = (s2.upgrade(), prvni()) {
                s.select_row(Some(&r));
                r.grab_focus();
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    hledat.add_controller(klavesy);
    // vybrat první řádek až po ukázání okénka (dřív GTK výběr nezobrazí)
    let (s2, prvni) = (seznam.downgrade(), prvni_viditelny.clone());
    glib::idle_add_local_once(move || {
        if let (Some(s), Some(r)) = (s2.upgrade(), prvni()) {
            s.select_row(Some(&r));
        }
    });
    hledat.grab_focus();

    // chybějící náhledy na pozadí, staré (smazané záznamy) uklidit
    let id_zaznamu: Vec<String> = zaznamy.iter().filter_map(|r| r.split_once('\t').map(|(id, _)| format!("{id}.png"))).collect();
    let k_vytvoreni: Vec<(String, PathBuf)> = chybi_nahled.iter().map(|(r, s, _)| (r.clone(), s.clone())).collect();
    na_pozadi(
        move || {
            if let Ok(soubory) = std::fs::read_dir(slozka_nahledu()) {
                for s in soubory.flatten() {
                    if !id_zaznamu.contains(&s.file_name().to_string_lossy().to_string()) {
                        let _ = std::fs::remove_file(s.path());
                    }
                }
            }
            k_vytvoreni.iter().map(|(r, s)| vytvorit_nahled(r, s)).collect::<Vec<_>>()
        },
        move |hotove| {
            for ((_, soubor, obrazek), ok) in chybi_nahled.iter().zip(hotove) {
                if ok {
                    obrazek.set_from_file(Some(soubor));
                }
            }
        },
    );
}
