//! One-time migration of files written by older versions, whose keys and values were Czech
//! (`dok`, `hodiny`, `poloha`…). Runs at daemon start; settings.json gets `"format": 2` afterwards.

use serde_json::Value;

use crate::common::home;

/// old (Czech) name → new name, for keys and (in settings.json) string values
const RENAMED: &[(&str, &str)] = &[
    ("POMOCNIK_DEBUG", "APOGEE_DEBUG"),
    ("active-tapeta", "active-wallpaper"),
    ("akce", "action"),
    ("aktivni", "active"),
    ("aktivni-tapeta", "active-tapeta"),
    ("aktualizace", "updates"),
    ("aktualizace-zkontrolovat", "updates-check"),
    ("alttab-zpet", "alttab-back"),
    ("aplikace", "apps"),
    ("aplikace-dlazdice", "apps-tiling"),
    ("baterie", "battery"),
    ("bublina", "bubble"),
    ("bubliny", "bubbles"),
    ("cas", "time"),
    ("chytre", "smart"),
    ("ctverec", "square"),
    ("dalsi", "next"),
    ("datum", "date"),
    ("delka", "lon"),
    ("den", "day"),
    ("den-hlavicka", "day-header"),
    ("displej", "display"),
    ("dlazdice", "tiling"),
    ("dlazdice_vychozi", "tiling_default"),
    ("dnd_cas", "dnd_schedule"),
    ("dnes", "today"),
    ("do", "to"),
    ("dock-zona", "dock-zone"),
    ("dok", "dock"),
    ("dok-mrizka", "dock-grid"),
    ("dok-nabidka", "dock-menu"),
    ("dok-polozka", "dock-item"),
    ("dok-zona", "dock-zona"),
    ("dok_skryvani", "dock_hiding"),
    ("dostupne", "available"),
    ("heslo", "password"),
    ("hlavni", "main"),
    ("hlavni-akce", "main-action"),
    ("hodiny", "clock"),
    ("hodnota", "value"),
    ("hudba", "music"),
    ("ikona", "icon"),
    ("ikony", "icons"),
    ("jas", "brightness"),
    ("jas-down", "brightness-down"),
    ("jas-up", "brightness-up"),
    ("jen-nahravani", "jen-recording"),
    ("jen-recording", "only-recording"),
    ("jen-screenshot", "only-screenshot"),
    ("jen-snimek", "jen-screenshot"),
    ("jiny-mesic", "other-mesic"),
    ("jmeno", "name"),
    ("kalendar", "calendar"),
    ("karta", "card"),
    ("kde", "place"),
    ("klavesy", "keys"),
    ("klikaci", "clickable"),
    ("kriticka", "critical"),
    ("lista", "bar"),
    ("mala-icon", "small-icon"),
    ("mala-ikona", "mala-icon"),
    ("maly-cas", "small-time"),
    ("mikrofon", "microphone"),
    ("modul", "module"),
    ("nadpis", "title"),
    ("nadpis-notifikace", "title-notifications"),
    ("nadpis-prepinace", "title-switcher"),
    ("nadpis-skupiny", "title-groups"),
    ("nadpis-stranky", "title-pages"),
    ("nahrava", "recording"),
    ("nahravani", "recording"),
    ("nahravani-stop", "recording-stop"),
    ("nahravat", "record"),
    ("nahravka-", "recording-"),
    ("nalehava", "urgent"),
    ("napajeni", "power"),
    ("nastaveni", "settings"),
    ("nazev", "name"),
    ("neprectene", "unread"),
    ("nikdy", "never"),
    ("nizka", "low"),
    ("notifikace", "notifications"),
    ("notifikace-zmena", "notifications-change"),
    ("o_pocitaci", "about"),
    ("obal", "wrapper"),
    ("obal-nahradni", "wrapper-fallback"),
    ("oblast", "area"),
    ("obnovit", "refresh"),
    ("obrazovka", "screen"),
    ("obri", "giant"),
    ("obsah", "content"),
    ("od", "from"),
    ("odkaz", "link"),
    ("okna", "windows"),
    ("okno", "window"),
    ("ostruvek", "island"),
    ("other-mesic", "other-month"),
    ("ovladani", "controls"),
    ("oznameni", "notifications"),
    ("panel_dok", "panel_dock"),
    ("panel_skryte", "panel_hidden"),
    ("plocha", "workspace"),
    ("plochy", "workspaces"),
    ("plovouci", "floating"),
    ("po_startu", "autostart"),
    ("pocasi", "weather"),
    ("pocasi-data", "weather-data"),
    ("pocasi-ikona", "weather-icon"),
    ("pocasi-obnovit", "weather-refresh"),
    ("poloha", "location"),
    ("popis", "description"),
    ("posledni", "last"),
    ("postranni", "sidebar"),
    ("potvrzeni", "toast"),
    ("pouziti", "usage"),
    ("pouziva", "using"),
    ("pozastaveno", "paused"),
    ("prace", "work"),
    ("prazdna", "empty"),
    ("precteno", "read"),
    ("predchozi", "previous"),
    ("prehravac", "player"),
    ("prepinac", "switcher"),
    ("prepinac-oken", "switcher-oken"),
    ("prepnout", "toggle"),
    ("prestavka", "break_time"),
    ("prouzek", "strip"),
    ("radek", "row"),
    ("radek-akce", "row-action"),
    ("radek-okna", "row-windows"),
    ("rezim", "mode"),
    ("rozlozeni", "layouts"),
    ("rychle", "quick"),
    ("rychle-ikona", "quick-icon"),
    ("schranka", "clipboard"),
    ("sekundy", "seconds"),
    ("sipka", "arrow"),
    ("sirka", "lat"),
    ("sit", "network"),
    ("skryta", "hidden"),
    ("skupina", "group"),
    ("skupina-stranek", "group-pages"),
    ("snimek", "screenshot"),
    ("snimek-", "screenshot-"),
    ("snimek-hned", "screenshot-now"),
    ("spotlight-ikona", "spotlight-icon"),
    ("spotlight-lupa", "spotlight-zoom"),
    ("spotlight-pole", "spotlight-field"),
    ("spotlight-vysledky", "spotlight-results"),
    ("spoustec", "launcher"),
    ("stahuje", "downloading"),
    ("stav", "state"),
    ("stranka", "page"),
    ("stranky", "pages"),
    ("stred", "center"),
    ("styl", "style"),
    ("sway-dlazdice-", "sway-tiling-"),
    ("sway-plovouci-", "sway-floating-"),
    ("switcher-oken", "switcher-windows"),
    ("tapeta", "wallpaper"),
    ("tapety", "wallpapers"),
    ("tecka", "dot"),
    ("tecky", "dots"),
    ("telo", "body"),
    ("teplota", "temperature"),
    ("titulek-skladby", "title-track"),
    ("tray-nabidka", "tray-menu"),
    ("tray-polozka", "tray-item"),
    ("tyden", "week"),
    ("uloziste", "storage"),
    ("upozorneni_ztlumene", "muted_apps"),
    ("upravit", "modify"),
    ("uzivatel", "user"),
    ("vedlejsi", "secondary"),
    ("velke", "large"),
    ("videt", "visible"),
    ("vikend", "weekend"),
    ("vpravo", "right"),
    ("vstup", "input"),
    ("vybrany", "selected"),
    ("vypnout", "power"),
    ("vypnuto", "off"),
    ("vysledek", "result"),
    ("vzdy", "always"),
    ("vzhled", "appearance"),
    ("zadne", "none"),
    ("zamknout", "lock"),
    ("zapad", "sunset"),
    ("zapnuto", "enabled"),
    ("zavrit", "close"),
    ("ze", "from"),
    ("zkratky", "shortcuts"),
    ("zpet", "back"),
    ("zpozdeni", "delay"),
    ("ztmaveni", "dim"),
    ("zvuk", "sound"),
];

