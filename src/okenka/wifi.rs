//! Wi-Fi: zapnutí, seznam sítí (nejsilnějších 10), připojení – u nové sítě s heslem přímo v okénku.

use std::collections::{BTreeMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::spolecne::*;

#[derive(Clone, Default)]
struct Sit {
    signal: i64,
    pripojeno: bool,
    zamek: bool,
}

type Site = (BTreeMap<String, Sit>, HashSet<String>);

fn nacist(znovu: bool) -> Site {
    if znovu {
        // během skenování vrací nmcli jen část sítí: spustit sken, počkat na něj, pak číst
        prikaz("nmcli", &["dev", "wifi", "rescan"]);
        std::thread::sleep(Duration::from_secs(3));
    }
    let mut site: BTreeMap<String, Sit> = BTreeMap::new();
    for radek in prikaz("nmcli", &["-t", "-e", "no", "-f", "IN-USE,SIGNAL,SECURITY,SSID", "dev", "wifi", "list", "--rescan", "no"]).lines() {
        let casti: Vec<&str> = radek.splitn(4, ':').collect();
        if casti.len() == 4 && !casti[3].is_empty() {
            let s = site.entry(casti[3].to_string()).or_default();
            s.signal = s.signal.max(casti[1].parse().unwrap_or(0));
            s.pripojeno |= casti[0] == "*";
            s.zamek |= !casti[2].is_empty() && casti[2] != "--";
        }
    }
    let ulozene = prikaz("nmcli", &["-t", "-e", "no", "-f", "NAME,TYPE", "connection", "show"])
        .lines()
        .filter_map(|r| r.strip_suffix(":802-11-wireless").map(String::from))
        .collect();
    (site, ulozene)
}

struct Wifi {
    o: Rc<Okenko>,
    seznam: gtk::Box,
    heslo_box: gtk::Box,
}

pub fn naplnit(o: &Rc<Okenko>) {
    let zapnuto = wifi_zapnuta();
    let seznam = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let heslo_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let w = Rc::new(Wifi { o: o.clone(), seznam: seznam.clone(), heslo_box: heslo_box.clone() });

    let w2 = w.clone();
    o.panel.append(&hlavicka_s_prepinacem("Wi-Fi", zapnuto, move |stav| {
        pozadi("nmcli", &["radio", "wifi", if stav { "on" } else { "off" }]);
        w2.seznam.set_visible(stav);
        if stav {
            // po zapnutí chvilku trvá, než Wi-Fi sítě najde
            let w3 = w2.clone();
            glib::timeout_add_local_once(Duration::from_millis(2500), move || w3.nacist(true));
        }
    }));
    o.panel.append(&seznam);
    o.panel.append(&heslo_box);
    o.panel.append(&oddelovac());
    let dole = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let nastaveni = odkaz_nastaveni(o, "Wi-Fi settings…", "wifi");
    nastaveni.set_hexpand(true);
    dole.append(&nastaveni);
    let obnovit = gtk::Button::with_label("\u{f2f1}  Refresh");
    obnovit.add_css_class("odkaz");
    let w2 = w.clone();
    obnovit.connect_clicked(move |_| w2.nacist(true));
    dole.append(&obnovit);
    o.panel.append(&dole);

    seznam.set_visible(zapnuto);
    // sítě z poslední prohlídky se ukážou hned, čerstvé po novém hledání
    w.nacist(false);
    if zapnuto {
        w.nacist(true);
    }
}

impl Wifi {
    fn nacist(self: &Rc<Self>, znovu: bool) {
        let w = self.clone();
        na_pozadi(move || nacist(znovu), move |data| w.ukazat(data));
    }

    fn ukazat(self: &Rc<Self>, (site, ulozene): Site) {
        // hledání sítí trvá i pár sekund, okénko mezitím mohlo zavřít
        if self.o.zavreno() {
            return;
        }
        while let Some(d) = self.seznam.first_child() {
            self.seznam.remove(&d);
        }
        let mut serazene: Vec<(String, Sit)> = site.into_iter().collect();
        serazene.sort_by_key(|(_, s)| (!s.pripojeno, -s.signal));
        for (ssid, s) in serazene.into_iter().take(10) {
            let vpravo = format!(
                "{}{}{} %",
                if s.pripojeno { "\u{f00c}  " } else { "" },
                if s.zamek { "\u{f023}  " } else { "" },
                s.signal
            );
            let w = self.clone();
            let ulozena = ulozene.contains(&ssid);
            let (ssid2, s2) = (ssid.clone(), s.clone());
            self.seznam.append(&radek("\u{f1eb}", &ssid, &vpravo, s.pripojeno, move || {
                w.pripojit(&ssid2, &s2, ulozena, None)
            }));
        }
        self.o.prepocitat();
    }

    fn pripojit(self: &Rc<Self>, ssid: &str, s: &Sit, ulozena: bool, heslo: Option<String>) {
        if s.pripojeno {
            return;
        }
        if s.zamek && !ulozena && heslo.is_none() {
            return self.zeptat_na_heslo(ssid, s);
        }
        self.o.stav(&format!("Connecting to {ssid}…"));
        let ssid_vlakno = ssid.to_string();
        let ssid_hotovo = ssid.to_string();
        let o = self.o.clone();
        na_pozadi(
            move || {
                let mut c = std::process::Command::new("nmcli");
                if ulozena && heslo.is_none() {
                    c.args(["connection", "up", "id", &ssid_vlakno]);
                } else {
                    c.args(["dev", "wifi", "connect", &ssid_vlakno]);
                    if let Some(h) = &heslo {
                        c.args(["password", h]);
                    }
                }
                c.output().map(|v| v.status.success()).unwrap_or(false)
            },
            move |ok| {
                if ok {
                    o.stav(&format!("Connected to {ssid_hotovo}"));
                    let o2 = o.clone();
                    glib::timeout_add_local_once(Duration::from_millis(900), move || o2.zavrit());
                } else {
                    o.stav("Couldn't connect – wrong password or the network is out of range");
                }
            },
        );
    }

    fn zeptat_na_heslo(self: &Rc<Self>, ssid: &str, s: &Sit) {
        while let Some(d) = self.heslo_box.first_child() {
            self.heslo_box.remove(&d);
        }
        self.o.drzet.set(true);
        self.heslo_box.append(&popisek(&format!("Password for {ssid}"), &["dim"]));
        let radek = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let pole = gtk::Entry::new();
        pole.set_visibility(false);
        pole.set_placeholder_text(Some("Password"));
        pole.set_hexpand(true);
        let tlacitko = gtk::Button::with_label("Connect");
        tlacitko.add_css_class("zapnuto");
        radek.append(&pole);
        radek.append(&tlacitko);
        self.heslo_box.append(&radek);
        let odeslat = {
            let (w, ssid, s, pole) = (self.clone(), ssid.to_string(), s.clone(), pole.clone());
            move || w.pripojit(&ssid, &s, false, Some(pole.text().to_string()))
        };
        let o2 = odeslat.clone();
        pole.connect_activate(move |_| o2());
        tlacitko.connect_clicked(move |_| odeslat());
        pole.grab_focus();
        self.o.prepocitat();
    }
}
