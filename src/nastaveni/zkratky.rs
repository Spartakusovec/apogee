//! Stránka Shortcuts: všechny klávesové zkratky a gesta z ~/.config/sway/shortcuts.txt
//! (stejný seznam jako Super+F1), po skupinách a s vlastním hledáním.

use gtk::prelude::*;

use super::ui::*;
use crate::spolecne::domov;

/// (skupina, [(klávesy, co dělá)]); řádky bez dvou mezer jsou poznámky (klávesy prázdné)
fn nacist() -> Vec<(String, Vec<(String, String)>)> {
    let t = std::fs::read_to_string(domov().join(".config/sway/shortcuts.txt")).unwrap_or_default();
    let mut v: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for r in t.lines() {
        let r = r.trim_end();
        if r.contains('─') {
            v.push((r.trim_matches(|c: char| c == '─' || c == ' ').to_string(), Vec::new()));
            continue;
        }
        if r.is_empty() || v.is_empty() {
            continue;
        }
        let (k, co) = match r.find("  ") {
            Some(i) => (r[..i].trim().to_string(), r[i..].trim().to_string()),
            None => (String::new(), r.to_string()),
        };
        // první písmeno velké („close window“ → „Close window“)
        let mut z = co.chars();
        let co = z.next().map(|p| p.to_uppercase().chain(z).collect()).unwrap_or_default();
        v.last_mut().unwrap().1.push((k, co));
    }
    v
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Shortcuts");
    let hledat = gtk::SearchEntry::new();
    hledat.set_placeholder_text(Some("Find a shortcut, e.g. screenshot"));
    s.append(&hledat);
    let mut radky: Vec<(gtk::ListBoxRow, String, gtk::Widget)> = Vec::new();
    for (skupina_nazev, polozky) in nacist() {
        let karta = skupina(&s, &skupina_nazev, "");
        let obal = karta.parent().unwrap();
        for (k, co) in polozky {
            let l = gtk::Label::new(Some(&k));
            l.add_css_class("klavesy");
            let row = radek(&karta, "", &co, "", if k.is_empty() { None } else { Some(l.upcast_ref()) });
            let hledani = crate::spotlight::bez_diakritiky(&format!("{k} {co} {skupina_nazev}").to_lowercase());
            radky.push((row, hledani, obal.clone()));
        }
    }
    hledat.connect_search_changed(move |h| {
        let slova: Vec<String> = h.text().split_whitespace().map(|x| crate::spotlight::bez_diakritiky(&x.to_lowercase())).collect();
        let mut viditelne_skupiny = std::collections::HashSet::new();
        for (row, text, obal) in &radky {
            let ano = slova.iter().all(|w| text.contains(w.as_str()));
            row.set_visible(ano);
            if ano {
                viditelne_skupiny.insert(obal.clone());
            }
        }
        for (_, _, obal) in &radky {
            obal.set_visible(viditelne_skupiny.contains(obal));
        }
    });
    s.upcast()
}
