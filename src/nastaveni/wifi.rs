//! Stránka Wi-Fi: zapnutí, připojená síť (IP, automatické připojení, odpojit, zapomenout),
//! dostupné sítě (heslo přímo v řádku), uložené sítě a skrytá síť. Vše přes nmcli na pozadí;
//! nejdřív sítě z poslední prohlídky, pak čerstvé. Dostupných sítí se ukáže jen pár, zbytek
//! po kliknutí. Podrobnosti sítě (IP, DNS, heslo…) a podnikové sítě (eduroam) jsou v wifi_sit.rs.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use gtk::prelude::*;

use super::ui::*;
use super::wifi_sit;
use crate::spolecne::{na_pozadi, prikaz, wifi_zapnuta};

/// kolik dostupných sítí ukázat, než se seznam sbalí
const UKAZAT: usize = 8;

thread_local! {
    /// uživatel rozbalil celý seznam (platí, dokud se znovu neotevře stránka)
    static ROZBALENO: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Default)]
struct Sit {
    signal: i64,
    pripojeno: bool,
    zamek: bool,
    /// WPA2/3-Enterprise (eduroam, firemní sítě): jméno a heslo místo jednoho hesla
    podnikova: bool,
}

#[derive(Default)]
struct Stav {
    site: Vec<(String, Sit)>,
    /// uložená připojení Wi-Fi (název = SSID)
    ulozene: Vec<String>,
    /// připojená síť: (název připojení, zařízení)
    pripojeno: Option<(String, String)>,
    ip: String,
    automaticky: bool,
}

fn nacist(znovu: bool) -> Stav {
    if znovu {
        // během skenování vrací nmcli jen část sítí: spustit sken, počkat, pak číst
        prikaz("nmcli", &["dev", "wifi", "rescan"]);
        std::thread::sleep(Duration::from_secs(3));
    }
    let mut site: BTreeMap<String, Sit> = BTreeMap::new();
    for r in prikaz("nmcli", &["-t", "-e", "no", "-f", "IN-USE,SIGNAL,SECURITY,SSID", "dev", "wifi", "list", "--rescan", "no"]).lines() {
        let c: Vec<&str> = r.splitn(4, ':').collect();
        if c.len() == 4 && !c[3].is_empty() {
            let s = site.entry(c[3].to_string()).or_default();
            s.signal = s.signal.max(c[1].parse().unwrap_or(0));
            s.pripojeno |= c[0] == "*";
            s.zamek |= !c[2].is_empty() && c[2] != "--";
            s.podnikova |= c[2].contains("802.1X");
        }
    }
    let mut site: Vec<(String, Sit)> = site.into_iter().collect();
    site.sort_by_key(|(_, s)| (!s.pripojeno, -s.signal));
    let ulozene = prikaz("nmcli", &["-t", "-e", "no", "-f", "NAME,TYPE", "connection", "show"])
        .lines()
        .filter_map(|r| r.strip_suffix(":802-11-wireless").map(String::from))
        .collect();
    let pripojeno = prikaz("nmcli", &["-t", "-e", "no", "-f", "DEVICE,TYPE,STATE,CONNECTION", "dev"])
        .lines()
        .find_map(|r| {
            let c: Vec<&str> = r.splitn(4, ':').collect();
            (c.len() == 4 && c[1] == "wifi" && c[2] == "connected").then(|| (c[3].to_string(), c[0].to_string()))
        });
    let (ip, automaticky) = match &pripojeno {
        Some((spojeni, zarizeni)) => (
            prikaz("nmcli", &["-g", "IP4.ADDRESS", "dev", "show", zarizeni]).lines().next().unwrap_or("").split('/').next().unwrap_or("").to_string(),
            prikaz("nmcli", &["-g", "connection.autoconnect", "connection", "show", spojeni]).trim() == "yes",
        ),
        None => (String::new(), false),
    };
    Stav { site, ulozene, pripojeno, ip, automaticky }
}

