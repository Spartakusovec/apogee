//! Storage page: disk usage, what takes the space (folders, Flatpak apps, caches, trash;
//! sizes by du in the background) and freeing space (trash, thumbnails, unused Flatpak runtimes,
//! the dnf cache).

use gtk::prelude::*;

use super::ui::*;
use crate::common::{home, in_background, command};

/// Size in bytes (du; errors and unreadable files are ignored).
fn size(path: &str) -> u64 {
    command("du", &["-sb", path]).split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0)
}

fn readable(b: u64) -> String {
    let b = b as f64;
    if b >= 1e9 { format!("{:.1} GB", b / 1e9) } else if b >= 1e6 { format!("{:.0} MB", b / 1e6) } else { format!("{:.0} kB", b / 1e3) }
}

struct Data {
    total: u64,
    free: u64,
    items: Vec<(&'static str, &'static str, String, u64)>,
    kos: u64,
    thumbnails: u64,
    dnf: u64,
}

fn load() -> Data {
    let df = command("df", &["-B1", "--output=size,avail", "/"]);
    let c: Vec<u64> = df.lines().nth(1).unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let d = home();
    let folder = |kind: &str, default: &str| {
        let p = command("xdg-user-dir", &[kind]);
        if p.is_empty() || p == d.to_string_lossy() { d.join(default).to_string_lossy().to_string() } else { p }
    };
    let mut items: Vec<(&str, &str, String, u64)> = vec![
        ("\u{f019}", "Downloads", folder("DOWNLOAD", "Downloads")),
        ("\u{f03d}", "Videos", folder("VIDEOS", "Videos")),
        ("\u{f03e}", "Pictures", folder("PICTURES", "Pictures")),
        ("\u{f15b}", "Documents", folder("DOCUMENTS", "Documents")),
        ("\u{f001}", "Music", folder("MUSIC", "Music")),
        ("\u{f00a}", "Flatpak apps (system)", "/var/lib/flatpak".to_string()),
        ("\u{f00a}", "Flatpak apps (yours)", d.join(".local/share/flatpak").to_string_lossy().to_string()),
        ("\u{f07c}", "App data", d.join(".var/app").to_string_lossy().to_string()),
        ("\u{f1c0}", "Caches", d.join(".cache").to_string_lossy().to_string()),
    ]
    // du on all folders at once (flatpak and the caches take longest)
    .into_iter()
    .map(|(i, n, p)| std::thread::spawn(move || {
        let v = size(&p);
        (i, n, p, v)
    }))
    .collect::<Vec<_>>()
    .into_iter()
    .filter_map(|h| h.join().ok())
    .filter(|x| x.3 > 0)
    .collect();
    items.sort_by_key(|x| std::cmp::Reverse(x.3));
    Data {
        total: c.first().copied().unwrap_or(0),
        free: c.get(1).copied().unwrap_or(0),
        items,
        kos: size(&d.join(".local/share/Trash").to_string_lossy()),
        thumbnails: size(&d.join(".cache/thumbnails").to_string_lossy()),
        dnf: size("/var/cache/libdnf5"),
    }
}

/// Clean-up in the background, then a toast and a page rebuild.
fn cleanup(program: &'static str, args: &'static [&'static str], done: &'static str) {
    super::notify("Cleaning up…");
    in_background(
        move || std::process::Command::new(program).args(args).output().map(|v| v.status.success()).unwrap_or(false),
        move |ok| {
            super::notify(if ok { done } else { "Not cleaned" });
            super::refresh("storage");
        },
    );
}

pub fn create() -> gtk::Widget {
    let s = page("Storage");
    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.append(&wrapper);
    let k = group(&wrapper, "", "");
    loading(&k, "Measuring… (takes a few seconds)");
    let o2 = wrapper.clone();
    in_background(load, move |d| {
        if o2.root().is_none() {
            return;
        }
        while let Some(x) = o2.first_child() {
            o2.remove(&x);
        }
        show(&o2, &d);
    });
    s.upcast()
}

fn show(s: &gtk::Box, d: &Data) {
    let card = group(s, "", "");
    let used = d.total.saturating_sub(d.free);
    let level = gtk::LevelBar::for_interval(0.0, 1.0);
    for n in ["low", "high", "full"] {
        level.remove_offset_value(Some(n));
    }
    level.set_value(if d.total > 0 { used as f64 / d.total as f64 } else { 0.0 });
    level.set_size_request(200, -1);
    add_row(&card, "\u{f0a0}", &format!("{} free", readable(d.free)), &format!("{} of {} used", readable(used), readable(d.total)), Some(level.upcast_ref()));

    let card = group(s, "What uses the space", "");
    for (icon, name, path, v) in &d.items {
        let c = path.clone();
        let open = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let l = gtk::Label::new(Some(&readable(*v)));
        l.add_css_class("description");
        open.append(&l);
        if !path.starts_with("/var") {
            open.append(&button("Open", false, move || crate::common::background("xdg-open", &[&c])));
        }
        add_row(&card, icon, name, "", Some(open.upcast_ref()));
    }

    let card = group(s, "Free up space", "");
    let kos = button("Empty", false, || cleanup("gio", &["trash", "--empty"], "Trash emptied"));
    kos.set_sensitive(d.kos > 0);
    add_row(&card, "\u{f1f8}", "Trash", &readable(d.kos), Some(kos.upcast_ref()));
    let thumbnails = button("Clear", false, || {
        let p = home().join(".cache/thumbnails");
        let _ = std::fs::remove_dir_all(&p);
        super::notify("Thumbnails cleared – they're made again when needed");
        super::refresh("storage");
    });
    add_row(&card, "\u{f03e}", "Picture thumbnails", &readable(d.thumbnails), Some(thumbnails.upcast_ref()));
    add_row(&card, "\u{f00a}", "Unused Flatpak parts", "Old runtimes no app needs any more", Some(button("Remove", false, || cleanup("flatpak", &["uninstall", "--unused", "-y", "--noninteractive"], "Unused Flatpak parts removed")).upcast_ref()));
    add_row(&card, "\u{f019}", "Downloaded system updates", &format!("{} – asks for your password", readable(d.dnf)), Some(button("Clear", false, || cleanup("pkexec", &["dnf", "clean", "packages"], "Update downloads cleared")).upcast_ref()));
}
