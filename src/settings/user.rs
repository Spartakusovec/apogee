//! Users page: your own account – picture and name (AccountsService over D-Bus), password change
//! (dialog in the window, chpasswd through pkexec – polkit asks for the current password) and account type.

use gtk::gio;
use gtk::prelude::*;

use super::ui::*;
use crate::common::{in_background, command};

fn user() -> String {
    std::env::var("USER").unwrap_or_default()
}

/// AccountsService calls for your own account (SetRealName, SetIconFile).
fn accounts(method: &'static str, value: String, done: &'static str) {
    let uid = command("id", &["-u"]);
    in_background(
        move || {
            std::process::Command::new("busctl")
                .args(["call", "org.freedesktop.Accounts", &format!("/org/freedesktop/Accounts/User{uid}"), "org.freedesktop.Accounts.User", method, "s", &value])
                .output()
                .map(|v| v.status.success())
                .unwrap_or(false)
        },
        move |ok| {
            super::notify(if ok { done } else { "Not changed" });
            super::refresh("user");
        },
    );
}

fn property(name: &str) -> String {
    let uid = command("id", &["-u"]);
    let v = command("busctl", &["get-property", "org.freedesktop.Accounts", &format!("/org/freedesktop/Accounts/User{uid}"), "org.freedesktop.Accounts.User", name]);
    // “s "Jane Doe"”
    v.split_once(' ').map(|(_, x)| x.trim().trim_matches('"').to_string()).unwrap_or_default()
}

pub fn create() -> gtk::Widget {
    let s = page("Users");
    let name = property("RealName");
    let icon = property("IconFile");

    let card = group(&s, "Your account", "");
    let img = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    img.add_css_class("row");
    // the picture scaled in advance (otherwise Picture would take the file's size)
    let avatar = match gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&icon, 72, 72, false) {
        #[allow(deprecated)]
        Ok(p) if !icon.is_empty() => gtk::Picture::for_paintable(&gtk::gdk::Texture::for_pixbuf(&p)),
        _ => gtk::Picture::new(),
    };
    avatar.set_size_request(72, 72);
    avatar.set_valign(gtk::Align::Center);
    avatar.add_css_class("avatar");
    avatar.set_overflow(gtk::Overflow::Hidden);
    img.append(&avatar);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 4);
    texts.set_valign(gtk::Align::Center);
    texts.set_hexpand(true);
    let show = if name.is_empty() { user() } else { name.clone() };
    let n = gtk::Label::new(Some(&show));
    n.add_css_class("title-pages");
    n.set_xalign(0.0);
    texts.append(&n);
    let admin = command("id", &["-nG"]).split_whitespace().any(|g| g == "wheel");
    let p = gtk::Label::new(Some(&format!("{}  ·  {}", user(), if admin { "Administrator" } else { "Standard user" })));
    p.add_css_class("description");
    p.set_xalign(0.0);
    texts.append(&p);
    img.append(&texts);
    let change = button("Change picture…", false, || {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Choose a picture");
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Pictures"));
        filter.add_mime_type("image/*");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        dialog.open(super::window().as_ref(), gio::Cancellable::NONE, |v| {
            let Some(p) = v.ok().and_then(|f| f.path()) else { return };
            // a 256 px square crop (AccountsService rejects bigger images)
            let target = crate::common::home().join(".face");
            let ok = gtk::gdk_pixbuf::Pixbuf::from_file(&p).ok().and_then(|o| {
                let side = o.width().min(o.height());
                let crop = o.new_subpixbuf((o.width() - side) / 2, (o.height() - side) / 2, side, side);
                crop.scale_simple(256, 256, gtk::gdk_pixbuf::InterpType::Bilinear)?.savev(&target, "png", &[]).ok()
            });
            if ok.is_some() {
                accounts("SetIconFile", target.to_string_lossy().to_string(), "Picture changed");
            } else {
                super::notify("Couldn't open the picture");
            }
        });
    });
    change.set_valign(gtk::Align::Center);
    img.append(&change);
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&img));
    card.append(&row);

    let field = gtk::Entry::new();
    field.set_text(&name);
    field.set_width_chars(22);
    field.connect_activate(|e| accounts("SetRealName", e.text().trim().to_string(), "Name changed"));
    add_row(&card, "\u{f007}", "Full name", "Shown on the lock screen. Press Enter to save.", Some(field.upcast_ref()));

    // heslo
    let card = group(&s, "Password", "You'll be asked for your current password first.");
    let new = gtk::PasswordEntry::new();
    new.set_show_peek_icon(true);
    new.set_width_chars(22);
    new.set_property("placeholder-text", "New password");
    let again = gtk::PasswordEntry::new();
    again.set_show_peek_icon(true);
    again.set_width_chars(22);
    again.set_property("placeholder-text", "Type it again");
    add_row(&card, "\u{f084}", "New password", "", Some(new.upcast_ref()));
    add_row(&card, "", "Confirm", "", Some(again.upcast_ref()));
    let change = button("Change password", true, move || {
        let (a, b) = (new.text().to_string(), again.text().to_string());
        if a.is_empty() || a != b {
            super::notify(if a.is_empty() { "Type a new password" } else { "The passwords don't match" });
            return;
        }
        if a.contains(':') || a.contains('\n') {
            super::notify("The password can't contain : or a new line");
            return;
        }
        super::notify("Waiting for your current password…");
        let row = format!("{}:{a}\n", user());
        let (n2, z2) = (new.clone(), again.clone());
        in_background(
            move || {
                use std::io::Write;
                std::process::Command::new("pkexec")
                    .arg("/usr/sbin/chpasswd")
                    .stdin(std::process::Stdio::piped())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .and_then(|mut p| {
                        if let Some(mut v) = p.stdin.take() {
                            v.write_all(row.as_bytes())?;
                        }
                        p.wait()
                    })
                    .map(|s| s.success())
                    .unwrap_or(false)
            },
            move |ok| {
                if ok {
                    n2.set_text("");
                    z2.set_text("");
                }
                super::notify(if ok { "Password changed – use it next time you log in or unlock" } else { "Password not changed" });
            },
        );
    });
    change.set_halign(gtk::Align::End);
    s.append(&change);
    s.upcast()
}
