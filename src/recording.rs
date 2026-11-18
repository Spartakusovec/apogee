//! Screenshots and screen recording (grim, slurp, wf-recorder): picking an area, a window or the whole
//! screen, saving, clipboard and notifications. The bar shows a red dot with the time while
//! recording (click = stop).
//!
//! Screenshots: ~/Pictures/Screenshots, also in the clipboard; a click on the notification opens the editor (swappy).
//! Recordings: ~/Videos/Recordings, encoded on the Intel GPU (VA-API), on the CPU when that doesn't work.

use std::cell::RefCell;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use gtk::glib;
use serde_json::json;
use swayipc::{Connection, Node};

use crate::panel::publish;
use crate::common::*;

/// Intel (renderD128 is the NVIDIA, which is left asleep)
const GRAPHICS: &str = "/dev/dri/renderD129";

fn folder(xdg: &str, fallback: &str, subfolder: &str) -> PathBuf {
    let base = command("xdg-user-dir", &[xdg]);
    let base = if base.is_empty() { home().join(fallback) } else { PathBuf::from(base) };
    let s = base.join(subfolder);
    let _ = std::fs::create_dir_all(&s);
    s
}

fn time_tag() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}

/// What to capture: Some(Some("x,y wxh")), Some(None) = the whole active screen, None = cancelled.
/// Runs outside GTK (slurp waits for the mouse).
fn choice(mode: &str) -> Option<Option<String>> {
    match mode {
        "area" => {
            let o = Command::new("slurp").stderr(Stdio::null()).output().ok()?;
            o.status.success().then(|| Some(String::from_utf8_lossy(&o.stdout).trim().to_string()))
        }
        "window" => {
            // slurp offers only the rectangles of visible windows, a click picks one
            let rectangles = visible_windows().join("\n");
            let mut slurp = Command::new("slurp").arg("-r").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
            let _ = slurp.stdin.take()?.write_all(rectangles.as_bytes());
            let o = slurp.wait_with_output().ok()?;
            o.status.success().then(|| Some(String::from_utf8_lossy(&o.stdout).trim().to_string()))
        }
        _ => Some(None),
    }
}

fn visible_windows() -> Vec<String> {
    let Ok(strom) = Connection::new().and_then(|mut c| c.get_tree()) else { return Vec::new() };
    let mut result = Vec::new();
    fn walk(n: &Node, v: &mut Vec<String>) {
        if n.pid.is_some() && n.visible == Some(true) {
            let r = &n.rect;
            v.push(format!("{},{} {}x{}", r.x, r.y, r.width, r.height));
        }
        for d in n.nodes.iter().chain(n.floating_nodes.iter()) {
            walk(d, v);
        }
    }
    walk(&strom, &mut result);
    result
}

fn active_screen() -> Option<String> {
    Connection::new().ok()?.get_outputs().ok()?.into_iter().find(|o| o.focused).map(|o| o.name)
}

/// Screenshot. `delay` in seconds (after the area is picked), `modify` opens the editor.
pub fn screenshot(mode: &str, delay: u64, modify: bool) {
    crate::popups::close();
    let mode = mode.to_string();
    std::thread::spawn(move || {
        // so the popup has time to disappear and isn't in the shot
        std::thread::sleep(Duration::from_millis(300));
        let Some(oblast) = choice(&mode) else { return };
        std::thread::sleep(Duration::from_secs(delay));
        let file = folder("PICTURES", "Pictures", "Screenshots").join(format!("screenshot-{}.png", time_tag()));
        let mut grim = Command::new("grim");
        match &oblast {
            Some(g) => grim.args(["-g", g]),
            None => grim.args(["-o", &active_screen().unwrap_or_default()]),
        };
        if !grim.arg(&file).status().is_ok_and(|s| s.success()) {
            background("notify-send", &["-i", "dialog-error", "Screenshot failed"]);
            return;
        }
        if let Ok(f) = std::fs::File::open(&file) {
            let _ = Command::new("wl-copy").args(["--type", "image/png"]).stdin(f).status();
        }
        let path = file.to_string_lossy().to_string();
        if modify {
            background("swappy", &["-f", &path]);
        }
        // a click on the notification opens the editor (notify-send waits until the notification disappears)
        let action = command("notify-send", &["-i", &path, "-A", "default=Edit", "Screenshot saved", &format!("Copied to clipboard. Click to edit.\n{path}")]);
        if action == "default" {
            background("swappy", &["-f", &path]);
        }
    });
}

struct Recording {
    process: Child,
    file: PathBuf,
    start: Instant,
    args: Vec<String>,
    /// VA-API doesn't work, record on the CPU
    cpu: bool,
    /// stopped by the user (not an error)
    stopped: bool,
}

