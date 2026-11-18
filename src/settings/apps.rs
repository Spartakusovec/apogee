//! Apps page: default apps (browser, files, video, music, pictures, PDF, text, email)
//! and apps started after login (settings.json `autostart`, the daemon starts them once
//! per session).

use gtk::gio;
use gtk::prelude::*;
use serde_json::json;

use super::ui::*;
use crate::common::{settings, run_dir, save_settings};

/// (name, MIME types to set; the first decides the list of apps and the current choice)
const DEFAULT: &[(&str, &str, &[&str])] = &[
    ("Web browser", "\u{f0ac}", &["x-scheme-handler/https", "x-scheme-handler/http", "text/html", "application/xhtml+xml"]),
    ("Files", "\u{f07b}", &["inode/directory"]),
    ("Video", "\u{f03d}", &["video/mp4", "video/x-matroska", "video/webm", "video/quicktime", "video/x-msvideo", "video/mpeg"]),
    ("Music", "\u{f001}", &["audio/mpeg", "audio/flac", "audio/ogg", "audio/x-wav", "audio/mp4", "audio/x-vorbis+ogg"]),
    ("Pictures", "\u{f03e}", &["image/jpeg", "image/png", "image/webp", "image/gif", "image/heif", "image/bmp"]),
    ("PDF documents", "\u{f1c1}", &["application/pdf"]),
    ("Text files", "\u{f15c}", &["text/plain"]),
    ("Email", "\u{f0e0}", &["x-scheme-handler/mailto"]),
];

fn at_login() -> Vec<String> {
    settings("autostart").as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Daemon at start: launches the apps from `autostart`, only once per login (not after a daemon restart).
pub fn start_po_login() {
    let tag = run_dir().join("apogee-autostart-done");
    if tag.exists() {
        return;
    }
    let _ = std::fs::write(&tag, "");
    gtk::glib::timeout_add_seconds_local_once(3, || {
        for id in at_login() {
            if let Some(a) = find(&id) {
                let _ = a.launch(&[], gio::AppLaunchContext::NONE);
            }
        }
    });
}

fn find(id: &str) -> Option<gio::AppInfo> {
    gio::AppInfo::all().into_iter().find(|a| a.id().is_some_and(|i| i == id))
}

/// An app picker with icon and name (in the button and in the opened list).
fn choice_apps(apps: &[gio::AppInfo]) -> gtk::DropDown {
    let list = gio::ListStore::new::<gio::AppInfo>();
    for a in apps {
        list.append(a);
    }
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let i = gtk::Image::new();
        i.set_pixel_size(20);
        r.append(&i);
        r.append(&gtk::Label::new(None));
        item.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&r));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let (Some(r), Some(a)) = (item.child(), item.item().and_downcast::<gio::AppInfo>()) else { return };
        let i = r.first_child().and_downcast::<gtk::Image>().unwrap();
        match a.icon() {
            Some(icon) => i.set_from_gicon(&icon),
            None => i.set_icon_name(Some("application-x-executable")),
        }
        r.last_child().and_downcast::<gtk::Label>().unwrap().set_text(&name(&a));
    });
    let d = gtk::DropDown::new(Some(list), None::<gtk::Expression>);
    d.set_factory(Some(&factory));
    d
}

fn name(a: &gio::AppInfo) -> String {
    a.display_name().to_string()
}

pub fn create() -> gtk::Widget {
    let s = page("Apps");

    let card = group(&s, "Default apps", "What opens when you open a link or a file.");
    for (name_type, icon, typy) in DEFAULT {
        let mut options: Vec<gio::AppInfo> = gio::AppInfo::all_for_type(typy[0]).into_iter().filter(|a| a.should_show()).collect();
        options.sort_by_key(|a| name(a).to_lowercase());
        options.dedup_by_key(|a| a.id());
        let now = gio::AppInfo::default_for_type(typy[0], false);
        if options.is_empty() {
            add_row(&card, icon, name_type, "No app installed for this", None);
            continue;
        }
        let d = choice_apps(&options);
        let position = now.as_ref().and_then(|t| options.iter().position(|a| a.id() == t.id()));
        d.set_selected(position.map(|p| p as u32).unwrap_or(gtk::INVALID_LIST_POSITION));
        let typy: &'static [&'static str] = typy;
        let n: &'static str = name_type;
        d.connect_selected_notify(move |d| {
            let Some(a) = options.get(d.selected() as usize) else { return };
            let ok = typy.iter().all(|t| a.set_as_default_for_type(t).is_ok());
            super::notify(&if ok { format!("{n}: {}", name(a)) } else { "Couldn't change it".to_string() });
        });
        add_row(&card, icon, name_type, "", Some(d.upcast_ref()));
    }

    let card = group(&s, "Start after login", "These apps open by themselves every time you log in.");
    let list = at_login();
    if list.is_empty() {
        add_row(&card, "", "No apps yet", "", None);
    }
    for (i, id) in list.iter().enumerate() {
        let info = find(id);
        let name = info.as_ref().map(|a| a.display_name().to_string()).unwrap_or_else(|| id.clone());
        let s2 = list.clone();
        let remove = button("Remove", false, move || {
            let mut v = s2.clone();
            v.remove(i);
            save_settings("autostart", json!(v));
            super::refresh("apps");
        });
        let r = add_row(&card, "", &name, "", Some(remove.upcast_ref()));
        if let Some(i) = info.and_then(|a| a.icon()) {
            let img = gtk::Image::from_gicon(&i);
            img.set_pixel_size(28);
            if let Some(b) = r.child().and_downcast::<gtk::Box>() {
                b.prepend(&img);
            }
        }
    }
    let mut all: Vec<gio::AppInfo> = gio::AppInfo::all().into_iter().filter(|a| a.should_show() && a.id().is_some_and(|i| !list.iter().any(|x| *x == i))).collect();
    all.sort_by_key(|a| name(a).to_lowercase());
    let mut names = vec!["Choose…".to_string()];
    names.extend(all.iter().map(name));
    let d = gtk::DropDown::from_strings(&names.iter().map(String::as_str).collect::<Vec<_>>());
    d.set_enable_search(true);
    d.set_expression(Some(gtk::PropertyExpression::new(gtk::StringObject::static_type(), None::<gtk::Expression>, "string")));
    d.connect_selected_notify(move |d| {
        let i = d.selected() as usize;
        if i == 0 {
            return;
        }
        let Some(id) = all.get(i - 1).and_then(|a| a.id()) else { return };
        let mut v = at_login();
        v.push(id.to_string());
        save_settings("autostart", json!(v));
        super::notify("Added");
        super::refresh("apps");
    });
    add_row(&card, "\u{f067}", "Add an app", "", Some(d.upcast_ref()));
    s.upcast()
}
