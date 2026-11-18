//! Notifikace (dřív mako): démon je notifikační server (org.freedesktop.Notifications), ukazuje
//! bubliny vpravo nahoře (bubliny.rs) a vede historii pro okénko pod zvonečkem.
//!
//! Do Not Disturb: bubliny se neukazují (kromě kritických), notifikace jdou jen do historie.
//! Historie (nejvýš 50) se ukládá do ~/.cache/apogee/notifications.json, přežije restart démona.
//! Obrázky z notifikací (image-data) se ukládají jako PNG do ~/.cache/apogee/notifications/.

pub mod bubliny;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;
use serde_json::{json, Value};

use crate::spolecne::*;

const JMENO: &str = "org.freedesktop.Notifications";
const CESTA: &str = "/org/freedesktop/Notifications";
const NEJVIC_V_HISTORII: usize = 50;
const VYCHOZI_DOBA_MS: u32 = 6000;

const XML: &str = r#"<node><interface name="org.freedesktop.Notifications">
<method name="GetCapabilities"><arg type="as" direction="out"/></method>
<method name="Notify">
 <arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="s" direction="in"/><arg type="s" direction="in"/>
 <arg type="s" direction="in"/><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/><arg type="i" direction="in"/>
 <arg type="u" direction="out"/>
</method>
<method name="CloseNotification"><arg type="u" direction="in"/></method>
<method name="GetServerInformation">
 <arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/><arg type="s" direction="out"/>
</method>
<signal name="NotificationClosed"><arg type="u"/><arg type="u"/></signal>
<signal name="ActionInvoked"><arg type="u"/><arg type="s"/></signal>
</interface></node>"#;

/// Proč notifikace zmizela (číslo podle specifikace)
#[derive(Clone, Copy)]
pub enum Duvod {
    Vyprsela = 1,
    Zavrena = 2,
    Aplikaci = 3,
}

#[derive(Clone)]
pub struct Upozorneni {
    pub id: u32,
    pub aplikace: String,
    /// název ikony nebo cesta k obrázku
    pub ikona: String,
    pub desktop: String,
    pub nadpis: String,
    pub telo: String,
    /// (klíč, popisek); „default“ = klik na bublinu
    pub akce: Vec<(String, String)>,
    /// 0 nízká, 1 normální, 2 kritická
    pub nalehavost: u8,
    pub cas: f64,
    pub precteno: bool,
    /// jen bublina, do historie ne (hint transient)
    pub docasna: bool,
    /// průběh 0–100 (hint value)
    pub prubeh: Option<i32>,
    /// x-canonical-private-synchronous / x-dunst-stack-tag: nahradí starší se stejnou značkou
    pub znacka: Option<String>,
    /// za kolik ms bublina zmizí (0 = nezmizí)
    pub doba_ms: u32,
}

thread_local! {
    static HISTORIE: RefCell<Vec<Upozorneni>> = RefCell::new(nacist_historii());
    static DALSI_ID: Cell<u32> = const { Cell::new(0) };
    static SBERNICE: RefCell<Option<gio::DBusConnection>> = const { RefCell::new(None) };
}

fn soubor_historie() -> PathBuf {
    domov().join(".cache/apogee/notifications.json")
}

fn slozka_obrazku() -> PathBuf {
    domov().join(".cache/apogee/notifications")
}

