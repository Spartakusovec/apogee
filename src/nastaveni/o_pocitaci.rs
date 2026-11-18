//! Stránka About: počítač (model, procesor, paměť, grafika a jestli NVIDIA spí, disk), systém
//! (Fedora, jádro, Sway s úpravou, doba běhu) a záloha (~/.dotfiles: kdy naposledy, zálohovat
//! teď, zkontrolovat obnovu). „Copy“ zkopíruje přehled do schránky.

use gtk::prelude::*;

use super::ui::*;
use crate::spolecne::{domov, na_pozadi, prikaz};

struct Udaje {
    model: String,
    procesor: String,
    pamet: String,
    grafika: Vec<String>,
    nvidia_spi: Option<bool>,
    disk: String,
    system: String,
    jadro: String,
    sway: String,
    bezi: String,
    nazev: String,
    zaloha: String,
}

fn cist(cesta: &str) -> String {
    std::fs::read_to_string(cesta).unwrap_or_default().trim().to_string()
}

fn nacist() -> Udaje {
    let model = cist("/sys/class/dmi/id/product_version");
    let model = if model.is_empty() { format!("{} {}", cist("/sys/class/dmi/id/sys_vendor"), cist("/sys/class/dmi/id/product_name")) } else { model };
    let cpuinfo = cist("/proc/cpuinfo");
    let procesor = cpuinfo.lines().find_map(|r| r.strip_prefix("model name").and_then(|r| r.split_once(':')).map(|(_, v)| v.trim().to_string())).unwrap_or_default();
    let jader = cpuinfo.lines().filter(|r| r.starts_with("processor")).count();
    let procesor = procesor.replace("(R)", "").replace("(TM)", "").replace(" CPU", "");
    let kb = cist("/proc/meminfo").lines().find_map(|r| r.strip_prefix("MemTotal:")).and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(0.0);
    // grafika ze sysfs a pci.ids – ne lspci: to čte konfiguraci karty a spící NVIDII probudí
    let mut grafika = Vec::new();
    let mut nvidia_spi = None;
    let mut zarizeni: Vec<_> = std::fs::read_dir("/sys/bus/pci/devices").map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    zarizeni.sort();
    for z in zarizeni {
        let c = |s: &str| cist(&format!("{}/{s}", z.display()));
        if !c("class").starts_with("0x03") {
            continue;
        }
        let (vyrobce, cislo) = (c("vendor"), c("device"));
        if vyrobce == "0x10de" {
            nvidia_spi = Some(c("power/runtime_status") == "suspended");
        }
        grafika.push(nazev_pci(vyrobce.trim_start_matches("0x"), cislo.trim_start_matches("0x")));
    }
    let df = prikaz("df", &["-h", "--output=size,avail", "/"]);
    let disk = df.lines().nth(1).map(|r| {
        let c: Vec<&str> = r.split_whitespace().collect();
        let gb = |x: &str| format!("{} {}B", x.trim_end_matches(char::is_alphabetic), x.trim_start_matches(|z: char| z.is_ascii_digit() || z == '.'));
        format!("{}, {} free", gb(c.first().unwrap_or(&"")), gb(c.get(1).unwrap_or(&"")))
    }).unwrap_or_default();
    let os = cist("/etc/os-release");
    let system = os.lines().find_map(|r| r.strip_prefix("PRETTY_NAME=")).unwrap_or("").trim_matches('"').to_string();
    let sekundy: f64 = cist("/proc/uptime").split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let s = sekundy as u64;
    let bezi = if s >= 86400 { format!("{} days {} h", s / 86400, s / 3600 % 24) } else { format!("{} h {} min", s / 3600, s / 60 % 60) };
    let rpm = prikaz("rpm", &["-q", "--qf", "%{VERSION}-%{RELEASE}", "sway"]);
    let sway = if rpm.contains(".fc") && prikaz("rpm", &["-q", "--qf", "%{RELEASE}", "sway"]).split('.').count() > 2 {
        format!("{rpm}  ·  with the title bar buttons patch")
    } else {
        rpm
    };
    let zaloha = prikaz("git", &["-C", &domov().join(".dotfiles").to_string_lossy(), "log", "-1", "--format=%cr"]);
    Udaje {
        model,
        procesor: format!("{procesor}  ·  {jader} cores"),
        pamet: format!("{:.1} GB", kb / 1024.0 / 1024.0),
        grafika,
        nvidia_spi,
        disk,
        system,
        jadro: prikaz("uname", &["-r"]),
        sway,
        bezi,
        nazev: cist("/etc/hostname"),
        zaloha,
    }
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("About");
    let obal = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.append(&obal);
    let docasna = skupina(&obal, "", "");
    nacitani(&docasna, "Loading…");
    let o2 = obal.clone();
    na_pozadi(nacist, move |u| {
        if o2.root().is_none() {
            return;
        }
        while let Some(d) = o2.first_child() {
            o2.remove(&d);
        }
        ukazat(&o2, &u);
    });
    s.upcast()
}

