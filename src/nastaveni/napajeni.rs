//! Stránka Power & Battery: stav a opotřebení baterie, režim napájení (tuned přes power-mode.sh),
//! conservation mode Lenova (heslo přes polkit, žádný terminál) a za jak dlouho zamknout
//! a zhasnout obrazovku (~/.config/sway/idle.conf, swayidle přes idle.sh).

use std::io::Write;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use super::ui::*;
use crate::spolecne::{baterie, domov, na_pozadi, pozadi, rezim_napajeni, skripty};

const BAT: &str = "/sys/class/power_supply/BAT0";
const SETRENI: &str = "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/conservation_mode";

fn cist(cesta: &str) -> Option<f64> {
    std::fs::read_to_string(cesta).ok()?.trim().parse().ok()
}

fn doba(s: i64) -> String {
    if s >= 3600 { format!("{} h {} min", s / 3600, s / 60 % 60) } else { format!("{} min", s / 60) }
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Power & Battery");

    // baterie
    let karta = skupina(&s, "Battery", "");
    if let Some(b) = baterie() {
        let stav = match b.stav {
            1 => "Charging",
            2 | 6 => "On battery",
            4 => "Fully charged",
            5 => "Plugged in, not charging",
            _ => "",
        };
        let mut popis = vec![stav.to_string()];
        let zbyva = if b.stav == 2 { b.do_vybiti } else { b.do_nabiti };
        if zbyva > 0 {
            popis.push(format!("{} {}", if b.stav == 2 { "empty in" } else { "full in" }, doba(zbyva)));
        }
        let uroven = gtk::LevelBar::for_interval(0.0, 100.0);
        uroven.set_mode(gtk::LevelBarMode::Continuous);
        for n in ["low", "high", "full"] {
            uroven.remove_offset_value(Some(n));
        }
        uroven.set_value(b.procenta);
        uroven.set_size_request(160, -1);
        radek(&karta, "\u{f240}", &format!("{:.0} %", b.procenta), &popis.join("  ·  "), Some(uroven.upcast_ref()));
        if b.prikon > 0.0 {
            let (nazev, popis) = if b.stav == 1 { ("Charging speed", format!("{:.1} W into the battery", b.prikon)) } else { ("Power draw", format!("{:.1} W right now", b.prikon)) };
            radek(&karta, "\u{f0e7}", nazev, &popis, None);
        }
        if let (Some(plna), Some(puvodni)) = (cist(&format!("{BAT}/energy_full")), cist(&format!("{BAT}/energy_full_design"))) {
            let zdravi = plna / puvodni * 100.0;
            let rada = if zdravi < 80.0 { "  ·  Battery care below helps it last longer" } else { "" };
            radek(&karta, "\u{f21e}", &format!("Battery health {zdravi:.0} %"), &format!("Holds {:.0} Wh of the original {:.0} Wh{rada}", plna / 1e6, puvodni / 1e6), None);
        }
    } else {
        radek(&karta, "", "No battery found", "", None);
    }

    // režim napájení
    let karta = skupina(&s, "Power mode", "On battery, Balanced saves power by itself.");
    let aktualni = rezim_napajeni();
    for (rezim, nazev, popis) in [
        ("power-saver", "Power saver", "Longest battery life, slower"),
        ("balanced", "Balanced", "Recommended"),
        ("performance", "Performance", "Fastest, uses more battery and runs warmer"),
    ] {
        let vybrany = rezim == aktualni;
        radek_akce(&karta, if vybrany { "\u{f00c}" } else { "" }, nazev, popis, "", move || {
            pozadi(&skripty().join("power-mode.sh").to_string_lossy(), &[rezim]);
            super::oznamit(&format!("Power mode: {nazev}"));
            glib::timeout_add_local_once(Duration::from_millis(500), || super::obnovit("napajeni"));
        });
    }

    // šetrné nabíjení (jen notebooky Lenovo s ideapad_acpi)
    if let Some(zapnuto) = cist(SETRENI) {
        let karta = skupina(&s, "Battery care", "");
        radek_prepinac(
            &karta,
            "Conservation mode",
            "Charges only to about 60 %. Good if the laptop is mostly plugged in; turn it off before a trip for a full charge.",
            zapnuto > 0.0,
            |stav| {
                super::oznamit("Waiting for your password…");
                na_pozadi(
                    move || {
                        // zápis do /sys potřebuje roota: pkexec se zeptá na heslo běžným dialogem
                        std::process::Command::new("pkexec")
                            .args(["/usr/bin/tee", SETRENI])
                            .stdin(std::process::Stdio::piped())
                            .stdout(std::process::Stdio::null())
                            .spawn()
                            .and_then(|mut p| {
                                if let Some(mut vstup) = p.stdin.take() {
                                    vstup.write_all(if stav { b"1" } else { b"0" })?;
                                }
                                p.wait()
                            })
                            .map(|s| s.success())
                            .unwrap_or(false)
                    },
                    move |ok| {
                        super::oznamit(match (ok, stav) {
                            (true, true) => "Conservation mode on – charging stops at about 60 %",
                            (true, false) => "Conservation mode off – charging to 100 %",
                            (false, _) => "Not changed",
                        });
                        super::obnovit("napajeni");
                    },
                );
            },
        );
    }

    // nečinnost
    let karta = skupina(&s, "When idle", "Not while a video or music is playing. The screen dims 20 seconds before it locks.");
    let (zamknout, vypnout) = necinnost();
    let volby: [(&str, u32); 6] = [("2 minutes", 120), ("5 minutes", 300), ("10 minutes", 600), ("15 minutes", 900), ("30 minutes", 1800), ("Never", 0)];
    let vyber = |aktualni: u32, zmena: fn(u32)| {
        let d = gtk::DropDown::from_strings(&volby.iter().map(|v| v.0).collect::<Vec<_>>());
        d.set_selected(volby.iter().position(|v| v.1 == aktualni).unwrap_or(2) as u32);
        d.connect_selected_notify(move |d| zmena(volby[d.selected() as usize].1));
        d
    };
    radek(&karta, "\u{f023}", "Lock the screen after", "", Some(vyber(zamknout, |v| ulozit_necinnost(Some(v), None)).upcast_ref()));
    radek(&karta, "\u{f108}", "Turn the screen off after", "", Some(vyber(vypnout, |v| ulozit_necinnost(None, Some(v))).upcast_ref()));
    s.upcast()
}

