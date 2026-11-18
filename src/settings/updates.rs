//! Updates page: apps and system (dnf + Flatpak, the same data as the bar popup),
//! the patched Sway (sway-patch: whether the installed version has the current patch, rebuild)
//! and firmware (fwupd: devices and available updates, installed through polkit).

use gtk::prelude::*;
use serde_json::Value;

use super::ui::*;
use crate::common::{in_background, command};

fn when(time: u64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    match now.saturating_sub(time) {
        0 => "never".into(),
        s if s < 120 => "just now".into(),
        s if s < 7200 => format!("{} minutes ago", s / 60),
        s if s < 172800 => format!("{} hours ago", s / 3600),
        s => format!("{} days ago", s / 86400),
    }
}

pub fn create() -> gtk::Widget {
    let s = page("Updates");

    // apps and system
    let v = crate::updates::load();
    let dnf = v["dnf"].as_array().cloned().unwrap_or_default();
    let flatpak = v["flatpak"].as_array().cloned().unwrap_or_default();
    let card = group(&s, "Apps and system", "Checked every 6 hours by itself.");
    let count = dnf.len() + flatpak.len();
    let check = button("Check now", false, || {
        super::notify("Checking for updates…");
        crate::updates::check(|_| {
            super::notify("Checked");
            super::refresh("updates");
        });
    });
    check.set_sensitive(!crate::updates::checking());
    let state = if count == 0 { "Everything is up to date".to_string() } else { format!("{count} updates ready") };
    add_row(&card, "\u{f019}", &state, &format!("Last checked {}", when(v["time"].as_u64().unwrap_or(0))), Some(check.upcast_ref()));
    if count > 0 {
        let list: Vec<String> = dnf.iter().chain(flatpak.iter()).filter_map(|x| x[0].as_str().map(String::from)).collect();
        let preview = if list.len() > 12 { format!("{} and {} more", list[..12].join(", "), list.len() - 12) } else { list.join(", ") };
        add_row(&card, "", &format!("{} system, {} Flatpak", dnf.len(), flatpak.len()), &preview, None);
        add_row(&card, "\u{f120}", "Install updates", "Opens a window that shows the progress and asks for your password", Some(button("Install", true, crate::updates::install).upcast_ref()));
    }

    // sway-krizek
    let card = group(&s, "Sway with title bar buttons", "Rebuilt by itself after every Sway update from Fedora.");
    // the new location (sway-patch); until install.sh has run, the old one (sway-krizek)
    let state = std::fs::read_to_string("/var/lib/sway-patch/state").or_else(|_| std::fs::read_to_string("/var/lib/sway-krizek/state")).unwrap_or_default();
    let mut parts = state.split_whitespace();
    let (version, fingerprint) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
    let patch = ["/usr/local/share/sway-patch/sway-buttons.patch", "/usr/local/share/sway-krizek/sway-krizek.patch"]
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|p| command("sha256sum", &[p]).chars().take(16).collect::<String>())
        .unwrap_or_default();
    let installed = command("rpm", &["-q", "--qf", "%{VERSION}-%{RELEASE}", "sway"]);
    let current = !version.is_empty() && version == installed && fingerprint == patch;
    let description = if current {
        format!("Sway {installed} has the latest buttons patch")
    } else if version.is_empty() {
        "Not built yet".to_string()
    } else {
        format!("Sway {installed} needs to be rebuilt with the latest patch")
    };
    let build = button("Rebuild", !current, || {
        super::notify("Building Sway – takes a few minutes, a notification says when it's done");
        in_background(
            || std::process::Command::new("pkexec").arg(if std::path::Path::new("/usr/local/libexec/sway-patch").exists() { "/usr/local/libexec/sway-patch" } else { "/usr/local/libexec/sway-krizek" }).output().map(|v| v.status.success()).unwrap_or(false),
            |ok| {
                super::notify(if ok { "Sway rebuilt – log out and back in to use it" } else { "Rebuild failed or was cancelled" });
                super::refresh("updates");
            },
        );
    });
    add_row(&card, if current { "\u{f00c}" } else { "\u{f071}" }, if current { "Up to date" } else { "Rebuild needed" }, &description, Some(build.upcast_ref()));

    // firmware
    let card = group(&s, "Firmware", "Updates for the BIOS, disk and other parts, from the Linux Vendor Firmware Service. Needs the charger plugged in.");
    loading(&card, "Looking for firmware updates…");
    let k2 = card.clone();
    in_background(
        || {
            let device = command("fwupdmgr", &["get-devices", "--json"]);
            let updates = command("fwupdmgr", &["get-updates", "--json"]);
            (serde_json::from_str::<Value>(&device).unwrap_or_default(), serde_json::from_str::<Value>(&updates).unwrap_or_default())
        },
        move |(device, updates)| {
            if k2.root().is_none() {
                return;
            }
            clear(&k2);
            let updates = updates["Devices"].as_array().cloned().unwrap_or_default();
            for z in updates.iter() {
                let name = z["Name"].as_str().unwrap_or("Device").to_string();
                let nova = z["Releases"][0]["Version"].as_str().unwrap_or("").to_string();
                let id = z["DeviceId"].as_str().unwrap_or("").to_string();
                let install = button("Update", false, move || {
                    super::notify("Updating firmware – keep the charger plugged in…");
                    let id = id.clone();
                    in_background(
                        move || std::process::Command::new("fwupdmgr").args(["update", &id, "-y", "--no-reboot-check"]).output().map(|v| v.status.success()).unwrap_or(false),
                        |ok| super::notify(if ok { "Firmware updated – restart to finish" } else { "Firmware update failed" }),
                    );
                });
                add_row(&k2, "\u{f2db}", &name, &format!("{} → {nova}", z["Version"].as_str().unwrap_or("")), Some(install.upcast_ref()));
            }
            if updates.is_empty() {
                add_row(&k2, "\u{f00c}", "No firmware updates", "", None);
            }
            // updatable devices (without duplicates)
            let mut names: Vec<String> = device["Devices"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["Flags"].as_array().is_some_and(|f| f.iter().any(|x| x == "updatable")))
                .filter_map(|d| Some(format!("{} {}", d["Name"].as_str()?, d["Version"].as_str().unwrap_or(""))))
                .collect();
            names.dedup();
            if !names.is_empty() {
                add_row(&k2, "", "Devices that can get updates", &names.join("\n"), None);
            }
        },
    );
    s.upcast()
}