fn new_name(s: &str) -> Option<&'static str> {
    RENAMED.iter().find(|(old, _)| *old == s).map(|(_, new)| *new)
}

fn migrate(v: &mut Value, values_too: bool) {
    match v {
        Value::Object(map) => {
            let old: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
            for (k, mut val) in old {
                migrate(&mut val, values_too);
                map.insert(new_name(&k).map(String::from).unwrap_or(k), val);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| migrate(x, values_too)),
        Value::String(s) if values_too => {
            if let Some(n) = new_name(s) {
                *s = n.to_string();
            }
        }
        _ => {}
    }
}

fn migrate_file(path: std::path::PathBuf, values_too: bool, mark: bool) {
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let Ok(mut v) = serde_json::from_str::<Value>(&text) else { return };
    if mark && v["format"].as_u64() == Some(2) {
        return;
    }
    migrate(&mut v, values_too);
    if mark {
        v["format"] = Value::from(2);
    }
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default());
}

/// Sway helper configs (key=value lines) shared with the scripts: Czech keys and values → English.
fn migrate_conf(path: std::path::PathBuf, pairs: &[(&str, &str)]) {
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let mut changed = false;
    let lines: Vec<String> = text
        .lines()
        .map(|l| match l.split_once('=') {
            Some((k, v)) if !l.starts_with('#') => {
                let nk = pairs.iter().find(|(o, _)| *o == k).map(|(_, n)| *n).unwrap_or(k);
                let nv = pairs.iter().find(|(o, _)| *o == v).map(|(_, n)| *n).unwrap_or(v);
                changed |= nk != k || nv != v;
                format!("{nk}={nv}")
            }
            _ => l.to_string(),
        })
        .collect();
    if changed {
        let _ = std::fs::write(&path, lines.join("\n") + "\n");
    }
}

/// Settings (keys and values) and caches (keys only) from before the English rename.
pub fn run() {
    let sway = home().join(".config/sway");
    migrate_conf(
        sway.join("night-light.conf"),
        &[("rezim", "mode"), ("teplota", "temperature"), ("posledni", "last"), ("poloha", "location"), ("zapad", "sunset"), ("vzdy", "always"), ("vypnuto", "off")],
    );
    migrate_conf(sway.join("idle.conf"), &[("zamknout", "lock"), ("vypnout", "screen_off")]);
    let settings = home().join(".config/apogee/settings.json");
    let done = std::fs::read_to_string(&settings).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).is_some_and(|v| v["format"].as_u64() == Some(2));
    if done {
        return;
    }
    migrate_file(settings, true, true);
    for f in ["notifications.json", "updates.json", "rates.json"] {
        migrate_file(home().join(".cache/apogee").join(f), false, false);
    }
}