fn ted() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn nacist_historii() -> Vec<Upozorneni> {
    let v: Value = std::fs::read_to_string(soubor_historie()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|n| Upozorneni {
                    id: n["id"].as_u64().unwrap_or(0) as u32,
                    aplikace: n["app_name"].as_str().unwrap_or("").into(),
                    ikona: n["app_icon"].as_str().unwrap_or("").into(),
                    desktop: n["desktop_entry"].as_str().unwrap_or("").into(),
                    nadpis: n["summary"].as_str().unwrap_or("").into(),
                    telo: n["body"].as_str().unwrap_or("").into(),
                    akce: Vec::new(),
                    nalehavost: n["urgency"].as_u64().unwrap_or(1) as u8,
                    cas: n["cas"].as_f64().unwrap_or(0.0),
                    precteno: n["precteno"].as_bool().unwrap_or(true),
                    docasna: false,
                    prubeh: None,
                    znacka: None,
                    doba_ms: 0,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn ulozit_historii() {
    let v: Vec<Value> = HISTORIE.with(|h| h.borrow().iter().map(do_json).collect());
    if let Some(s) = soubor_historie().parent() {
        let _ = std::fs::create_dir_all(s);
    }
    let _ = std::fs::write(soubor_historie(), Value::Array(v).to_string());
}

/// Notifikace ve tvaru, jaký čte okénko pod zvonečkem.
pub fn do_json(n: &Upozorneni) -> Value {
    json!({
        "id": n.id, "app_name": n.aplikace, "app_icon": n.ikona, "desktop_entry": n.desktop,
        "summary": n.nadpis, "body": n.telo, "urgency": n.nalehavost, "cas": n.cas, "precteno": n.precteno,
    })
}

/// Historie od nejnovější.
pub fn historie() -> Vec<Upozorneni> {
    HISTORIE.with(|h| h.borrow().iter().rev().cloned().collect())
}

pub fn neprectenych() -> usize {
    HISTORIE.with(|h| h.borrow().iter().filter(|n| !n.precteno).count())
}

pub fn oznacit_prectene() {
    HISTORIE.with(|h| h.borrow_mut().iter_mut().for_each(|n| n.precteno = true));
    ulozit_historii();
}

/// Smazat z historie (a zavřít bublinu, když je vidět).
pub fn smazat(id: u32) {
    HISTORIE.with(|h| h.borrow_mut().retain(|n| n.id != id));
    bubliny::zavrit(id, Duvod::Zavrena);
    ulozit_historii();
}

pub fn smazat_vse() {
    let id: Vec<u32> = HISTORIE.with(|h| h.borrow_mut().drain(..).map(|n| n.id).collect());
    for i in id {
        bubliny::zavrit(i, Duvod::Zavrena);
    }
    ulozit_historii();
}

/// Aplikace, které uživatel v Settings ztlumil (nastaveni.json `upozorneni_ztlumene`).
pub fn ztlumena(aplikace: &str) -> bool {
    !aplikace.is_empty() && nastaveni("upozorneni_ztlumene").as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(aplikace)))
}

/// Aplikace, které kdy poslaly oznámení (z historie), pro seznam v Settings.
pub fn aplikace_v_historii() -> Vec<String> {
    let mut v: Vec<String> = HISTORIE.with(|h| h.borrow().iter().map(|n| n.aplikace.clone()).filter(|a| !a.is_empty()).collect());
    v.sort_by_key(|a| a.to_lowercase());
    v.dedup();
    v
}

/// Nerušit podle času (`dnd_cas`: {zapnuto, od, do} v minutách od půlnoci): kontrola každou
/// minutu; zapne / vypne jen na hranici intervalu, takže ruční přepnutí mezi tím platí.
fn hlidat_dnd_cas() {
    use chrono::Timelike;
    thread_local! { static BYLO: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) }; }
    let c = nastaveni("dnd_cas");
    if c["zapnuto"].as_bool().unwrap_or(false) {
        let (od, do_) = (c["od"].as_u64().unwrap_or(22 * 60) as u32, c["do"].as_u64().unwrap_or(7 * 60) as u32);
        let t = chrono::Local::now();
        let m = t.hour() * 60 + t.minute();
        let uvnitr = if od <= do_ { m >= od && m < do_ } else { m >= od || m < do_ };
        if BYLO.with(|b| b.get()) != Some(uvnitr) {
            if BYLO.with(|b| b.get()).is_some() || uvnitr {
                crate::okenka::prikaz("dnd", if uvnitr { "on" } else { "off" });
            }
            BYLO.with(|b| b.set(Some(uvnitr)));
        }
    } else {
        BYLO.with(|b| b.set(None));
    }
}

