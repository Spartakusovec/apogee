//! Snímky a nahrávání obrazovky (grim, slurp, wf-recorder): výběr oblasti, okna nebo celé
//! obrazovky, uložení, schránka a notifikace. Panel ukazuje červenou tečku s časem, dokud se
//! nahrává (klik = konec).
//!
//! Snímky: ~/Pictures/Screenshots, zároveň ve schránce; klik na notifikaci otevře editor (swappy).
//! Nahrávky: ~/Videos/Recordings, kóduje grafika Intel (VA-API), když to nejde, procesor.

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use gtk::glib;
use serde_json::json;
use swayipc::{Connection, Node};

use crate::panel::publikovat;
use crate::spolecne::*;

/// Intel (renderD128 je NVIDIA, ta se nechává spát)
const GRAFIKA: &str = "/dev/dri/renderD129";

fn slozka(xdg: &str, nahradni: &str, podslozka: &str) -> PathBuf {
    let zaklad = prikaz("xdg-user-dir", &[xdg]);
    let zaklad = if zaklad.is_empty() { domov().join(nahradni) } else { PathBuf::from(zaklad) };
    let s = zaklad.join(podslozka);
    let _ = std::fs::create_dir_all(&s);
    s
}

fn casova_znacka() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}

/// Co se zachytí: Some(Some("x,y wxh")), Some(None) = celá aktivní obrazovka, None = zrušeno.
/// Běží mimo GTK (slurp čeká na myš).
fn vyber(rezim: &str) -> Option<Option<String>> {
    match rezim {
        "oblast" => {
            let o = Command::new("slurp").stderr(Stdio::null()).output().ok()?;
            o.status.success().then(|| Some(String::from_utf8_lossy(&o.stdout).trim().to_string()))
        }
        "okno" => {
            // slurp nabídne jen obdélníky viditelných oken, klik jedno vybere
            let obdelniky = viditelna_okna().join("\n");
            let mut slurp = Command::new("slurp").arg("-r").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
            let _ = slurp.stdin.take()?.write_all(obdelniky.as_bytes());
            let o = slurp.wait_with_output().ok()?;
            o.status.success().then(|| Some(String::from_utf8_lossy(&o.stdout).trim().to_string()))
        }
        _ => Some(None),
    }
}

fn viditelna_okna() -> Vec<String> {
    let Ok(strom) = Connection::new().and_then(|mut c| c.get_tree()) else { return Vec::new() };
    let mut vysledek = Vec::new();
    fn projdi(n: &Node, v: &mut Vec<String>) {
        if n.pid.is_some() && n.visible == Some(true) {
            let r = &n.rect;
            v.push(format!("{},{} {}x{}", r.x, r.y, r.width, r.height));
        }
        for d in n.nodes.iter().chain(n.floating_nodes.iter()) {
            projdi(d, v);
        }
    }
    projdi(&strom, &mut vysledek);
    vysledek
}

fn aktivni_obrazovka() -> Option<String> {
    Connection::new().ok()?.get_outputs().ok()?.into_iter().find(|o| o.focused).map(|o| o.name)
}

/// Snímek obrazovky. `zpozdeni` v sekundách (až po výběru oblasti), `upravit` otevře editor.
pub fn snimek(rezim: &str, zpozdeni: u64, upravit: bool) {
    crate::okenka::zavrit();
    let rezim = rezim.to_string();
    std::thread::spawn(move || {
        // ať okénko stihne zmizet a není na snímku
        std::thread::sleep(Duration::from_millis(300));
        let Some(oblast) = vyber(&rezim) else { return };
        std::thread::sleep(Duration::from_secs(zpozdeni));
        let soubor = slozka("PICTURES", "Pictures", "Screenshots").join(format!("snimek-{}.png", casova_znacka()));
        let mut grim = Command::new("grim");
        match &oblast {
            Some(g) => grim.args(["-g", g]),
            None => grim.args(["-o", &aktivni_obrazovka().unwrap_or_default()]),
        };
        if !grim.arg(&soubor).status().is_ok_and(|s| s.success()) {
            pozadi("notify-send", &["-i", "dialog-error", "Screenshot failed"]);
            return;
        }
        if let Ok(f) = std::fs::File::open(&soubor) {
            let _ = Command::new("wl-copy").args(["--type", "image/png"]).stdin(f).status();
        }
        let cesta = soubor.to_string_lossy().to_string();
        if upravit {
            pozadi("swappy", &["-f", &cesta]);
        }
        // klik na notifikaci otevře editor (notify-send čeká, než notifikace zmizí)
        let akce = prikaz("notify-send", &["-i", &cesta, "-A", "default=Edit", "Screenshot saved", &format!("Copied to clipboard. Click to edit.\n{cesta}")]);
        if akce == "default" {
            pozadi("swappy", &["-f", &cesta]);
        }
    });
}

struct Nahravani {
    proces: Child,
    soubor: PathBuf,
    zacatek: Instant,
    argumenty: Vec<String>,
    /// VA-API nejde, nahrává se procesorem
    procesorem: bool,
    /// ukončené uživatelem (ne chybou)
    zastaveno: bool,
}

thread_local! {
    static NAHRAVANI: RefCell<Option<Nahravani>> = const { RefCell::new(None) };
}