/// nmcli na pozadí; po dokončení potvrzení dole v okně a znovu sestavit stránku.
pub fn nmcli(args: Vec<String>, hotovo: &'static str, chyba: &'static str) {
    na_pozadi(
        move || {
            let a: Vec<&str> = args.iter().map(String::as_str).collect();
            std::process::Command::new("nmcli").args(&a).output().map(|v| v.status.success()).unwrap_or(false)
        },
        move |ok| {
            super::oznamit(if ok { hotovo } else { chyba });
            super::obnovit("wifi");
        },
    );
}

fn ikona_signalu(signal: i64) -> &'static str {
    // Font Awesome má jen jednu ikonu Wi-Fi; síla je číslem vpravo
    let _ = signal;
    "\u{f1eb}"
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Wi-Fi");
    ROZBALENO.with(|r| r.set(false));
    let zapnuto = wifi_zapnuta();
    let hlavni = skupina(&s, "", "");
    radek_prepinac(&hlavni, "Wi-Fi", "Connect to wireless networks", zapnuto, |stav| {
        crate::spolecne::pozadi("nmcli", &["radio", "wifi", if stav { "on" } else { "off" }]);
        super::oznamit(if stav { "Wi-Fi on, looking for networks…" } else { "Wi-Fi off" });
        // po zapnutí chvíli trvá, než najde sítě
        gtk::glib::timeout_add_local_once(Duration::from_millis(if stav { 4000 } else { 400 }), || super::obnovit("wifi"));
    });
    if !zapnuto {
        return s.upcast();
    }

    let pripojena = skupina(&s, "Connected network", "");
    let dostupne = skupina(&s, "Available networks", "");
    let ulozene = skupina(&s, "Saved networks", "Networks this computer remembers and joins on its own.");
    let dalsi = skupina(&s, "", "");
    nacitani(&dostupne, "Looking for networks…");
    let skryta = {
        let dalsi = dalsi.clone();
        move || formular_skryte(&dalsi)
    };
    radek_akce(&dalsi, "\u{f067}", "Connect to a hidden network…", "A network that doesn't show its name", "", skryta);
    radek_akce(&dalsi, "\u{f19d}", "Connect to eduroam or a work network…", "Networks where you sign in with a username and password", "\u{f054}", || wifi_sit::podnikova(""));
    let k = dalsi.clone();
    radek_akce(&k, "\u{f2f1}", "Look for networks again", "", "", || {
        super::oznamit("Looking for networks…");
        na_pozadi(|| nacist(true), |_| super::obnovit("wifi"));
    });

    let karty = Rc::new((pripojena, dostupne, ulozene));
    // nejdřív sítě z poslední prohlídky (hned), pak čerstvé (pár sekund)
    let k = karty.clone();
    na_pozadi(|| nacist(false), move |stav| {
        ukazat(&k, &stav);
        let k = k.clone();
        na_pozadi(|| nacist(true), move |stav| ukazat(&k, &stav));
    });
    s.upcast()
}