fn hodnota(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("popis");
    l.set_selectable(true);
    l.set_wrap(true);
    l.set_xalign(1.0);
    l.set_max_width_chars(40);
    l
}

fn ukazat(s: &gtk::Box, u: &Udaje) {
    let karta = skupina(s, "Computer", "");
    radek(&karta, "\u{f109}", &u.model, &u.nazev, None);
    radek(&karta, "", "Processor", "", Some(hodnota(&u.procesor).upcast_ref()));
    radek(&karta, "", "Memory", "", Some(hodnota(&u.pamet).upcast_ref()));
    radek(&karta, "", "Graphics", "", Some(hodnota(&u.grafika.join("\n")).upcast_ref()));
    if let Some(spi) = u.nvidia_spi {
        let popis = if spi { "Asleep – saving battery" } else { "Awake – something is using it, or it is waking up" };
        radek(&karta, "", "NVIDIA graphics", "Only games and apps that ask for it wake it up", Some(hodnota(popis).upcast_ref()));
    }
    radek(&karta, "", "Disk", "", Some(hodnota(&u.disk).upcast_ref()));

    let karta = skupina(s, "Software", "");
    radek(&karta, "", "System", "", Some(hodnota(&u.system).upcast_ref()));
    radek(&karta, "", "Linux kernel", "", Some(hodnota(&u.jadro).upcast_ref()));
    radek(&karta, "", "Sway", "", Some(hodnota(&u.sway).upcast_ref()));
    radek(&karta, "", "Running for", "", Some(hodnota(&u.bezi).upcast_ref()));
    let prehled = format!(
        "{}\nProcessor: {}\nMemory: {}\nGraphics: {}\nDisk: {}\nSystem: {}\nKernel: {}\nSway: {}",
        u.model, u.procesor, u.pamet, u.grafika.join(", "), u.disk, u.system, u.jadro, u.sway
    );
    let kopie = tlacitko("Copy", false, move || {
        if let Some(d) = gtk::gdk::Display::default() {
            d.clipboard().set_text(&prehled);
            super::oznamit("Copied – paste it where someone asks about your computer");
        }
    });
    radek(&karta, "\u{f0c5}", "Copy these details", "", Some(kopie.upcast_ref()));

    let karta = skupina(s, "Backup", "Settings, scripts and the list of installed apps are saved to ~/.dotfiles and GitHub, so the whole setup can be restored on a fresh Fedora.");
    let posledni = if u.zaloha.is_empty() { "Never".to_string() } else { u.zaloha.clone() };
    let zaloha = tlacitko("Back up now", true, || {
        super::oznamit("Backing up…");
        let skript = domov().join(".dotfiles/backup.sh").to_string_lossy().to_string();
        na_pozadi(
            move || prikaz("bash", &[&skript]),
            |vystup| {
                super::oznamit(vystup.lines().filter(|r| !r.trim().is_empty()).last().unwrap_or("Done"));
                super::obnovit("o_pocitaci");
            },
        );
    });
    radek(&karta, "\u{f0c7}", "Last backup", &posledni, Some(zaloha.upcast_ref()));
    let kontrola = tlacitko("Check", false, || {
        super::oznamit("Checking…");
        let skript = domov().join(".dotfiles/restore.sh").to_string_lossy().to_string();
        na_pozadi(
            move || prikaz("bash", &[&skript, "--check"]),
            |vystup| {
                let posledni = vystup.lines().filter(|r| !r.trim().is_empty()).last().unwrap_or("").to_string();
                let ok = posledni.contains("v pořádku");
                super::oznamit(if ok { "The backup has everything a restore needs" } else { "Something is missing from the backup – ask Claude to look at restore.sh --check" });
            },
        );
    });
    radek(&karta, "\u{f00c}", "Check the backup", "Makes sure a restore would have everything it needs", Some(kontrola.upcast_ref()));
}

/// Název grafiky z /usr/share/hwdata/pci.ids: „Intel HD Graphics 630“, „NVIDIA GeForce GTX 1050 Mobile“.
fn nazev_pci(vyrobce: &str, cislo: &str) -> String {
    let ids = std::fs::read_to_string("/usr/share/hwdata/pci.ids").unwrap_or_default();
    let mut firma = String::new();
    let mut v_sekci = false;
    for r in ids.lines() {
        if !r.starts_with('\t') && !r.starts_with('#') {
            v_sekci = r.starts_with(&format!("{vyrobce}  "));
            if v_sekci {
                firma = r[vyrobce.len()..].trim().split_whitespace().next().unwrap_or("").to_string();
            } else if !firma.is_empty() {
                break;
            }
        } else if v_sekci && !r.starts_with("\t\t") {
            if let Some(nazev) = r.trim_start().strip_prefix(&format!("{cislo}  ")) {
                let nazev = match (nazev.find('['), nazev.rfind(']')) {
                    (Some(a), Some(b)) if b > a => &nazev[a + 1..b],
                    _ => nazev,
                };
                return format!("{firma} {nazev}");
            }
        }
    }
    format!("{firma} graphics ({vyrobce}:{cislo})").trim().to_string()
}
