//! Wallpaper choice: thumbnails of the pictures in ~/Pictures/Wallpapers; a click sets the wallpaper and the
//! colours of the whole desktop from it (set-wallpaper). Thumbnails are saved in ~/.cache/apogee/wallpapers.

use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;

use crate::common::*;

const WIDTH: i32 = 172;
const HEIGHT: i32 = 97;

pub fn folder_tapet() -> PathBuf {
    let images = command("xdg-user-dir", &["PICTURES"]);
    let images = if images.is_empty() { home().join("Pictures") } else { PathBuf::from(images) };
    images.join("Wallpapers")
}

pub fn wallpapers() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(folder_tapet())
        .map(|d| {
            d.flatten()
                .map(|s| s.path())
                .filter(|p| {
                    let extension = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "webp")
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Thumbnail (creates it if missing; slow, call outside GTK).
pub fn thumbnail(wallpaper: &PathBuf) -> Option<PathBuf> {
    let change = std::fs::metadata(wallpaper).and_then(|m| m.modified()).ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    let folder = home().join(".cache/apogee/wallpapers");
    let file = folder.join(format!("{}-{change}.jpg", wallpaper.file_stem()?.to_string_lossy()));
    if !file.exists() {
        let _ = std::fs::create_dir_all(&folder);
        let image = gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(wallpaper, WIDTH * 2, -1, true).ok()?;
        image.savev(&file, "jpeg", &[("quality", "88")]).ok()?;
    }
    Some(file)
}

pub fn fill(o: &Rc<Popup>) {
    o.panel.append(&title("Wallpaper", "The colors of the whole desktop follow the wallpaper"));
    let list = wallpapers();
    if list.is_empty() {
        o.state("No pictures in Pictures → Wallpapers yet");
    }
    let current = std::fs::canonicalize(home().join(".config/theme/wallpaper")).ok();
    let grid = gtk::Grid::new();
    grid.set_row_spacing(10);
    grid.set_column_spacing(10);
    let mut images = Vec::new();
    for (i, wallpaper) in list.iter().enumerate() {
        let b = gtk::Button::new();
        b.add_css_class("wallpaper");
        b.set_overflow(gtk::Overflow::Hidden);
        if current.as_ref() == std::fs::canonicalize(wallpaper).ok().as_ref() {
            b.add_css_class("active-wallpaper");
        }
        let image = gtk::Picture::new();
        image.set_content_fit(gtk::ContentFit::Cover);
        image.set_size_request(WIDTH, HEIGHT);
        b.set_child(Some(&image));
        let (path, weak, grid2) = (wallpaper.to_string_lossy().to_string(), Rc::downgrade(o), grid.clone());
        b.connect_clicked(move |b| {
            let Some(o) = weak.upgrade() else { return };
            // recolouring takes a moment (blurred wallpaper for the lock screen, new colours, Sway reload)
            let mut d = grid2.first_child();
            while let Some(w) = d {
                w.remove_css_class("active-wallpaper");
                d = w.next_sibling();
            }
            b.add_css_class("active-wallpaper");
            grid2.set_sensitive(false);
            o.keep_open.set(true);
            o.state("Applying the wallpaper and colors…");
            let path = path.clone();
            let program = home().join(".local/bin/set-wallpaper").to_string_lossy().to_string();
            let weak = Rc::downgrade(&o);
            in_background(
                move || command(&program, &[&path]),
                move |_| {
                    if let Some(o) = weak.upgrade() {
                        o.close();
                    }
                },
            );
        });
        grid.attach(&b, (i % 2) as i32, (i / 2) as i32, 1, 1);
        images.push((wallpaper.clone(), image));
    }
    o.panel.append(&grid);

    let folder = gtk::Button::with_label("Open the wallpaper folder…");
    folder.add_css_class("link");
    folder.set_halign(gtk::Align::Start);
    let weak = Rc::downgrade(o);
    folder.connect_clicked(move |_| {
        let s = folder_tapet();
        let _ = std::fs::create_dir_all(&s);
        background("xdg-open", &[&s.to_string_lossy()]);
        if let Some(o) = weak.upgrade() {
            o.close();
        }
    });
    o.panel.append(&folder);

    // thumbnails in the background, so the popup opens at once
    let paths: Vec<PathBuf> = images.iter().map(|(t, _)| t.clone()).collect();
    in_background(
        move || paths.iter().map(thumbnail).collect::<Vec<_>>(),
        move |thumbnails| {
            for ((_, image), thumbnail) in images.iter().zip(thumbnails) {
                if let Some(n) = thumbnail {
                    image.set_filename(Some(&n));
                }
            }
        },
    );
}
