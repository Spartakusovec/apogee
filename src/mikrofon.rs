//! Ikona mikrofonu v panelu, jen když nějaká aplikace nahrává z mikrofonu (klik = okénko zvuku).
//! Změny hlásí `pactl subscribe` (jeden proces, nic se neběhá dokola). Nahrávání zvuku systému
//! (monitor výstupu) a ukazatele úrovně v pavucontrol se nepočítají.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use crate::panel::publikovat;
use crate::spolecne::prikaz;

static APLIKACE: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Aplikace, které teď nahrávají z mikrofonu.
pub fn kdo_nahrava() -> Vec<String> {
    APLIKACE.lock().unwrap().clone()
}

fn zjistit() {
    let json = |co: &str| serde_json::from_str::<Value>(&prikaz("pactl", &["-f", "json", "list", co])).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let monitory: Vec<i64> = json("sources")
        .iter()
        .filter(|s| s["name"].as_str().is_some_and(|n| n.ends_with(".monitor")))
        .filter_map(|s| s["index"].as_i64())
        .collect();
    let mut aplikace: Vec<String> = json("source-outputs")
        .iter()
        .filter(|o| !o["corked"].as_bool().unwrap_or(false))
        .filter(|o| o["source"].as_i64().is_some_and(|s| !monitory.contains(&s)))
        .filter(|o| !o["properties"]["application.id"].as_str().unwrap_or("").contains("pavucontrol"))
        .map(|o| o["properties"]["application.name"].as_str().unwrap_or("An app").to_string())
        .collect();
    aplikace.dedup();
    let v = if aplikace.is_empty() { json!({"text": ""}) } else { json!({"text": "\u{f130}", "class": "pouziva"}) };
    *APLIKACE.lock().unwrap() = aplikace;
    publikovat("mikrofon", v);
}

pub fn sledovat() {
    std::thread::spawn(|| loop {
        zjistit();
        let mut c = Command::new("pactl");
        c.arg("subscribe").stdout(Stdio::piped()).stderr(Stdio::null());
        // skončit s démonem (jinak by po jeho restartu zůstal viset)
        unsafe {
            use std::os::unix::process::CommandExt;
            c.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        if let Ok(mut p) = c.spawn() {
            if let Some(vystup) = p.stdout.take() {
                for radek in BufReader::new(vystup).lines().map_while(Result::ok) {
                    // Event 'new' on source-output #42
                    if radek.contains("source-output") {
                        zjistit();
                    }
                    // hlasitost výstupu nebo jiný výchozí výstup: ikona zvuku v liště
                    if radek.contains(" on sink #") || radek.contains(" on server") {
                        crate::lista::zdroje::hlasitost_zmenena();
                    }
                }
            }
            let _ = p.wait();
        }
        // zvukový server se restartoval: připojit znovu
        std::thread::sleep(Duration::from_secs(2));
    });
}
