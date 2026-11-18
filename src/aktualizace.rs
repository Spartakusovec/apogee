//! Aktualizace: jednou za 6 hodin se podívá, co je nového v dnf a Flatpaku (na pozadí, s nízkou
//! prioritou). Panel ukáže ikonu s počtem, jen když něco je; klik = okénko se seznamem.
//! Výsledek se pamatuje v ~/.cache/apogee/updates.json, restart démona znovu nekontroluje.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use gtk::glib;
use serde_json::{json, Value};

use crate::panel::publikovat;
use crate::spolecne::*;

const INTERVAL: u64 = 6 * 3600;

thread_local! {
    static KONTROLUJE: Cell<bool> = const { Cell::new(false) };
}

fn soubor() -> PathBuf {
    domov().join(".cache/apogee/updates.json")
}

fn ted() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Poslední výsledek: {"cas": …, "dnf": [[název, verze]…], "flatpak": [[název, verze]…]}
pub fn nacist() -> Value {
    std::fs::read_to_string(soubor()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}))
}

pub fn kontroluje() -> bool {
    KONTROLUJE.with(|k| k.get())
}

fn pocet(v: &Value) -> usize {
    v["dnf"].as_array().map_or(0, |a| a.len()) + v["flatpak"].as_array().map_or(0, |a| a.len())
}

fn do_panelu(v: &Value) {
    let n = pocet(v);
    publikovat("aktualizace", if n > 0 { json!({"text": format!("\u{f019}  {n}"), "class": "dostupne"}) } else { json!({"text": ""}) });
}

/// Zjistí aktualizace (pomalé: stahuje seznamy balíků, volat mimo GTK).
fn zjistit() -> Value {
    // řádky „název.arch  verze  repozitář“ (dnf vrací 100, když něco je)
    let dnf: Vec<Value> = prikaz("nice", &["-n", "19", "dnf", "check-update", "-q"])
        .lines()
        .filter_map(|r| {
            let casti: Vec<&str> = r.split_whitespace().collect();
            (casti.len() == 3 && casti[0].contains('.')).then(|| json!([casti[0].rsplit_once('.').map_or(casti[0], |(n, _)| n), casti[1]]))
        })
        .collect();
    // „flatpak remote-ls --updates“ hlásí i věci, které flatpak update neaktualizuje (jiné větve
    // runtimů), proto se ptát přímo flatpak update a na otázku odpovědět „n“:
    // řádky „ 1.  org.inkscape.Inkscape  stable  u  fedora  < 52 MB“
    let flatpak: Vec<Value> = {
        use std::io::Write;
        use std::process::{Command, Stdio};
        Command::new("nice")
            .args(["-n", "19", "flatpak", "update"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut p| {
                if let Some(mut vstup) = p.stdin.take() {
                    let _ = vstup.write_all(b"n\n");
                }
                p.wait_with_output()
            })
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default()
            .lines()
            .filter_map(|r| {
                let casti: Vec<&str> = r.split_whitespace().collect();
                let poradi = casti.first()?.strip_suffix('.')?;
                poradi.parse::<u32>().ok()?;
                let id = casti.iter().skip(1).find(|c| c.contains('.'))?;
                let i = casti.iter().position(|c| c == id)?;
                Some(json!([id.rsplit('.').next().unwrap_or(id), casti.get(i + 1).copied().unwrap_or("")]))
            })
            .collect()
    };
    json!({"cas": ted(), "dnf": dnf, "flatpak": flatpak})
}

/// Zkontrolovat hned (tlačítko v okénku, po instalaci); `hotovo` dostane nový výsledek.
pub fn zkontrolovat(hotovo: impl FnOnce(Value) + 'static) {
    if KONTROLUJE.with(|k| k.replace(true)) {
        return;
    }
    na_pozadi(zjistit, move |v| {
        KONTROLUJE.with(|k| k.set(false));
        if let Some(s) = soubor().parent() {
            let _ = std::fs::create_dir_all(s);
        }
        let _ = std::fs::write(soubor(), v.to_string());
        do_panelu(&v);
        hotovo(v);
    });
}

pub fn sledovat() {
    do_panelu(&nacist());
    // kontrola podle skutečného času (i po probuzení z uspání), poprvé až po 5 minutách,
    // ať se nezdržuje přihlášení
    glib::timeout_add_seconds_local(300, || {
        if ted().saturating_sub(nacist()["cas"].as_u64().unwrap_or(0)) >= INTERVAL {
            zkontrolovat(|_| {});
        }
        glib::ControlFlow::Continue
    });
}

/// Otevře terminál s aktualizací (sudo chce heslo), na konci zkontroluje znovu.
pub fn nainstalovat() {
    let skript = "sudo dnf upgrade --refresh; echo; flatpak update; ~/.local/bin/apogee check-updates; \
                  echo; read -rp 'Done. Press Enter to close this window.'";
    pozadi("ptyxis", &["--new-window", "-T", "System update", "--", "bash", "-c", skript]);
}