fn soubor_necinnosti() -> std::path::PathBuf {
    domov().join(".config/sway/idle.conf")
}

/// (zamknout, vypnout obrazovku) v sekundách, 0 = nikdy
fn necinnost() -> (u32, u32) {
    let t = std::fs::read_to_string(soubor_necinnosti()).unwrap_or_default();
    let hodnota = |k: &str, vychozi: u32| t.lines().find_map(|r| r.strip_prefix(&format!("{k}="))).and_then(|v| v.trim().parse().ok()).unwrap_or(vychozi);
    (hodnota("zamknout", 600), hodnota("vypnout", 900))
}

fn ulozit_necinnost(zamknout: Option<u32>, vypnout: Option<u32>) {
    let (z, v) = necinnost();
    let (z, v) = (zamknout.unwrap_or(z), vypnout.unwrap_or(v));
    let text = format!("# nečinnost v sekundách (0 = nikdy), mění aplikace Settings; viz scripts/idle.sh\nzamknout={z}\nvypnout={v}\n");
    if std::fs::write(soubor_necinnosti(), text).is_ok() {
        // swayidle znovu s novými časy (v sezení Swaye, ne jako dítě démona)
        crate::sway::prikaz(&format!("exec {}", skripty().join("idle.sh").to_string_lossy()));
        super::oznamit("Saved");
    } else {
        super::oznamit("Couldn't save the setting");
    }
}
