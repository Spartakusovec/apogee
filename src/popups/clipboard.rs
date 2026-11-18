//! Clipboard history (Super+V): typing filters, arrows and Enter (or a click) pick.
//! Images get a thumbnail (saved in ~/.cache/apogee/clipboard). It replaced a fuzzel script.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};

use crate::common::*;

const THUMBNAIL: i32 = 64;
const MAX: usize = 300;

fn folder_thumbnail() -> PathBuf {
    home().join(".cache/apogee/clipboard")
}

/// Content of a record (a line from `cliphist list`).
fn decode(row: &str) -> Vec<u8> {
    let Ok(mut p) = Command::new("cliphist").arg("decode").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
        return Vec::new();
    };
    if let Some(mut input) = p.stdin.take() {
        let _ = input.write_all(format!("{row}\n").as_bytes());
    }
    p.wait_with_output().map(|o| o.stdout).unwrap_or_default()
}

/// Image thumbnail (creates it if missing; slow, call outside GTK).
fn create_thumbnail(row: &str, file: &PathBuf) -> bool {
    if file.exists() {
        return true;
    }
    let data = decode(row);
    let loader = gtk::gdk_pixbuf::PixbufLoader::new();
    if loader.write(&data).is_err() || loader.close().is_err() {
        return false;
    }
    let Some(image) = loader.pixbuf() else { return false };
    let (s, v) = (image.width() as f64, image.height() as f64);
    let mira = (160.0 / s.max(v)).min(1.0);
    let Some(small) = image.scale_simple((s * mira).max(1.0) as i32, (v * mira).max(1.0) as i32, gtk::gdk_pixbuf::InterpType::Bilinear) else {
        return false;
    };
    let _ = std::fs::create_dir_all(folder_thumbnail());
    small.savev(file, "png", &[]).is_ok()
}

/// „[[ binary data 1 MiB png 1920x1080 ]]“ → „Image · png 1920x1080 · 1 MiB“
fn description_images(content: &str) -> String {
    let inner = content.trim_start_matches("[[ binary data ").trim_end_matches(" ]]");
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() >= 3 {
        format!("Image  ·  {}  ·  {} {}", parts[2..].join(" "), parts[0], parts[1])
    } else {
        "Image".into()
    }
}

pub fn fill(o: &Rc<Popup>) {
    // keyboard-driven: typing filters, arrows, Enter; don't close by itself
    o.window.set_keyboard_mode(KeyboardMode::Exclusive);
    o.keep_open_itself();
    let records: Vec<String> = command("cliphist", &["list"]).lines().take(MAX).map(String::from).collect();
    if records.is_empty() {
        o.state("The clipboard history is empty");
        return;
    }

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search the clipboard…"));
    // Esc is kept by the search field (clears the search), so close the popup here
    let slaby = Rc::downgrade(o);
    search.connect_stop_search(move |_| {
        if let Some(o) = slaby.upgrade() {
            o.close();
        }
    });
    o.panel.append(&search);
    let list = gtk::ListBox::new();
    list.add_css_class("clipboard");
    list.set_selection_mode(gtk::SelectionMode::Browse);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(480);
    scroll.set_child(Some(&list));
    o.panel.append(&scroll);

    let mut rows: Vec<(gtk::ListBoxRow, String)> = Vec::new();
    let mut missing_thumbnail: Vec<(String, PathBuf, gtk::Image)> = Vec::new();
    for row in &records {
        let (id, content) = row.split_once('\t').unwrap_or(("", row));
        let content_rows = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let text = if content.starts_with("[[ binary data") {
            let file = folder_thumbnail().join(format!("{id}.png"));
            let thumbnail = gtk::Image::new();
            thumbnail.set_pixel_size(THUMBNAIL);
            if file.exists() {
                thumbnail.set_from_file(Some(&file));
            } else {
                missing_thumbnail.push((row.clone(), file, thumbnail.clone()));
            }
            content_rows.append(&thumbnail);
            description_images(content)
        } else {
            content.to_string()
        };
        let l = label(&text, &[]);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        l.set_hexpand(true);
        content_rows.append(&l);
        let r = gtk::ListBoxRow::new();
        r.set_child(Some(&content_rows));
        list.append(&r);
        rows.push((r, text.to_lowercase()));
    }
    let rows = Rc::new(rows);
    let records = Rc::new(records);

    // pick: copy to the clipboard and close
    let slaby = Rc::downgrade(o);
    let z = records.clone();
    list.connect_row_activated(move |_, r| {
        let Some(row) = z.get(r.index() as usize).cloned() else { return };
        if let Some(o) = slaby.upgrade() {
            o.close();
        }
        std::thread::spawn(move || {
            let data = decode(&row);
            if let Ok(mut p) = Command::new("wl-copy").stdin(Stdio::piped()).spawn() {
                if let Some(mut input) = p.stdin.take() {
                    let _ = input.write_all(&data);
                }
                let _ = p.wait();
            }
        });
    });

    // typing filters, the first visible row is selected
    let first_visible = {
        let rows = rows.clone();
        move || rows.iter().find(|(r, _)| r.is_visible()).map(|(r, _)| r.clone())
    };
    let (r2, s2, first) = (rows.clone(), list.downgrade(), first_visible.clone());
    search.connect_search_changed(move |h| {
        let searched = h.text().to_lowercase();
        for (r, text) in r2.iter() {
            r.set_visible(searched.is_empty() || text.contains(&searched));
        }
        if let (Some(s), Some(r)) = (s2.upgrade(), first()) {
            s.select_row(Some(&r));
        }
    });
    // Enter in the search picks the selected row, arrow down moves into the list
    let s2 = list.downgrade();
    search.connect_activate(move |_| {
        if let Some(s) = s2.upgrade() {
            if let Some(r) = s.selected_row() {
                r.activate();
            }
        }
    });
    let keys = gtk::EventControllerKey::new();
    let (s2, first) = (list.downgrade(), first_visible.clone());
    keys.connect_key_pressed(move |_, k, _, _| {
        if k == gtk::gdk::Key::Down {
            if let (Some(s), Some(r)) = (s2.upgrade(), first()) {
                s.select_row(Some(&r));
                r.grab_focus();
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    search.add_controller(keys);
    // select the first row only after the popup is shown (GTK doesn't show a selection before)
    let (s2, first) = (list.downgrade(), first_visible.clone());
    glib::idle_add_local_once(move || {
        if let (Some(s), Some(r)) = (s2.upgrade(), first()) {
            s.select_row(Some(&r));
        }
    });
    search.grab_focus();

    // missing thumbnails in the background, clean up old ones (deleted records)
    let id_record: Vec<String> = records.iter().filter_map(|r| r.split_once('\t').map(|(id, _)| format!("{id}.png"))).collect();
    let k_create: Vec<(String, PathBuf)> = missing_thumbnail.iter().map(|(r, s, _)| (r.clone(), s.clone())).collect();
    in_background(
        move || {
            if let Ok(files) = std::fs::read_dir(folder_thumbnail()) {
                for s in files.flatten() {
                    if !id_record.contains(&s.file_name().to_string_lossy().to_string()) {
                        let _ = std::fs::remove_file(s.path());
                    }
                }
            }
            k_create.iter().map(|(r, s)| create_thumbnail(r, s)).collect::<Vec<_>>()
        },
        move |done| {
            for ((_, file, image), ok) in missing_thumbnail.iter().zip(done) {
                if ok {
                    image.set_from_file(Some(file));
                }
            }
        },
    );
}
