//! Microphone icon in the bar, only while some app records from the microphone (click = sound popup).
//! Changes are reported by `pactl subscribe` (one process, nothing is polled). Recording the system sound
//! (an output monitor) and pavucontrol's level meters don't count.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use crate::panel::publish;
use crate::common::command;

static APPS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Apps that are recording from the microphone right now.
pub fn who_recording() -> Vec<String> {
    APPS.lock().unwrap().clone()
}

fn detect() {
    let json = |co: &str| serde_json::from_str::<Value>(&command("pactl", &["-f", "json", "list", co])).ok().and_then(|v| v.as_array().cloned()).unwrap_or_default();
    let monitory: Vec<i64> = json("sources")
        .iter()
        .filter(|s| s["name"].as_str().is_some_and(|n| n.ends_with(".monitor")))
        .filter_map(|s| s["index"].as_i64())
        .collect();
    let mut apps: Vec<String> = json("source-outputs")
        .iter()
        .filter(|o| !o["corked"].as_bool().unwrap_or(false))
        .filter(|o| o["source"].as_i64().is_some_and(|s| !monitory.contains(&s)))
        .filter(|o| !o["properties"]["application.id"].as_str().unwrap_or("").contains("pavucontrol"))
        .map(|o| o["properties"]["application.name"].as_str().unwrap_or("An app").to_string())
        .collect();
    apps.dedup();
    let v = if apps.is_empty() { json!({"text": ""}) } else { json!({"text": "\u{f130}", "class": "using"}) };
    *APPS.lock().unwrap() = apps;
    publish("microphone", v);
}

pub fn watch() {
    std::thread::spawn(|| loop {
        detect();
        let mut c = Command::new("pactl");
        c.arg("subscribe").stdout(Stdio::piped()).stderr(Stdio::null());
        // end with the daemon (otherwise it would hang around after a restart)
        unsafe {
            use std::os::unix::process::CommandExt;
            c.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        if let Ok(mut p) = c.spawn() {
            if let Some(output) = p.stdout.take() {
                for row in BufReader::new(output).lines().map_while(Result::ok) {
                    // Event 'new' on source-output #42
                    if row.contains("source-output") {
                        detect();
                    }
                    // output volume or another default output: the sound icon in the bar
                    if row.contains(" on sink #") || row.contains(" on server") {
                        crate::bar::sources::volume_changed();
                    }
                }
            }
            let _ = p.wait();
        }
        // the sound server restarted: connect again
        std::thread::sleep(Duration::from_secs(2));
    });
}