fn ukazat(karty: &Rc<(gtk::ListBox, gtk::ListBox, gtk::ListBox)>, stav: &Stav) {
    if !super::otevreno() || karty.0.root().is_none() {
        return; // stránka mezitím zmizela
    }
    let (pripojena, dostupne, ulozene) = (&karty.0, &karty.1, &karty.2);

    vycistit(pripojena);
    match &stav.pripojeno {
        Some((spojeni, _)) => {
            let signal = stav.site.iter().find(|(n, s)| s.pripojeno || n == spojeni).map(|(_, s)| s.signal).unwrap_or(0);
            let tlacitka = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let sp = spojeni.clone();
            tlacitka.append(&tlacitko("Disconnect", false, move || {
                nmcli(vec!["connection".into(), "down".into(), "id".into(), sp.clone()], "Disconnected", "Couldn't disconnect");
            }));
            let sp = spojeni.clone();
            tlacitka.append(&tlacitko("Details", false, move || wifi_sit::podrobnosti(&sp)));
            let popis = if stav.ip.is_empty() { format!("Signal {signal} %") } else { format!("Signal {signal} %  ·  IP address {}", stav.ip) };
            radek(pripojena, ikona_signalu(signal), spojeni, &popis, Some(tlacitka.upcast_ref()));
            let sp = spojeni.clone();
            radek_prepinac(pripojena, "Connect automatically", "Join this network whenever it is in range", stav.automaticky, move |a| {
                nmcli(
                    vec!["connection".into(), "modify".into(), "id".into(), sp.clone(), "connection.autoconnect".into(), if a { "yes" } else { "no" }.into()],
                    if a { "Will connect automatically" } else { "Won't connect automatically" },
                    "Couldn't change the setting",
                );
            });
        }
        None => {
            radek(pripojena, "", "Not connected", "Pick a network below", None);
        }
    }

    vycistit(dostupne);
    let viditelne: Vec<&(String, Sit)> = stav.site.iter().filter(|(_, s)| !s.pripojeno).collect();
    if viditelne.is_empty() {
        radek(dostupne, "", "No other networks found", "", None);
    }
    let rozbaleno = ROZBALENO.with(|r| r.get());
    let mut schovane = Vec::new();
    for (i, (ssid, sit)) in viditelne.iter().map(|x| (&x.0, &x.1)).enumerate() {
        let ulozena = stav.ulozene.contains(ssid);
        let popis = match (sit.zamek, ulozena, sit.podnikova) {
            (_, true, _) => "Saved",
            (_, false, true) => "Secured with a username (e.g. eduroam)",
            (true, false, false) => "Secured",
            (false, false, false) => "Open",
        };
        let vpravo = format!("{}{} %", if sit.zamek { "\u{f023}  " } else { "" }, sit.signal);
        let (ssid2, zamek, podnikova) = (ssid.clone(), sit.zamek, sit.podnikova);
        let karta = dostupne.clone();
        let row = radek_akce(dostupne, ikona_signalu(sit.signal), ssid, popis, &vpravo, || {});
        if i >= UKAZAT && !rozbaleno {
            row.set_visible(false);
            schovane.push(row.clone());
        }
        // klik: uložená / otevřená síť hned, zabezpečená nová se zeptá na heslo pod řádkem
        let tlacitko_radku = row.child().and_downcast::<gtk::Button>();
        if let Some(b) = tlacitko_radku {
            let row2 = row.clone();
            b.connect_clicked(move |_| {
                if ulozena {
                    super::oznamit(&format!("Connecting to {ssid2}…"));
                    nmcli(vec!["connection".into(), "up".into(), "id".into(), ssid2.clone()], "Connected", "Couldn't connect – is the network in range?");
                } else if podnikova {
                    wifi_sit::podnikova(&ssid2);
                } else if !zamek {
                    super::oznamit(&format!("Connecting to {ssid2}…"));
                    nmcli(vec!["dev".into(), "wifi".into(), "connect".into(), ssid2.clone()], "Connected", "Couldn't connect");
                } else {
                    heslo_pod(&karta, &row2, &ssid2);
                }
            });
        }
    }
    if !schovane.is_empty() {
        let pocet = schovane.len();
        let rozbalit = radek_akce(dostupne, "\u{f078}", &format!("Show {pocet} more networks"), "", "", || {});
        let r2 = rozbalit.clone();
        if let Some(b) = rozbalit.child().and_downcast::<gtk::Button>() {
            b.connect_clicked(move |_| {
                ROZBALENO.with(|r| r.set(true));
                for r in &schovane {
                    r.set_visible(true);
                }
                r2.set_visible(false);
            });
        }
    }

    vycistit(ulozene);
    let mut ul: Vec<&String> = stav.ulozene.iter().filter(|n| stav.pripojeno.as_ref().map(|p| &p.0) != Some(*n)).collect();
    ul.sort();
    if ul.is_empty() {
        radek(ulozene, "", "No other saved networks", "", None);
    }
    for nazev in ul {
        let n = nazev.clone();
        let v_dosahu = stav.site.iter().any(|(s, _)| s == nazev);
        radek_akce(ulozene, "\u{f1eb}", nazev, if v_dosahu { "In range" } else { "Not in range" }, "\u{f054}", move || wifi_sit::podrobnosti(&n));
    }
}