pub fn dnd() -> bool {
    nastaveni("dnd").as_bool().unwrap_or(false)
}

pub fn nastavit_dnd(zapnout: bool) {
    ulozit_nastaveni("dnd", json!(zapnout));
    if zapnout {
        bubliny::schovat_nekriticke();
    }
}

/// Notifikace se zavřela (vypršela, zavřel ji uživatel): oznámit aplikaci.
pub fn ohlasit_zavreni(id: u32, duvod: Duvod) {
    if let Some(s) = SBERNICE.with(|s| s.borrow().clone()) {
        let _ = s.emit_signal(None, CESTA, JMENO, "NotificationClosed", Some(&(id, duvod as u32).to_variant()));
    }
}

/// Uživatel zvolil akci (klik na bublinu = „default“).
pub fn vyvolat_akci(id: u32, klic: &str) {
    if let Some(s) = SBERNICE.with(|s| s.borrow().clone()) {
        let _ = s.emit_signal(None, CESTA, JMENO, "ActionInvoked", Some(&(id, klic).to_variant()));
    }
}

/// Spustit server (při startu démona). Když službu drží jiný program (mako), démon čeká ve frontě.
pub fn spustit() {
    glib::timeout_add_seconds_local_once(5, hlidat_dnd_cas);
    glib::timeout_add_seconds_local(30, || {
        hlidat_dnd_cas();
        glib::ControlFlow::Continue
    });
    let Ok(s) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else { return };
    SBERNICE.with(|x| x.replace(Some(s.clone())));
    DALSI_ID.with(|d| d.set(HISTORIE.with(|h| h.borrow().iter().map(|n| n.id).max().unwrap_or(0))));
    let Ok(uzel) = gio::DBusNodeInfo::for_xml(XML) else { return };
    let Some(rozhrani) = uzel.lookup_interface(JMENO) else { return };
    let ok = s
        .register_object(CESTA, &rozhrani)
        .method_call(|_, odesilatel, _, _, metoda, parametry, volani| match metoda {
            "GetCapabilities" => {
                let schopnosti = vec!["actions", "body", "body-markup", "icon-static", "persistence", "x-canonical-private-synchronous"];
                volani.return_value(Some(&(schopnosti,).to_variant()));
            }
            "GetServerInformation" => volani.return_value(Some(&("apogee", "Spartakusovec", "1.0", "1.2").to_variant())),
            "CloseNotification" => {
                if let Some((id,)) = parametry.get::<(u32,)>() {
                    bubliny::zavrit(id, Duvod::Aplikaci);
                }
                volani.return_value(None);
            }
            "Notify" => {
                let id = prijmout(&parametry, odesilatel.unwrap_or(""));
                volani.return_value(Some(&(id,).to_variant()));
            }
            _ => volani.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", "Unknown method"),
        })
        .build();
    if ok.is_err() {
        return;
    }
    let id = gio::bus_own_name_on_connection(&s, JMENO, gio::BusNameOwnerFlags::ALLOW_REPLACEMENT | gio::BusNameOwnerFlags::REPLACE, |_, _| {}, |_, _| {});
    std::mem::forget(id);
    crate::okenka::prikaz("notifikace-zmena", "");
}

