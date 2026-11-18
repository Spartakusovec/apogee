//! Stránka Date & Time: automatický čas, časové pásmo, formát hodin (odkaz na Panel & Dock)
//! a formáty data, čísel a jednotek (localectl, změna se projeví po novém přihlášení).
//! Změny systému jdou přes timedatectl / localectl, které se zeptají na heslo (polkit).

use std::collections::HashMap;

use gtk::prelude::*;

use super::ui::*;
use crate::spolecne::{na_pozadi, prikaz};

/// formáty: (název, locale)
const FORMATY: &[(&str, &str)] = &[
    ("Czech – 18. 11. 2026, 1 234,5", "cs_CZ.UTF-8"),
    ("English (US) – 11/18/2026, 1,234.5", "en_US.UTF-8"),
    ("English (UK) – 18/11/2026, 1,234.5", "en_GB.UTF-8"),
    ("Slovak – 18. 11. 2026, 1 234,5", "sk_SK.UTF-8"),
];
const KATEGORIE: &[&str] = &["LC_TIME", "LC_NUMERIC", "LC_MONETARY", "LC_MEASUREMENT", "LC_PAPER"];

fn timedatectl() -> HashMap<String, String> {
    prikaz("timedatectl", &["show"]).lines().filter_map(|r| r.split_once('=').map(|(a, b)| (a.to_string(), b.to_string()))).collect()
}

