//! Power & Battery page: battery state and wear, power mode (tuned through power-mode.sh),
//! Lenovo conservation mode (password through polkit, no terminal) and when to lock
//! and turn off the screen (~/.config/sway/idle.conf, swayidle through idle.sh).

use std::io::Write;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use super::ui::*;
use crate::common::{battery, home, in_background, background, mode_power, scripts};

const BAT: &str = "/sys/class/power_supply/BAT0";
const CONSERVATION: &str = "/sys/bus/platform/drivers/ideapad_acpi/VPC2004:00/conservation_mode";

fn read(path: &str) -> Option<f64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn duration(s: i64) -> String {
    if s >= 3600 { format!("{} h {} min", s / 3600, s / 60 % 60) } else { format!("{} min", s / 60) }
}

pub fn create() -> gtk::Widget {
    let s = page("Power & Battery");

    // baterie
    let card = group(&s, "Battery", "");
    if let Some(b) = battery() {
        let state = match b.state {
            1 => "Charging",
            2 | 6 => "On battery",
            4 => "Fully charged",
            5 => "Plugged in, not charging",
            _ => "",
        };
        let mut description = vec![state.to_string()];
        let remaining = if b.state == 2 { b.to_empty } else { b.to_full };
        if remaining > 0 {
            description.push(format!("{} {}", if b.state == 2 { "empty in" } else { "full in" }, duration(remaining)));
        }
        let level = gtk::LevelBar::for_interval(0.0, 100.0);
        level.set_mode(gtk::LevelBarMode::Continuous);
        for n in ["low", "high", "full"] {
            level.remove_offset_value(Some(n));
        }
        level.set_value(b.percent);
        level.set_size_request(160, -1);
        add_row(&card, "\u{f240}", &format!("{:.0} %", b.percent), &description.join("  ·  "), Some(level.upcast_ref()));
        if b.power_draw > 0.0 {
            let (name, description) = if b.state == 1 { ("Charging speed", format!("{:.1} W into the battery", b.power_draw)) } else { ("Power draw", format!("{:.1} W right now", b.power_draw)) };
            add_row(&card, "\u{f0e7}", name, &description, None);
        }
        if let (Some(full), Some(original)) = (read(&format!("{BAT}/energy_full")), read(&format!("{BAT}/energy_full_design"))) {
            let health = full / original * 100.0;
            let rada = if health < 80.0 { "  ·  Battery care below helps it last longer" } else { "" };
            add_row(&card, "\u{f21e}", &format!("Battery health {health:.0} %"), &format!("Holds {:.0} Wh of the original {:.0} Wh{rada}", full / 1e6, original / 1e6), None);
        }
    } else {
        add_row(&card, "", "No battery found", "", None);
    }

    // power mode
    let card = group(&s, "Power mode", "On battery, Balanced saves power by itself.");
    let current = mode_power();
    for (mode, name, description) in [
        ("power-saver", "Power saver", "Longest battery life, slower"),
        ("balanced", "Balanced", "Recommended"),
        ("performance", "Performance", "Fastest, uses more battery and runs warmer"),
    ] {
        let selected = mode == current;
        action_row(&card, if selected { "\u{f00c}" } else { "" }, name, description, "", move || {
            background(&scripts().join("power-mode.sh").to_string_lossy(), &[mode]);
            super::notify(&format!("Power mode: {name}"));
            glib::timeout_add_local_once(Duration::from_millis(500), || super::refresh("power"));
        });
    }

    // battery care (only Lenovo laptops with ideapad_acpi)
    if let Some(enabled) = read(CONSERVATION) {
        let card = group(&s, "Battery care", "");
        switch_row(
            &card,
            "Conservation mode",
            "Charges only to about 60 %. Good if the laptop is mostly plugged in; turn it off before a trip for a full charge.",
            enabled > 0.0,
            |state| {
                super::notify("Waiting for your password…");
                in_background(
                    move || {
                        // writing to /sys needs root: pkexec asks for the password in a normal dialog
                        std::process::Command::new("pkexec")
                            .args(["/usr/bin/tee", CONSERVATION])
                            .stdin(std::process::Stdio::piped())
                            .stdout(std::process::Stdio::null())
                            .spawn()
                            .and_then(|mut p| {
                                if let Some(mut input) = p.stdin.take() {
                                    input.write_all(if state { b"1" } else { b"0" })?;
                                }
                                p.wait()
                            })
                            .map(|s| s.success())
                            .unwrap_or(false)
                    },
                    move |ok| {
                        super::notify(match (ok, state) {
                            (true, true) => "Conservation mode on – charging stops at about 60 %",
                            (true, false) => "Conservation mode off – charging to 100 %",
                            (false, _) => "Not changed",
                        });
                        super::refresh("power");
                    },
                );
            },
        );
    }

    // idle
    let card = group(&s, "When idle", "Not while a video or music is playing. The screen dims 20 seconds before it locks.");
    let (lock, power) = idle();
    let choices: [(&str, u32); 6] = [("2 minutes", 120), ("5 minutes", 300), ("10 minutes", 600), ("15 minutes", 900), ("30 minutes", 1800), ("Never", 0)];
    let choice = |current: u32, change: fn(u32)| {
        let d = gtk::DropDown::from_strings(&choices.iter().map(|v| v.0).collect::<Vec<_>>());
        d.set_selected(choices.iter().position(|v| v.1 == current).unwrap_or(2) as u32);
        d.connect_selected_notify(move |d| change(choices[d.selected() as usize].1));
        d
    };
    add_row(&card, "\u{f023}", "Lock the screen after", "", Some(choice(lock, |v| save_idle(Some(v), None)).upcast_ref()));
    add_row(&card, "\u{f108}", "Turn the screen off after", "", Some(choice(power, |v| save_idle(None, Some(v))).upcast_ref()));
    s.upcast()
}

fn file_idle() -> std::path::PathBuf {
    home().join(".config/sway/idle.conf")
}

/// (lock, screen off) in seconds, 0 = never
fn idle() -> (u32, u32) {
    let t = std::fs::read_to_string(file_idle()).unwrap_or_default();
    let value = |k: &str, default: u32| t.lines().find_map(|r| r.strip_prefix(&format!("{k}="))).and_then(|v| v.trim().parse().ok()).unwrap_or(default);
    (value("lock", 600), value("screen_off", 900))
}

fn save_idle(lock: Option<u32>, power: Option<u32>) {
    let (z, v) = idle();
    let (z, v) = (lock.unwrap_or(z), power.unwrap_or(v));
    let text = format!("# Idle times in seconds (0 = never), changed by the Settings app; see scripts/idle.sh\nlock={z}\nscreen_off={v}\n");
    if std::fs::write(file_idle(), text).is_ok() {
        // swayidle again with the new times (in the Sway session, not as a child of the daemon)
        crate::sway::command(&format!("exec {}", scripts().join("idle.sh").to_string_lossy()));
        super::notify("Saved");
    } else {
        super::notify("Couldn't save the setting");
    }
}
