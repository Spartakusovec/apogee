//! About page: computer (model, CPU, memory, graphics and whether the NVIDIA sleeps, disk), system
//! (Fedora, kernel, Sway with the patch, uptime) and backup (~/.dotfiles: last backup, back up
//! now, check the restore). “Copy” copies the summary to the clipboard.

use gtk::prelude::*;

use super::ui::*;
use crate::common::{home, in_background, command};

struct Info {
    model: String,
    cpu: String,
    memory: String,
    graphics: Vec<String>,
    nvidia_spi: Option<bool>,
    disk: String,
    system: String,
    kernel: String,
    sway: String,
    running: String,
    name: String,
    backup: String,
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

fn load() -> Info {
    let model = read("/sys/class/dmi/id/product_version");
    let model = if model.is_empty() { format!("{} {}", read("/sys/class/dmi/id/sys_vendor"), read("/sys/class/dmi/id/product_name")) } else { model };
    let cpuinfo = read("/proc/cpuinfo");
    let cpu = cpuinfo.lines().find_map(|r| r.strip_prefix("model name").and_then(|r| r.split_once(':')).map(|(_, v)| v.trim().to_string())).unwrap_or_default();
    let cores = cpuinfo.lines().filter(|r| r.starts_with("processor")).count();
    let cpu = cpu.replace("(R)", "").replace("(TM)", "").replace(" CPU", "");
    let kb = read("/proc/meminfo").lines().find_map(|r| r.strip_prefix("MemTotal:")).and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(0.0);
    // graphics from sysfs and pci.ids – not lspci: that reads the card's config space and wakes a sleeping NVIDIA
    let mut graphics = Vec::new();
    let mut nvidia_spi = None;
    let mut device: Vec<_> = std::fs::read_dir("/sys/bus/pci/devices").map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    device.sort();
    for z in device {
        let c = |s: &str| read(&format!("{}/{s}", z.display()));
        if !c("class").starts_with("0x03") {
            continue;
        }
        let (vendor, number) = (c("vendor"), c("device"));
        if vendor == "0x10de" {
            nvidia_spi = Some(c("power/runtime_status") == "suspended");
        }
        graphics.push(name_pci(vendor.trim_start_matches("0x"), number.trim_start_matches("0x")));
    }
    let df = command("df", &["-h", "--output=size,avail", "/"]);
    let disk = df.lines().nth(1).map(|r| {
        let c: Vec<&str> = r.split_whitespace().collect();
        let gb = |x: &str| format!("{} {}B", x.trim_end_matches(char::is_alphabetic), x.trim_start_matches(|z: char| z.is_ascii_digit() || z == '.'));
        format!("{}, {} free", gb(c.first().unwrap_or(&"")), gb(c.get(1).unwrap_or(&"")))
    }).unwrap_or_default();
    let os = read("/etc/os-release");
    let system = os.lines().find_map(|r| r.strip_prefix("PRETTY_NAME=")).unwrap_or("").trim_matches('"').to_string();
    let seconds: f64 = read("/proc/uptime").split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let s = seconds as u64;
    let running = if s >= 86400 { format!("{} days {} h", s / 86400, s / 3600 % 24) } else { format!("{} h {} min", s / 3600, s / 60 % 60) };
    let rpm = command("rpm", &["-q", "--qf", "%{VERSION}-%{RELEASE}", "sway"]);
    let sway = if rpm.contains(".fc") && command("rpm", &["-q", "--qf", "%{RELEASE}", "sway"]).split('.').count() > 2 {
        format!("{rpm}  ·  with the title bar buttons patch")
    } else {
        rpm
    };
    let backup = command("git", &["-C", &home().join(".dotfiles").to_string_lossy(), "log", "-1", "--format=%cr"]);
    Info {
        model,
        cpu: format!("{cpu}  ·  {cores} cores"),
        memory: format!("{:.1} GB", kb / 1024.0 / 1024.0),
        graphics,
        nvidia_spi,
        disk,
        system,
        kernel: command("uname", &["-r"]),
        sway,
        running,
        name: read("/etc/hostname"),
        backup,
    }
}

pub fn create() -> gtk::Widget {
    let s = page("About");
    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.append(&wrapper);
    let transient = group(&wrapper, "", "");
    loading(&transient, "Loading…");
    let o2 = wrapper.clone();
    in_background(load, move |u| {
        if o2.root().is_none() {
            return;
        }
        while let Some(d) = o2.first_child() {
            o2.remove(&d);
        }
        show(&o2, &u);
    });
    s.upcast()
}

fn value(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("description");
    l.set_selectable(true);
    l.set_wrap(true);
    l.set_xalign(1.0);
    l.set_max_width_chars(40);
    l
}

fn show(s: &gtk::Box, u: &Info) {
    let card = group(s, "Computer", "");
    add_row(&card, "\u{f109}", &u.model, &u.name, None);
    add_row(&card, "", "Processor", "", Some(value(&u.cpu).upcast_ref()));
    add_row(&card, "", "Memory", "", Some(value(&u.memory).upcast_ref()));
    add_row(&card, "", "Graphics", "", Some(value(&u.graphics.join("\n")).upcast_ref()));
    if let Some(spi) = u.nvidia_spi {
        let description = if spi { "Asleep – saving battery" } else { "Awake – something is using it, or it is waking up" };
        add_row(&card, "", "NVIDIA graphics", "Only games and apps that ask for it wake it up", Some(value(description).upcast_ref()));
    }
    add_row(&card, "", "Disk", "", Some(value(&u.disk).upcast_ref()));

    let card = group(s, "Software", "");
    add_row(&card, "", "System", "", Some(value(&u.system).upcast_ref()));
    add_row(&card, "", "Linux kernel", "", Some(value(&u.kernel).upcast_ref()));
    add_row(&card, "", "Sway", "", Some(value(&u.sway).upcast_ref()));
    add_row(&card, "", "Running for", "", Some(value(&u.running).upcast_ref()));
    let overview = format!(
        "{}\nProcessor: {}\nMemory: {}\nGraphics: {}\nDisk: {}\nSystem: {}\nKernel: {}\nSway: {}",
        u.model, u.cpu, u.memory, u.graphics.join(", "), u.disk, u.system, u.kernel, u.sway
    );
    let copy = button("Copy", false, move || {
        if let Some(d) = gtk::gdk::Display::default() {
            d.clipboard().set_text(&overview);
            super::notify("Copied – paste it where someone asks about your computer");
        }
    });
    add_row(&card, "\u{f0c5}", "Copy these details", "", Some(copy.upcast_ref()));

    let card = group(s, "Backup", "Settings, scripts and the list of installed apps are saved to ~/.dotfiles and GitHub, so the whole setup can be restored on a fresh Fedora.");
    let last = if u.backup.is_empty() { "Never".to_string() } else { u.backup.clone() };
    let backup = button("Back up now", true, || {
        super::notify("Backing up…");
        let script = home().join(".dotfiles/backup.sh").to_string_lossy().to_string();
        in_background(
            move || command("bash", &[&script]),
            |output| {
                super::notify(output.lines().filter(|r| !r.trim().is_empty()).last().unwrap_or("Done"));
                super::refresh("about");
            },
        );
    });
    add_row(&card, "\u{f0c7}", "Last backup", &last, Some(backup.upcast_ref()));
    let check = button("Check", false, || {
        super::notify("Checking…");
        let script = home().join(".dotfiles/restore.sh").to_string_lossy().to_string();
        in_background(
            // the check script exits with 0 when the backup is complete
            move || std::process::Command::new("bash").args([&script, "--check"]).output().map(|o| o.status.success()).unwrap_or(false),
            |ok| {
                super::notify(if ok { "The backup has everything a restore needs" } else { "Something is missing from the backup – run restore.sh --check to see what" });
            },
        );
    });
    add_row(&card, "\u{f00c}", "Check the backup", "Makes sure a restore would have everything it needs", Some(check.upcast_ref()));
}

/// Graphics name from /usr/share/hwdata/pci.ids: “Intel HD Graphics 630”, “NVIDIA GeForce GTX 1050 Mobile”.
fn name_pci(vendor: &str, number: &str) -> String {
    let ids = std::fs::read_to_string("/usr/share/hwdata/pci.ids").unwrap_or_default();
    let mut firma = String::new();
    let mut v_section = false;
    for r in ids.lines() {
        if !r.starts_with('\t') && !r.starts_with('#') {
            v_section = r.starts_with(&format!("{vendor}  "));
            if v_section {
                firma = r[vendor.len()..].trim().split_whitespace().next().unwrap_or("").to_string();
            } else if !firma.is_empty() {
                break;
            }
        } else if v_section && !r.starts_with("\t\t") {
            if let Some(name) = r.trim_start().strip_prefix(&format!("{number}  ")) {
                let name = match (name.find('['), name.rfind(']')) {
                    (Some(a), Some(b)) if b > a => &name[a + 1..b],
                    _ => name,
                };
                return format!("{firma} {name}");
            }
        }
    }
    format!("{firma} graphics ({vendor}:{number})").trim().to_string()
}
