//! Aktualizace: seznam balíků (dnf) a aplikací (Flatpak), instalace v terminálu, kontrola znovu.

use std::rc::Rc;

use gtk::prelude::*;
use serde_json::Value;

use crate::aktualizace;
use crate::spolecne::*;

pub fn naplnit(o: &Rc<Okenko>) {
    ukazat(o, &aktualizace::nacist());
}

fn kdy(cas: u64) -> String {
    if cas == 0 {
        return "Not checked yet".into();
    }
    chrono::DateTime::from_timestamp(cas as i64, 0)
        .map(|d| format!("Checked {}", d.with_timezone(&chrono::Local).format("%-d %b, %H:%M")))
        .unwrap_or_default()
}

fn ukazat(o: &Rc<Okenko>, v: &Value) {
    o.smazat_obsah();
    let seznam = |k: &str| v[k].as_array().cloned().unwrap_or_default();
    let (dnf, flatpak) = (seznam("dnf"), seznam("flatpak"));
    let cas = v["cas"].as_u64().unwrap_or(0);
    let popis = match (dnf.len(), flatpak.len()) {
        (0, 0) if cas == 0 => "Checked every 6 hours".to_string(),
        (0, 0) => "Everything is up to date".to_string(),
        (d, 0) => format!("{d} system packages"),
        (0, f) => format!("{f} Flatpak apps and runtimes"),
        (d, f) => format!("{d} system packages, {f} Flatpak apps and runtimes"),
    };
    o.panel.append(&nadpis("Updates", &popis));

    if !dnf.is_empty() || !flatpak.is_empty() {
        let obsah = gtk::Box::new(gtk::Orientation::Vertical, 2);
        for (nazev, polozky) in [("System", &dnf), ("Flatpak", &flatpak)] {
            if polozky.is_empty() {
                continue;
            }
            let h = popisek(nazev, &["dim"]);
            h.set_margin_top(4);
            obsah.append(&h);
            for p in polozky {
                let radek = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let n = popisek(p[0].as_str().unwrap_or(""), &[]);
                n.set_hexpand(true);
                n.set_ellipsize(gtk::pango::EllipsizeMode::End);
                radek.append(&n);
                radek.append(&popisek(p[1].as_str().unwrap_or(""), &["vpravo"]));
                obsah.append(&radek);
            }
        }
        let posuv = gtk::ScrolledWindow::new();
        posuv.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        posuv.set_propagate_natural_height(true);
        posuv.set_max_content_height(320);
        posuv.set_child(Some(&obsah));
        o.panel.append(&posuv);

        let instalovat = gtk::Button::with_label("Install in a terminal…");
        instalovat.add_css_class("hlavni-akce");
        let slabe = Rc::downgrade(o);
        instalovat.connect_clicked(move |_| {
            aktualizace::nainstalovat();
            if let Some(o) = slabe.upgrade() {
                o.zavrit();
            }
        });
        o.panel.append(&instalovat);
    }

    let dole = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let k = popisek(&kdy(cas), &["dim"]);
    k.set_hexpand(true);
    dole.append(&k);
    let znovu = gtk::Button::with_label(if aktualizace::kontroluje() { "Checking…" } else { "Check now" });
    znovu.add_css_class("odkaz");
    let slabe = Rc::downgrade(o);
    znovu.connect_clicked(move |b| {
        b.set_label("Checking…");
        b.set_sensitive(false);
        let slabe = slabe.clone();
        aktualizace::zkontrolovat(move |v| {
            if let Some(o) = slabe.upgrade() {
                if o.okno.is_visible() {
                    ukazat(&o, &v);
                }
            }
        });
    });
    dole.append(&znovu);
    o.panel.append(&dole);
    o.prepocitat();
}
