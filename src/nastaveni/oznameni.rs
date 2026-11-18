//! Stránka Notifications: Nerušit (ručně i podle času), historie a ztlumení jednotlivých aplikací
//! (jejich oznámení jdou jen do historie, bez bubliny).

use gtk::prelude::*;
use serde_json::json;

use super::ui::*;
use crate::spolecne::{nastaveni, ulozit_nastaveni};

fn cas(minuty: u64) -> String {
    format!("{:02}:{:02}", minuty / 60, minuty % 60)
}

/// Výběr času po půl hodině.
fn vyber_casu(minuty: u64, zmena: impl Fn(u64) + 'static) -> gtk::DropDown {
    let casy: Vec<String> = (0..48).map(|i| cas(i * 30)).collect();
    let d = gtk::DropDown::from_strings(&casy.iter().map(String::as_str).collect::<Vec<_>>());
    d.set_selected((minuty / 30) as u32);
    d.connect_selected_notify(move |d| zmena(d.selected() as u64 * 30));
    d
}

fn ulozit_cas(klic: &str, hodnota: serde_json::Value) {
    let mut c = nastaveni("dnd_cas");
    if !c.is_object() {
        c = json!({"zapnuto": false, "od": 22 * 60, "do": 7 * 60});
    }
    c[klic] = hodnota;
    ulozit_nastaveni("dnd_cas", c);
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Notifications");

    let karta = skupina(&s, "", "");
    radek_prepinac(&karta, "Do Not Disturb", "Notifications go quietly to the list (Super + N); urgent ones still show", crate::upozorneni::dnd(), |z| {
        crate::okenka::prikaz("dnd", if z { "on" } else { "off" });
    });
    let c = nastaveni("dnd_cas");
    let zapnuto = c["zapnuto"].as_bool().unwrap_or(false);
    radek_prepinac(&karta, "Turn on Do Not Disturb at night", "", zapnuto, |z| {
        ulozit_cas("zapnuto", json!(z));
        super::oznamit("Saved");
        super::obnovit("oznameni");
    });
    if zapnuto {
        radek(&karta, "\u{f186}", "From", "", Some(vyber_casu(c["od"].as_u64().unwrap_or(22 * 60), |m| ulozit_cas("od", json!(m))).upcast_ref()));
        radek(&karta, "\u{f185}", "Until", "", Some(vyber_casu(c["do"].as_u64().unwrap_or(7 * 60), |m| ulozit_cas("do", json!(m))).upcast_ref()));
    }

    let karta = skupina(&s, "Apps", "Turn an app off to keep its notifications in the list without popping up.");
    let aplikace = crate::upozorneni::aplikace_v_historii();
    let mut vsechny = aplikace.clone();
    for a in nastaveni("upozorneni_ztlumene").as_array().into_iter().flatten().filter_map(|v| v.as_str()) {
        if !vsechny.iter().any(|x| x == a) {
            vsechny.push(a.to_string());
        }
    }
    if vsechny.is_empty() {
        radek(&karta, "", "No app has sent a notification yet", "", None);
    }
    for a in vsechny {
        let ztlumena = crate::upozorneni::ztlumena(&a);
        let jmeno = a.clone();
        radek_prepinac(&karta, &a, "", !ztlumena, move |z| {
            let mut v: Vec<String> = nastaveni("upozorneni_ztlumene").as_array().map(|x| x.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
            v.retain(|x| *x != jmeno);
            if !z {
                v.push(jmeno.clone());
            }
            ulozit_nastaveni("upozorneni_ztlumene", json!(v));
        });
    }

    let karta = skupina(&s, "", "");
    let pocet = crate::upozorneni::historie().len();
    let smazat = tlacitko("Clear", false, || {
        crate::upozorneni::smazat_vse();
        super::oznamit("Notifications cleared");
        super::obnovit("oznameni");
    });
    smazat.set_sensitive(pocet > 0);
    radek(&karta, "\u{f0f3}", "Notification list", &format!("{pocet} saved"), Some(smazat.upcast_ref()));
    s.upcast()
}
