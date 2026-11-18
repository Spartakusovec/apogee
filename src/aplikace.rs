//! Název a ikona aplikace podle jejího .desktop souboru (stejně jako spouštěč aplikací).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Clone)]
pub struct Info {
    pub jmeno: String,
    pub ikona: String,
}

struct Rejstrik {
    /// název souboru bez .desktop (malými písmeny) → cesta
    podle_souboru: HashMap<String, PathBuf>,
    /// StartupWMClass (malými písmeny) → cesta
    podle_tridy: HashMap<String, PathBuf>,
    hotove: HashMap<String, Info>,
}

static REJSTRIK: Mutex<Option<Rejstrik>> = Mutex::new(None);

fn slozky() -> Vec<PathBuf> {
    vec![
        crate::spolecne::domov().join(".local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/usr/share/applications"),
    ]
}

fn sestavit() -> Rejstrik {
    let mut r = Rejstrik { podle_souboru: HashMap::new(), podle_tridy: HashMap::new(), hotove: HashMap::new() };
    for slozka in slozky() {
        let Ok(soubory) = std::fs::read_dir(&slozka) else { continue };
        for s in soubory.flatten() {
            let cesta = s.path();
            let Some(nazev) = cesta.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".desktop")) else {
                continue;
            };
            r.podle_souboru.entry(nazev.to_lowercase()).or_insert_with(|| cesta.clone());
            if let Ok(obsah) = std::fs::read_to_string(&cesta) {
                if let Some(trida) = obsah.lines().find_map(|l| l.strip_prefix("StartupWMClass=")) {
                    r.podle_tridy.entry(trida.trim().to_lowercase()).or_insert(cesta);
                }
            }
        }
    }
    r
}

/// Rejstřík se sestaví na pozadí hned po spuštění, ať je Alt+Tab rychlý i poprvé.
pub fn nacist_na_pozadi() {
    std::thread::spawn(|| {
        let r = sestavit();
        *REJSTRIK.lock().unwrap() = Some(r);
    });
}

fn precist(cesta: &PathBuf, id: &str) -> Info {
    let mut jmeno = None;
    let mut ikona = None;
    let mut sekce = false;
    for radek in std::fs::read_to_string(cesta).unwrap_or_default().lines() {
        if radek.starts_with('[') {
            sekce = radek == "[Desktop Entry]";
        } else if sekce {
            if let Some(v) = radek.strip_prefix("Name=") {
                jmeno.get_or_insert_with(|| v.to_string());
            } else if let Some(v) = radek.strip_prefix("Icon=") {
                ikona.get_or_insert_with(|| v.to_string());
            }
        }
    }
    Info { jmeno: jmeno.unwrap_or_else(|| id.to_string()), ikona: ikona.unwrap_or_else(|| "application-x-executable".into()) }
}

fn najit(r: &Rejstrik, app_id: &str) -> Option<PathBuf> {
    let maly = app_id.to_lowercase();
    r.podle_souboru
        .get(&maly)
        .or_else(|| r.podle_tridy.get(&maly))
        // app_id se ne vždy shoduje s názvem souboru (Discord, Spotify…): zkusit konec názvu
        .or_else(|| r.podle_souboru.iter().find(|(k, _)| k.ends_with(&format!(".{maly}"))).map(|(_, v)| v))
        .cloned()
}

/// .desktop soubor aplikace podle app_id okna (None = žádný není).
pub fn cesta(app_id: &str) -> Option<PathBuf> {
    let mut zamek = REJSTRIK.lock().unwrap();
    let r = zamek.get_or_insert_with(sestavit);
    najit(r, app_id)
}

pub fn info(app_id: &str) -> Info {
    let mut zamek = REJSTRIK.lock().unwrap();
    let r = zamek.get_or_insert_with(sestavit);
    if let Some(i) = r.hotove.get(app_id) {
        return i.clone();
    }
    let cesta = najit(r, app_id);
    let i = match cesta {
        Some(c) => precist(&c, app_id),
        None => Info { jmeno: app_id.to_string(), ikona: "application-x-executable".into() },
    };
    r.hotove.insert(app_id.to_string(), i.clone());
    i
}
