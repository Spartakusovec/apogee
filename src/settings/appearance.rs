//! Appearance page: wallpaper (and the colours of the whole desktop from it, set-wallpaper), a custom
//! picture, app style (dark / light), text size, pointer size and window buttons.

use std::path::PathBuf;
use gtk::prelude::*;

use super::sway_conf;
use super::ui::*;
use crate::popups::wallpapers::{thumbnail, folder_tapet, wallpapers};
use crate::common::{home, in_background, command};

const INTERFACE: &str = "org.gnome.desktop.interface";

fn gsettings(schema: &str, key: &str) -> String {
    command("gsettings", &["get", schema, key]).trim().trim_matches('\'').to_string()
}

fn set_gsettings(schema: &str, key: &str, value: &str) {
    crate::common::background("gsettings", &["set", schema, key, value]);
}

/// Sets the wallpaper (colours, lock screen, Sway reload take a few seconds).
fn apply_wallpaper(path: PathBuf) {
    apply(path, "Wallpaper changed");
}

fn apply(path: PathBuf, done: &'static str) {
    super::notify("Applying the colors…");
    let program = home().join(".local/bin/set-wallpaper").to_string_lossy().to_string();
    in_background(
        move || command(&program, &[&path.to_string_lossy()]),
        move |_| {
            super::notify(done);
            super::refresh("appearance");
        },
    );
}