thread_local! {
    static RECORDING: RefCell<Option<Recording>> = const { RefCell::new(None) };
}

pub fn recording() -> bool {
    RECORDING.with(|n| n.borrow().is_some())
}

/// Starts recording (sound: off, system, microphone); if it's already recording, stops it.
pub fn record(mode: &str, sound: &str) {
    if recording() {
        stop();
        return;
    }
    crate::popups::close();
    let (mode, sound) = (mode.to_string(), sound.to_string());
    in_background(
        move || {
            std::thread::sleep(Duration::from_millis(300));
            let oblast = choice(&mode)?;
            let device = match sound.as_str() {
                "system" => Some(format!("{}.monitor", command("pactl", &["get-default-sink"]))),
                "microphone" => Some(command("pactl", &["get-default-source"])),
                _ => None,
            };
            Some((oblast, device, active_screen().unwrap_or_default()))
        },
        |choice| {
            let Some((oblast, device, screen)) = choice else { return };
            let file = folder("VIDEOS", "Videos", "Recordings").join(format!("recording-{}.mp4", time_tag()));
            let mut args: Vec<String> = Vec::new();
            match oblast {
                Some(g) => args.extend(["-g".into(), sude(&g)]),
                None => args.extend(["-o".into(), screen]),
            }
            if let Some(z) = device {
                args.push(format!("--audio={z}"));
            }
            args.extend(["-f".into(), file.to_string_lossy().to_string()]);
            start(args, file, false);
        },
    );
}

/// x264 wants even dimensions
fn sude(oblast: &str) -> String {
    let Some((location, size)) = oblast.split_once(' ') else { return oblast.to_string() };
    let Some((s, v)) = size.split_once('x') else { return oblast.to_string() };
    let sude = |t: &str| t.parse::<i64>().map(|c| (c / 2 * 2).max(2).to_string()).unwrap_or_else(|_| t.to_string());
    format!("{location} {}x{}", sude(s), sude(v))
}

fn start(args: Vec<String>, file: PathBuf, cpu: bool) {
    use std::os::unix::process::CommandExt;
    let mut c = Command::new("wf-recorder");
    c.arg("-y");
    if !cpu {
        c.args(["-c", "h264_vaapi", "-d", GRAPHICS]);
    }
    let process = c.args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn();
    let Ok(process) = process else {
        background("notify-send", &["-i", "dialog-error", "Screen recording failed", "wf-recorder could not be started."]);
        return;
    };
    RECORDING.with(|n| n.replace(Some(Recording { process, file, start: Instant::now(), args, cpu, stopped: false })));
    to_panel();
    glib::timeout_add_local(Duration::from_millis(500), || {
        if watch() {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

/// Is the recording still running? When it ended, report the result.
fn watch() -> bool {
    let finished = RECORDING.with(|n| {
        let mut n = n.borrow_mut();
        let x = n.as_mut()?;
        match x.process.try_wait() {
            Ok(None) => None,
            _ => n.take(),
        }
    });
    let Some(x) = finished else {
        to_panel();
        return true;
    };
    // VA-API doesn't work (it ended right away): try again on the CPU
    if !x.cpu && !x.stopped && x.start.elapsed() < Duration::from_secs(2) {
        start(x.args, x.file, true);
        return false;
    }
    to_panel();
    let path = x.file.to_string_lossy().to_string();
    if std::fs::metadata(&x.file).is_ok_and(|m| m.len() > 0) {
        std::thread::spawn(move || {
            let action = command("notify-send", &["-i", "video-x-generic", "-A", "default=Open", "Recording saved", &format!("Click to play.\n{path}")]);
            if action == "default" {
                background("xdg-open", &[&path]);
            }
        });
    } else {
        background("notify-send", &["-i", "dialog-error", "Screen recording failed"]);
    }
    false
}

/// Stops recording (wf-recorder finishes the file after Ctrl+C).
pub fn stop() {
    RECORDING.with(|n| {
        if let Some(x) = n.borrow_mut().as_mut() {
            x.stopped = true;
            unsafe { libc::kill(x.process.id() as i32, libc::SIGINT) };
        }
    });
}

/// Bar: a red dot with the time while recording.
fn to_panel() {
    match length() {
        Some(s) => publish("recording", json!({"text": format!("\u{f111}  {}:{:02}", s / 60, s % 60), "class": "recording"})),
        None => publish("recording", json!({"text": ""})),
    }
}

/// How many seconds it has been recording (None = not recording).
pub fn length() -> Option<u64> {
    RECORDING.with(|n| n.borrow().as_ref().map(|x| x.start.elapsed().as_secs()))
}

pub fn start_panel() {
    to_panel();
}