/// Obrázek z hintu image-data (iiibiiay) → PNG v mezipaměti, vrátí cestu.
fn ulozit_obrazek(id: u32, v: &Variant) -> Option<String> {
    let (sirka, vyska, radek, alfa, _bity, kanaly, data) = v.get::<(i32, i32, i32, bool, i32, i32, Vec<u8>)>()?;
    if sirka <= 0 || vyska <= 0 || kanaly < 3 {
        return None;
    }
    let obrazek = gtk::gdk_pixbuf::Pixbuf::from_bytes(&glib::Bytes::from_owned(data), gtk::gdk_pixbuf::Colorspace::Rgb, alfa, 8, sirka, vyska, radek);
    let _ = std::fs::create_dir_all(slozka_obrazku());
    let cesta = slozka_obrazku().join(format!("{id}.png"));
    let maly = if sirka > 128 || vyska > 128 {
        let m = 128.0 / sirka.max(vyska) as f64;
        obrazek.scale_simple((sirka as f64 * m) as i32, (vyska as f64 * m) as i32, gtk::gdk_pixbuf::InterpType::Bilinear)?
    } else {
        obrazek
    };
    maly.savev(&cesta, "png", &[]).ok()?;
    Some(cesta.to_string_lossy().to_string())
}

/// Notify(app_name, replaces_id, app_icon, summary, body, actions, hints, expire_timeout) → id
fn prijmout(p: &Variant, _odesilatel: &str) -> u32 {
    let Some((aplikace, nahradit, ikona, nadpis, telo, akce, hinty, vyprseni)) =
        p.get::<(String, u32, String, String, String, Vec<String>, HashMap<String, Variant>, i32)>()
    else {
        return 0;
    };
    let text = |k: &str| hinty.get(k).and_then(|v| v.str().map(String::from));
    let znacka = text("x-canonical-private-synchronous").or_else(|| text("x-dunst-stack-tag"));
    // nahradit: podle replaces_id, nebo starší se stejnou značkou
    let existujici = HISTORIE.with(|h| {
        let h = h.borrow();
        if nahradit > 0 && h.iter().any(|n| n.id == nahradit) {
            Some(nahradit)
        } else {
            znacka.as_ref().and_then(|z| h.iter().rev().find(|n| n.znacka.as_ref() == Some(z)).map(|n| n.id))
        }
    }).or_else(|| (nahradit > 0 && bubliny::je_videt(nahradit)).then_some(nahradit));
    let id = existujici.unwrap_or_else(|| {
        DALSI_ID.with(|d| {
            d.set(d.get() + 1);
            d.get()
        })
    });
    let nalehavost = hinty.get("urgency").and_then(|v| v.get::<u8>()).unwrap_or(1);
    let obrazek = ["image-data", "image_data", "icon_data"].iter().find_map(|k| hinty.get(*k)).and_then(|v| ulozit_obrazek(id, v));
    let cesta_obrazku = text("image-path").or_else(|| text("image_path"));
    let ikona = obrazek
        .or(cesta_obrazku)
        .unwrap_or(ikona)
        .trim_start_matches("file://")
        .to_string();
    let n = Upozorneni {
        id,
        aplikace,
        ikona,
        desktop: text("desktop-entry").unwrap_or_default(),
        nadpis,
        telo,
        akce: akce.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect(),
        nalehavost,
        cas: ted(),
        precteno: false,
        docasna: hinty.get("transient").and_then(|v| v.get::<bool>()).unwrap_or(false),
        prubeh: hinty.get("value").and_then(|v| v.get::<i32>()),
        znacka,
        doba_ms: if nalehavost == 2 || vyprseni == 0 { 0 } else if vyprseni < 0 { VYCHOZI_DOBA_MS } else { vyprseni as u32 },
    };
    if !n.docasna {
        HISTORIE.with(|h| {
            let mut h = h.borrow_mut();
            h.retain(|x| x.id != id);
            h.push(n.clone());
            let nadbytek = h.len().saturating_sub(NEJVIC_V_HISTORII);
            h.drain(..nadbytek);
        });
        ulozit_historii();
    }
    // ztlumené aplikace (Settings → Notifications): jen do historie, bez bubliny
    if (!dnd() && !ztlumena(&n.aplikace)) || nalehavost == 2 {
        bubliny::ukazat(&n);
    }
    crate::okenka::prikaz("notifikace-zmena", "");
    id
}