pub fn create() -> gtk::Widget {
    let s = page("Appearance");

    // tapety
    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let n = gtk::Label::new(Some("Wallpaper"));
    n.add_css_class("title-groups");
    n.set_xalign(0.0);
    wrapper.append(&n);
    let p = gtk::Label::new(Some("The colors of the panel, dock, windows and lock screen follow the wallpaper."));
    p.add_css_class("description");
    p.set_xalign(0.0);
    wrapper.append(&p);
    let grid = gtk::FlowBox::new();
    grid.set_selection_mode(gtk::SelectionMode::None);
    grid.set_min_children_per_line(3);
    grid.set_max_children_per_line(3);
    grid.set_row_spacing(10);
    grid.set_column_spacing(10);
    grid.set_homogeneous(true);
    let current = std::fs::canonicalize(home().join(".config/theme/wallpaper")).ok();
    let list = wallpapers();
    let mut images = Vec::new();
    for t in &list {
        let b = gtk::Button::new();
        b.add_css_class("wallpaper");
        b.set_overflow(gtk::Overflow::Hidden);
        b.set_tooltip_text(t.file_stem().map(|x| x.to_string_lossy().to_string()).as_deref());
        if current.as_ref() == std::fs::canonicalize(t).ok().as_ref() {
            b.add_css_class("active-wallpaper");
        }
        let image = gtk::Picture::new();
        image.set_content_fit(gtk::ContentFit::Cover);
        image.set_size_request(184, 104);
        b.set_child(Some(&image));
        let (path, m) = (t.clone(), grid.clone());
        b.connect_clicked(move |b| {
            let mut d = m.first_child();
            while let Some(w) = d {
                if let Some(tl) = w.first_child() {
                    tl.remove_css_class("active-wallpaper");
                }
                d = w.next_sibling();
            }
            b.add_css_class("active-wallpaper");
            apply_wallpaper(path.clone());
        });
        grid.append(&b);
        images.push(image);
    }
    wrapper.append(&grid);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.append(&button("Add a picture…", true, || {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Choose a wallpaper");
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Pictures"));
        filter.add_mime_type("image/*");
        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        dialog.open(super::window().as_ref(), gtk::gio::Cancellable::NONE, |v| {
            let Some(source) = v.ok().and_then(|f| f.path()) else { return };
            // copy it to the wallpaper folder, so it's offered next time too
            let folder = folder_tapet();
            let _ = std::fs::create_dir_all(&folder);
            let target = folder.join(source.file_name().unwrap_or_default());
            if target != source && std::fs::copy(&source, &target).is_err() {
                super::notify("Couldn't copy the picture");
                return;
            }
            apply_wallpaper(target);
        });
    }));
    buttons.append(&button("Open the wallpaper folder", false, || {
        let s = folder_tapet();
        let _ = std::fs::create_dir_all(&s);
        crate::common::background("xdg-open", &[&s.to_string_lossy()]);
    }));
    wrapper.append(&buttons);
    s.append(&wrapper);
    let paths = list.clone();
    in_background(
        move || paths.iter().map(thumbnail).collect::<Vec<_>>(),
        move |thumbnails| {
            for (o, n) in images.iter().zip(thumbnails) {
                if let Some(n) = n {
                    o.set_filename(Some(&n));
                }
            }
        },
    );

    // aplikace
    let card = group(&s, "Look", "Colors always come from the wallpaper, in a dark or a light version. Apps like Files and Firefox use the wallpaper's accent color too.");
    let dark = gsettings(INTERFACE, "color-scheme") == "prefer-dark";
    let style = gtk::DropDown::from_strings(&["Dark", "Light"]);
    style.set_selected(!dark as u32);
    style.connect_selected_notify(|d| {
        let dark = d.selected() == 0;
        // right away (not in the background): theme.py reads the mode from gsettings
        command("gsettings", &["set", INTERFACE, "color-scheme", if dark { "prefer-dark" } else { "default" }]);
        // GTK 3 apps have their own theme
        if gsettings(INTERFACE, "gtk-theme").starts_with("adw-gtk3") {
            set_gsettings(INTERFACE, "gtk-theme", if dark { "adw-gtk3-dark" } else { "adw-gtk3" });
        }
        // set-wallpaper switches the icons (Papirus-Wallpaper / -Dark) through scripts/icons.sh
        // bar, dock, windows, lock screen and this window too: a new palette from the wallpaper (dark / light)
        if let Ok(t) = std::fs::canonicalize(home().join(".config/theme/wallpaper")) {
            apply(t, if dark { "Dark style on" } else { "Light style on" });
        }
    });
    add_row(&card, "\u{f042}", "Style", "The whole desktop and the apps", Some(style.upcast_ref()));

    let sizes = [("100 %", 1.0), ("110 %", 1.1), ("125 %", 1.25), ("150 %", 1.5)];
    let now: f64 = gsettings(INTERFACE, "text-scaling-factor").parse().unwrap_or(1.0);
    let text = gtk::DropDown::from_strings(&sizes.iter().map(|v| v.0).collect::<Vec<_>>());
    text.set_selected(sizes.iter().position(|v| (v.1 - now).abs() < 0.01).unwrap_or(0) as u32);
    text.connect_selected_notify(move |d| {
        set_gsettings(INTERFACE, "text-scaling-factor", &format!("{}", sizes[d.selected() as usize].1));
        super::notify("Text size changed – some apps need to be reopened");
    });
    add_row(&card, "\u{f031}", "Text size", "Bigger text in apps without making everything bigger", Some(text.upcast_ref()));

    let wm = "org.gnome.desktop.wm.preferences";
    let layouts = [("Minimize, maximize and close", "appmenu:minimize,maximize,close"), ("Close only", "appmenu:close")];
    let now = gsettings(wm, "button-layout");
    let tl = gtk::DropDown::from_strings(&layouts.iter().map(|v| v.0).collect::<Vec<_>>());
    tl.set_selected(layouts.iter().position(|v| v.1 == now).unwrap_or(0) as u32);
    tl.connect_selected_notify(move |d| {
        set_gsettings(wm, "button-layout", layouts[d.selected() as usize].1);
        super::notify("Saved");
    });
    add_row(&card, "\u{f2d0}", "Window buttons", "In apps that draw their own title bar (Files, Settings…)", Some(tl.upcast_ref()));

    // kurzor
    let card = group(&s, "Pointer", "");
    let cursors = [("Normal", 24), ("Large", 32), ("Larger", 40), ("Largest", 48)];
    let now: i64 = gsettings(INTERFACE, "cursor-size").parse().unwrap_or(24);
    let k = gtk::DropDown::from_strings(&cursors.iter().map(|v| v.0).collect::<Vec<_>>());
    k.set_selected(cursors.iter().position(|v| v.1 == now).unwrap_or(0) as u32);
    k.connect_selected_notify(move |d| {
        let v = cursors[d.selected() as usize].1;
        set_gsettings(INTERFACE, "cursor-size", &v.to_string());
        let tema = gsettings(INTERFACE, "cursor-theme");
        sway_conf::set("seat seat0 xcursor_theme", &format!("{tema} {v}"));
        super::notify("Pointer size changed – some apps need to be reopened");
    });
    add_row(&card, "\u{f245}", "Pointer size", "", Some(k.upcast_ref()));

    s.upcast()
}
