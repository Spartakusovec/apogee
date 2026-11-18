//! Stránka Users: vlastní účet – obrázek a jméno (AccountsService přes D-Bus), změna hesla
//! (dialog v okně, chpasswd přes pkexec – polkit se zeptá na současné heslo) a typ účtu.

use gtk::gio;
use gtk::prelude::*;

use super::ui::*;
use crate::spolecne::{na_pozadi, prikaz};

fn uzivatel() -> String {
    std::env::var("USER").unwrap_or_default()
}

/// Volání AccountsService pro vlastní účet (SetRealName, SetIconFile).
fn accounts(metoda: &'static str, hodnota: String, hotovo: &'static str) {
    let uid = prikaz("id", &["-u"]);
    na_pozadi(
        move || {
            std::process::Command::new("busctl")
                .args(["call", "org.freedesktop.Accounts", &format!("/org/freedesktop/Accounts/User{uid}"), "org.freedesktop.Accounts.User", metoda, "s", &hodnota])
                .output()
                .map(|v| v.status.success())
                .unwrap_or(false)
        },
        move |ok| {
            super::oznamit(if ok { hotovo } else { "Not changed" });
            super::obnovit("uzivatel");
        },
    );
}

fn vlastnost(nazev: &str) -> String {
    let uid = prikaz("id", &["-u"]);
    let v = prikaz("busctl", &["get-property", "org.freedesktop.Accounts", &format!("/org/freedesktop/Accounts/User{uid}"), "org.freedesktop.Accounts.User", nazev]);
    // „s "Jan Novák"“
    v.split_once(' ').map(|(_, x)| x.trim().trim_matches('"').to_string()).unwrap_or_default()
}

pub fn vytvorit() -> gtk::Widget {
    let s = stranka("Users");
    let jmeno = vlastnost("RealName");
    let ikona = vlastnost("IconFile");

    let karta = skupina(&s, "Your account", "");
    let obr = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    obr.add_css_class("radek");
    // obrázek zmenšený předem (Picture by jinak měl velikost souboru)
    let avatar = match gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(&ikona, 72, 72, false) {
        #[allow(deprecated)]
        Ok(p) if !ikona.is_empty() => gtk::Picture::for_paintable(&gtk::gdk::Texture::for_pixbuf(&p)),
        _ => gtk::Picture::new(),
    };
    avatar.set_size_request(72, 72);
    avatar.set_valign(gtk::Align::Center);
    avatar.add_css_class("avatar");
    avatar.set_overflow(gtk::Overflow::Hidden);
    obr.append(&avatar);
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 4);
    texty.set_valign(gtk::Align::Center);
    texty.set_hexpand(true);
    let zobrazit = if jmeno.is_empty() { uzivatel() } else { jmeno.clone() };
    let n = gtk::Label::new(Some(&zobrazit));
    n.add_css_class("nadpis-stranky");
    n.set_xalign(0.0);
    texty.append(&n);
    let admin = prikaz("id", &["-nG"]).split_whitespace().any(|g| g == "wheel");
    let p = gtk::Label::new(Some(&format!("{}  ·  {}", uzivatel(), if admin { "Administrator" } else { "Standard user" })));
    p.add_css_class("popis");
    p.set_xalign(0.0);
    texty.append(&p);
    obr.append(&texty);
    let zmenit = tlacitko("Change picture…", false, || {
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Choose a picture");
        let filtr = gtk::FileFilter::new();
        filtr.set_name(Some("Pictures"));
        filtr.add_mime_type("image/*");
        let filtry = gio::ListStore::new::<gtk::FileFilter>();
        filtry.append(&filtr);
        dialog.set_filters(Some(&filtry));
        dialog.open(super::okno().as_ref(), gio::Cancellable::NONE, |v| {
            let Some(p) = v.ok().and_then(|f| f.path()) else { return };
            // čtvercový výřez 256 px (AccountsService větší obrázky odmítá)
            let cil = crate::spolecne::domov().join(".face");
            let ok = gtk::gdk_pixbuf::Pixbuf::from_file(&p).ok().and_then(|o| {
                let strana = o.width().min(o.height());
                let vyrez = o.new_subpixbuf((o.width() - strana) / 2, (o.height() - strana) / 2, strana, strana);
                vyrez.scale_simple(256, 256, gtk::gdk_pixbuf::InterpType::Bilinear)?.savev(&cil, "png", &[]).ok()
            });
            if ok.is_some() {
                accounts("SetIconFile", cil.to_string_lossy().to_string(), "Picture changed");
            } else {
                super::oznamit("Couldn't open the picture");
            }
        });
    });
    zmenit.set_valign(gtk::Align::Center);
    obr.append(&zmenit);
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&obr));
    karta.append(&row);

    let pole = gtk::Entry::new();
    pole.set_text(&jmeno);
    pole.set_width_chars(22);
    pole.connect_activate(|e| accounts("SetRealName", e.text().trim().to_string(), "Name changed"));
    radek(&karta, "\u{f007}", "Full name", "Shown on the lock screen. Press Enter to save.", Some(pole.upcast_ref()));

    // heslo
    let karta = skupina(&s, "Password", "You'll be asked for your current password first.");
    let nove = gtk::PasswordEntry::new();
    nove.set_show_peek_icon(true);
    nove.set_width_chars(22);
    nove.set_property("placeholder-text", "New password");
    let znovu = gtk::PasswordEntry::new();
    znovu.set_show_peek_icon(true);
    znovu.set_width_chars(22);
    znovu.set_property("placeholder-text", "Type it again");
    radek(&karta, "\u{f084}", "New password", "", Some(nove.upcast_ref()));
    radek(&karta, "", "Confirm", "", Some(znovu.upcast_ref()));
    let zmenit = tlacitko("Change password", true, move || {
        let (a, b) = (nove.text().to_string(), znovu.text().to_string());
        if a.is_empty() || a != b {
            super::oznamit(if a.is_empty() { "Type a new password" } else { "The passwords don't match" });
            return;
        }
        if a.contains(':') || a.contains('\n') {
            super::oznamit("The password can't contain : or a new line");
            return;
        }
        super::oznamit("Waiting for your current password…");
        let radek = format!("{}:{a}\n", uzivatel());
        let (n2, z2) = (nove.clone(), znovu.clone());
        na_pozadi(
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
                            v.write_all(radek.as_bytes())?;
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
                super::oznamit(if ok { "Password changed – use it next time you log in or unlock" } else { "Password not changed" });
            },
        );
    });
    zmenit.set_halign(gtk::Align::End);
    s.append(&zmenit);
    s.upcast()
}
