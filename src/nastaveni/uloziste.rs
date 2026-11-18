//! Stránka Storage: zaplnění disku, co zabírá místo (složky, aplikace Flatpak, mezipaměti, koš;
//! velikosti přes du na pozadí) a uvolnění místa (koš, náhledy, nepoužívané Flatpak runtime,
//! mezipaměť dnf).

use gtk::prelude::*;

use super::ui::*;
use crate::spolecne::{domov, na_pozadi, prikaz};

/// Velikost v bajtech (du, chyby a nepřístupné soubory se ignorují).
fn velikost(cesta: &str) -> u64 {
    prikaz("du", &["-sb", cesta]).split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn citelne(b: u64) -> String {
    let b = b as f64;
    if b >= 1e9 { format!("{:.1} GB", b / 1e9) } else if b >= 1e6 { format!("{:.0} MB", b / 1e6) } else { format!("{:.0} kB", b / 1e3) }
}

struct Data {
    celkem: u64,
    volno: u64,
    polozky: Vec<(&'static str, &'static str, String, u64)>,
    kos: u64,
    nahledy: u64,
    dnf: u64,
}

fn nacist() -> Data {
    let df = prikaz("df", &["-B1", "--output=size,avail", "/"]);
    let c: Vec<u64> = df.lines().nth(1).unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let d = domov();
    let slozka = |druh: &str, vychozi: &str| {
        let p = prikaz("xdg-user-dir", &[druh]);
        if p.is_empty() || p == d.to_string_lossy() { d.join(vychozi).to_string_lossy().to_string() } else { p }
    };
    let mut polozky: Vec<(&str, &str, String, u64)> = vec![
        ("\u{f019}", "Downloads", slozka("DOWNLOAD", "Downloads")),
        ("\u{f03d}", "Videos", slozka("VIDEOS", "Videos")),
        ("\u{f03e}", "Pictures", slozka("PICTURES", "Pictures")),
        ("\u{f15b}", "Documents", slozka("DOCUMENTS", "Documents")),
        ("\u{f001}", "Music", slozka("MUSIC", "Music")),
        ("\u{f00a}", "Flatpak apps (system)", "/var/lib/flatpak".to_string()),
        ("\u{f00a}", "Flatpak apps (yours)", d.join(".local/share/flatpak").to_string_lossy().to_string()),
        ("\u{f07c}", "App data", d.join(".var/app").to_string_lossy().to_string()),
        ("\u{f1c0}", "Caches", d.join(".cache").to_string_lossy().to_string()),
    ]
    // du na všech složkách najednou (flatpak a mezipaměti trvají nejdéle)
    .into_iter()
    .map(|(i, n, p)| std::thread::spawn(move || {
        let v = velikost(&p);
        (i, n, p, v)
    }))
    .collect::<Vec<_>>()
    .into_iter()
    .filter_map(|h| h.join().ok())
    .filter(|x| x.3 > 0)
    .collect();
    polozky.sort_by_key(|x| std::cmp::Reverse(x.3));
    Data {
        celkem: c.first().copied().unwrap_or(0),
        volno: c.get(1).copied().unwrap_or(0),
        polozky,
        kos: velikost(&d.join(".local/share/Trash").to_string_lossy()),
        nahledy: velikost(&d.join(".cache/thumbnails").to_string_lossy()),
        dnf: velikost("/var/cache/libdnf5"),
    }
}

/// Úklid na pozadí, pak potvrzení a nové sestavení stránky.
fn uklid(program: &'static str, args: &'static [&'static str], hotovo: &'static str) {
    super::oznamit("Cleaning up…");
    na_pozadi(
        move || std::process::Command::new(program).args(args).output().map(|v| v.status.success()).unwrap_or(false),
        move |ok| {
            super::oznamit(if ok { hotovo } else { "Not cleaned" });
            super::obnovit("uloziste");
        },
    );
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Storage");
    let obal = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.append(&obal);
    let k = skupina(&obal, "", "");
    nacitani(&k, "Measuring… (takes a few seconds)");
    let o2 = obal.clone();
    na_pozadi(nacist, move |d| {
        if o2.root().is_none() {
            return;
        }
        while let Some(x) = o2.first_child() {
            o2.remove(&x);
        }
        ukazat(&o2, &d);
    });
    s.upcast()
}

fn ukazat(s: &gtk::Box, d: &Data) {
    let karta = skupina(s, "", "");
    let pouzito = d.celkem.saturating_sub(d.volno);
    let uroven = gtk::LevelBar::for_interval(0.0, 1.0);
    for n in ["low", "high", "full"] {
        uroven.remove_offset_value(Some(n));
    }
    uroven.set_value(if d.celkem > 0 { pouzito as f64 / d.celkem as f64 } else { 0.0 });
    uroven.set_size_request(200, -1);
    radek(&karta, "\u{f0a0}", &format!("{} free", citelne(d.volno)), &format!("{} of {} used", citelne(pouzito), citelne(d.celkem)), Some(uroven.upcast_ref()));

    let karta = skupina(s, "What uses the space", "");
    for (ikona, nazev, cesta, v) in &d.polozky {
        let c = cesta.clone();
        let otevrit = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let l = gtk::Label::new(Some(&citelne(*v)));
        l.add_css_class("popis");
        otevrit.append(&l);
        if !cesta.starts_with("/var") {
            otevrit.append(&tlacitko("Open", false, move || crate::spolecne::pozadi("xdg-open", &[&c])));
        }
        radek(&karta, ikona, nazev, "", Some(otevrit.upcast_ref()));
    }

    let karta = skupina(s, "Free up space", "");
    let kos = tlacitko("Empty", false, || uklid("gio", &["trash", "--empty"], "Trash emptied"));
    kos.set_sensitive(d.kos > 0);
    radek(&karta, "\u{f1f8}", "Trash", &citelne(d.kos), Some(kos.upcast_ref()));
    let nahledy = tlacitko("Clear", false, || {
        let p = domov().join(".cache/thumbnails");
        let _ = std::fs::remove_dir_all(&p);
        super::oznamit("Thumbnails cleared – they're made again when needed");
        super::obnovit("uloziste");
    });
    radek(&karta, "\u{f03e}", "Picture thumbnails", &citelne(d.nahledy), Some(nahledy.upcast_ref()));
    radek(&karta, "\u{f00a}", "Unused Flatpak parts", "Old runtimes no app needs any more", Some(tlacitko("Remove", false, || uklid("flatpak", &["uninstall", "--unused", "-y", "--noninteractive"], "Unused Flatpak parts removed")).upcast_ref()));
    radek(&karta, "\u{f019}", "Downloaded system updates", &format!("{} – asks for your password", citelne(d.dnf)), Some(tlacitko("Clear", false, || uklid("pkexec", &["dnf", "clean", "packages"], "Update downloads cleared")).upcast_ref()));
}