/// Pole na heslo hned pod řádkem sítě.
fn heslo_pod(karta: &gtk::ListBox, row: &gtk::ListBoxRow, ssid: &str) {
    // jen jedno pole najednou
    let mut r = karta.first_child();
    while let Some(w) = r {
        r = w.next_sibling();
        if w.widget_name() == "heslo" {
            karta.remove(&w);
        }
    }
    let pole = gtk::PasswordEntry::new();
    pole.set_show_peek_icon(true);
    pole.set_hexpand(true);
    pole.set_property("placeholder-text", format!("Password for {ssid}"));
    let pripojit = {
        let (pole, ssid) = (pole.clone(), ssid.to_string());
        move || {
            let heslo = pole.text().to_string();
            if heslo.is_empty() {
                return;
            }
            super::oznamit(&format!("Connecting to {ssid}…"));
            nmcli(
                vec!["dev".into(), "wifi".into(), "connect".into(), ssid.clone(), "password".into(), heslo],
                "Connected",
                "Couldn't connect – wrong password or the network is out of range",
            );
        }
    };
    let p2 = pripojit.clone();
    pole.connect_activate(move |_| p2());
    let obal = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    obal.add_css_class("radek");
    obal.append(&pole);
    obal.append(&tlacitko("Connect", true, pripojit));
    let novy = gtk::ListBoxRow::new();
    novy.set_widget_name("heslo");
    novy.set_activatable(false);
    novy.set_child(Some(&obal));
    karta.insert(&novy, row.index() + 1);
    pole.grab_focus();
}

/// Skrytá síť: název a heslo.
fn formular_skryte(karta: &gtk::ListBox) {
    let mut r = karta.first_child();
    while let Some(w) = r {
        r = w.next_sibling();
        if w.widget_name() == "skryta" {
            return; // už je otevřený
        }
    }
    let nazev = gtk::Entry::new();
    nazev.set_placeholder_text(Some("Network name"));
    nazev.set_hexpand(true);
    let heslo = gtk::PasswordEntry::new();
    heslo.set_show_peek_icon(true);
    heslo.set_hexpand(true);
    heslo.set_property("placeholder-text", "Password (empty for an open network)");
    let pripojit = {
        let (nazev, heslo) = (nazev.clone(), heslo.clone());
        move || {
            let n = nazev.text().to_string();
            if n.is_empty() {
                return;
            }
            let mut a: Vec<String> = vec!["dev".into(), "wifi".into(), "connect".into(), n.clone(), "hidden".into(), "yes".into()];
            let h = heslo.text().to_string();
            if !h.is_empty() {
                a.extend(["password".to_string(), h]);
            }
            super::oznamit(&format!("Connecting to {n}…"));
            nmcli(a, "Connected", "Couldn't connect – check the name and password");
        }
    };
    let p2 = pripojit.clone();
    heslo.connect_activate(move |_| p2());
    let obal = gtk::Box::new(gtk::Orientation::Vertical, 8);
    obal.add_css_class("radek");
    obal.append(&nazev);
    let dole = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    dole.append(&heslo);
    dole.append(&tlacitko("Connect", true, pripojit));
    obal.append(&dole);
    let row = gtk::ListBoxRow::new();
    row.set_widget_name("skryta");
    row.set_activatable(false);
    row.set_child(Some(&obal));
    karta.insert(&row, 1);
    nazev.grab_focus();
}