pub fn nahrava() -> bool {
    NAHRAVANI.with(|n| n.borrow().is_some())
}

/// Spustí nahrávání (zvuk: vypnuto, system, mikrofon); když už se nahrává, zastaví ho.
pub fn nahravat(rezim: &str, zvuk: &str) {
    if nahrava() {
        zastavit();
        return;
    }
    crate::okenka::zavrit();
    let (rezim, zvuk) = (rezim.to_string(), zvuk.to_string());
    na_pozadi(
        move || {
            std::thread::sleep(Duration::from_millis(300));
            let oblast = vyber(&rezim)?;
            let zarizeni = match zvuk.as_str() {
                "system" => Some(format!("{}.monitor", prikaz("pactl", &["get-default-sink"]))),
                "mikrofon" => Some(prikaz("pactl", &["get-default-source"])),
                _ => None,
            };
            Some((oblast, zarizeni, aktivni_obrazovka().unwrap_or_default()))
        },
        |vyber| {
            let Some((oblast, zarizeni, obrazovka)) = vyber else { return };
            let soubor = slozka("VIDEOS", "Videos", "Recordings").join(format!("nahravka-{}.mp4", casova_znacka()));
            let mut argumenty: Vec<String> = Vec::new();
            match oblast {
                Some(g) => argumenty.extend(["-g".into(), sude(&g)]),
                None => argumenty.extend(["-o".into(), obrazovka]),
            }
            if let Some(z) = zarizeni {
                argumenty.push(format!("--audio={z}"));
            }
            argumenty.extend(["-f".into(), soubor.to_string_lossy().to_string()]);
            spustit(argumenty, soubor, false);
        },
    );
}

/// x264 chce sudé rozměry
fn sude(oblast: &str) -> String {
    let Some((poloha, rozmer)) = oblast.split_once(' ') else { return oblast.to_string() };
    let Some((s, v)) = rozmer.split_once('x') else { return oblast.to_string() };
    let sude = |t: &str| t.parse::<i64>().map(|c| (c / 2 * 2).max(2).to_string()).unwrap_or_else(|_| t.to_string());
    format!("{poloha} {}x{}", sude(s), sude(v))
}

fn spustit(argumenty: Vec<String>, soubor: PathBuf, procesorem: bool) {
    use std::os::unix::process::CommandExt;
    let mut c = Command::new("wf-recorder");
    c.arg("-y");
    if !procesorem {
        c.args(["-c", "h264_vaapi", "-d", GRAFIKA]);
    }
    let proces = c.args(&argumenty).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn();
    let Ok(proces) = proces else {
        pozadi("notify-send", &["-i", "dialog-error", "Screen recording failed", "wf-recorder could not be started."]);
        return;
    };
    NAHRAVANI.with(|n| n.replace(Some(Nahravani { proces, soubor, zacatek: Instant::now(), argumenty, procesorem, zastaveno: false })));
    do_panelu();
    glib::timeout_add_local(Duration::from_millis(500), || {
        if hlidat() {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

/// Běží nahrávání dál? Když skončilo, oznámí výsledek.
fn hlidat() -> bool {
    let skoncilo = NAHRAVANI.with(|n| {
        let mut n = n.borrow_mut();
        let x = n.as_mut()?;
        match x.proces.try_wait() {
            Ok(None) => None,
            _ => n.take(),
        }
    });
    let Some(x) = skoncilo else {
        do_panelu();
        return true;
    };
    // VA-API nefunguje (hned samo skončilo): zkusit znovu procesorem
    if !x.procesorem && !x.zastaveno && x.zacatek.elapsed() < Duration::from_secs(2) {
        spustit(x.argumenty, x.soubor, true);
        return false;
    }
    do_panelu();
    let cesta = x.soubor.to_string_lossy().to_string();
    if std::fs::metadata(&x.soubor).is_ok_and(|m| m.len() > 0) {
        std::thread::spawn(move || {
            let akce = prikaz("notify-send", &["-i", "video-x-generic", "-A", "default=Open", "Recording saved", &format!("Click to play.\n{cesta}")]);
            if akce == "default" {
                pozadi("xdg-open", &[&cesta]);
            }
        });
    } else {
        pozadi("notify-send", &["-i", "dialog-error", "Screen recording failed"]);
    }
    false
}

/// Ukončí nahrávání (wf-recorder po Ctrl+C soubor dopíše).
pub fn zastavit() {
    NAHRAVANI.with(|n| {
        if let Some(x) = n.borrow_mut().as_mut() {
            x.zastaveno = true;
            unsafe { libc::kill(x.proces.id() as i32, libc::SIGINT) };
        }
    });
}

/// Panel: červená tečka s časem, dokud se nahrává.
fn do_panelu() {
    match delka() {
        Some(s) => publikovat("nahravani", json!({"text": format!("\u{f111}  {}:{:02}", s / 60, s % 60), "class": "nahrava"})),
        None => publikovat("nahravani", json!({"text": ""})),
    }
}

/// Kolik sekund se nahrává (None = nenahrává se).
pub fn delka() -> Option<u64> {
    NAHRAVANI.with(|n| n.borrow().as_ref().map(|x| x.zacatek.elapsed().as_secs()))
}

pub fn spustit_panel() {
    do_panelu();
}