/// Systémový příkaz na pozadí (polkit se zeptá na heslo), pak potvrzení a nové sestavení.
fn spustit(program: &'static str, args: Vec<String>, hotovo: &'static str) {
    super::oznamit("Waiting for your password…");
    na_pozadi(
        move || std::process::Command::new(program).args(&args).output().map(|v| v.status.success()).unwrap_or(false),
        move |ok| {
            super::oznamit(if ok { hotovo } else { "Not changed" });
            super::obnovit("cas");
        },
    );
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Date & Time");
    let t = timedatectl();

    let karta = skupina(&s, "", "");
    let ted = chrono::Local::now();
    radek(&karta, "\u{f017}", &ted.format("%H:%M").to_string(), &ted.format("%A %e %B %Y").to_string(), None);
    radek_prepinac(&karta, "Set the time automatically", "From the internet", t.get("NTP").is_some_and(|v| v == "yes"), |z| {
        spustit("timedatectl", vec!["set-ntp".into(), z.to_string()], "Saved");
    });
    let pasma: Vec<String> = prikaz("timedatectl", &["list-timezones"]).lines().map(String::from).collect();
    let pasmo = t.get("Timezone").cloned().unwrap_or_default();
    let d = gtk::DropDown::from_strings(&pasma.iter().map(|p| p.replace('_', " ")).collect::<Vec<_>>().iter().map(String::as_str).collect::<Vec<_>>());
    d.set_enable_search(true);
    d.set_expression(Some(gtk::PropertyExpression::new(gtk::StringObject::static_type(), None::<gtk::Expression>, "string")));
    d.set_selected(pasma.iter().position(|p| *p == pasmo).map(|p| p as u32).unwrap_or(gtk::INVALID_LIST_POSITION));
    d.connect_selected_notify(move |d| {
        if let Some(p) = pasma.get(d.selected() as usize) {
            if *p != pasmo {
                spustit("timedatectl", vec!["set-timezone".into(), p.clone()], "Time zone changed");
            }
        }
    });
    radek(&karta, "\u{f57d}", "Time zone", "", Some(d.upcast_ref()));
    radek_akce(&karta, "\u{f2f2}", "Clock in the panel", "Date, seconds, 12-hour time", "\u{f054}", || super::otevrit("panel_dok"));

    // poloha: počasí a noční režim (východ a západ slunce)
    let karta = skupina(&s, "Location", "For the weather and for night light at sunset. Search for your town.");
    let ted = crate::okenka::pocasi::poloha().map(|p| p.0).unwrap_or_else(|| "Not set".into());
    radek(&karta, "\u{f3c5}", &ted, "", None);
    let hledat = gtk::Entry::new();
    hledat.set_placeholder_text(Some("Town or city"));
    hledat.set_width_chars(22);
    let vysledky = karta.clone();
    let najit = {
        let hledat = hledat.clone();
        move || {
            let dotaz = hledat.text().trim().to_string();
            if dotaz.is_empty() {
                return;
            }
            super::oznamit("Searching…");
            let karta = vysledky.clone();
            na_pozadi(
                move || {
                    let url = format!(
                        "https://geocoding-api.open-meteo.com/v1/search?count=6&language=en&format=json&name={}",
                        dotaz.replace(' ', "+")
                    );
                    serde_json::from_str::<serde_json::Value>(&prikaz("curl", &["-sf", "--max-time", "10", &url]))
                        .ok()
                        .and_then(|v| v["results"].as_array().cloned())
                        .unwrap_or_default()
                },
                move |mista| {
                    // staré výsledky pryč (řádky za řádkem s hledáním)
                    while let Some(r) = karta.row_at_index(2) {
                        karta.remove(&r);
                    }
                    if mista.is_empty() {
                        super::oznamit("Nothing found – check the spelling or the internet connection");
                    }
                    for m in mista {
                        let nazev = m["name"].as_str().unwrap_or("").to_string();
                        let kde = [m["admin1"].as_str(), m["country"].as_str()].into_iter().flatten().collect::<Vec<_>>().join(", ");
                        let (sirka, delka) = (m["latitude"].as_f64().unwrap_or(0.0), m["longitude"].as_f64().unwrap_or(0.0));
                        let n = nazev.clone();
                        radek_akce(&karta, "", &nazev, &kde, "Use", move || {
                            crate::spolecne::ulozit_nastaveni("poloha", serde_json::json!({"nazev": n, "sirka": sirka, "delka": delka}));
                            crate::okenka::pocasi::zmenit_polohu();
                            // noční režim si polohu přečte při novém spuštění
                            crate::okenka::jas::ulozit("poloha", &format!("{sirka:.3},{delka:.3}"));
                            super::oznamit(&format!("Location: {n}"));
                            super::obnovit("cas");
                        });
                    }
                },
            );
        }
    };
    let n2 = najit.clone();
    hledat.connect_activate(move |_| n2());
    let obal = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    obal.append(&hledat);
    obal.append(&tlacitko("Search", false, najit));
    radek(&karta, "\u{f002}", "Change", "", Some(obal.upcast_ref()));

    // formáty
    let karta = skupina(&s, "Formats", "Dates, numbers, money and units in apps. The menus stay in English. Applies after you log in again.");
    let locale = prikaz("localectl", &["status"]);
    let ted = locale.split_whitespace().find_map(|w| w.strip_prefix("LC_TIME=")).or_else(|| locale.split_whitespace().find_map(|w| w.strip_prefix("LANG="))).unwrap_or("en_US.UTF-8").to_string();
    let d = gtk::DropDown::from_strings(&FORMATY.iter().map(|f| f.0).collect::<Vec<_>>());
    d.set_selected(FORMATY.iter().position(|f| f.1.eq_ignore_ascii_case(&ted) || f.1.replace("UTF-8", "utf8") == ted).unwrap_or(1) as u32);
    d.connect_selected_notify(move |d| {
        let l = FORMATY[d.selected() as usize].1;
        let mut a = vec!["set-locale".to_string(), "LANG=en_US.UTF-8".to_string()];
        a.extend(KATEGORIE.iter().map(|k| format!("{k}={l}")));
        spustit("localectl", a, "Saved – log out and back in to see it");
    });
    radek(&karta, "\u{f1ab}", "Region", "", Some(d.upcast_ref()));
    if t.get("LocalRTC").is_some_and(|v| v == "yes") {
        radek(&karta, "\u{f05a}", "Hardware clock keeps local time", "Usual when Windows is installed on the same computer", None);
    }
    s.upcast()
}
