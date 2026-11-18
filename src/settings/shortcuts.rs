//! Shortcuts page: all keyboard shortcuts and gestures from ~/.config/sway/shortcuts.txt
//! (the same list as Super+F1), in groups and with its own search.

use gtk::prelude::*;

use super::ui::*;
use crate::common::home;

/// (group, [(keys, what it does)]); lines without two spaces are notes (keys empty)
fn load() -> Vec<(String, Vec<(String, String)>)> {
    let t = std::fs::read_to_string(home().join(".config/sway/shortcuts.txt")).unwrap_or_default();
    let mut v: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for r in t.lines() {
        let r = r.trim_end();
        if r.contains('─') {
            v.push((r.trim_matches(|c: char| c == '─' || c == ' ').to_string(), Vec::new()));
            continue;
        }
        if r.is_empty() || v.is_empty() {
            continue;
        }
        let (k, co) = match r.find("  ") {
            Some(i) => (r[..i].trim().to_string(), r[i..].trim().to_string()),
            None => (String::new(), r.to_string()),
        };
        // capital first letter (“close window” → “Close window”)
        let mut z = co.chars();
        let co = z.next().map(|p| p.to_uppercase().chain(z).collect()).unwrap_or_default();
        v.last_mut().unwrap().1.push((k, co));
    }
    v
}

pub fn create() -> gtk::Widget {
    let s = page("Shortcuts");
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Find a shortcut, e.g. screenshot"));
    s.append(&search);
    let mut rows: Vec<(gtk::ListBoxRow, String, gtk::Widget)> = Vec::new();
    for (group_name, items) in load() {
        let card = group(&s, &group_name, "");
        let wrapper = card.parent().unwrap();
        for (k, co) in items {
            let l = gtk::Label::new(Some(&k));
            l.add_css_class("keys");
            let row = add_row(&card, "", &co, "", if k.is_empty() { None } else { Some(l.upcast_ref()) });
            let search = crate::spotlight::without_diacritics(&format!("{k} {co} {group_name}").to_lowercase());
            rows.push((row, search, wrapper.clone()));
        }
    }
    search.connect_search_changed(move |h| {
        let words: Vec<String> = h.text().split_whitespace().map(|x| crate::spotlight::without_diacritics(&x.to_lowercase())).collect();
        let mut visible_groups = std::collections::HashSet::new();
        for (row, text, wrapper) in &rows {
            let yes = words.iter().all(|w| text.contains(w.as_str()));
            row.set_visible(yes);
            if yes {
                visible_groups.insert(wrapper.clone());
            }
        }
        for (_, _, wrapper) in &rows {
            wrapper.set_visible(visible_groups.contains(wrapper));
        }
    });
    s.upcast()
}
