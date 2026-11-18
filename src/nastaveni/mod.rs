//! Aplikace Settings (`apogee nastaveni [stránka]`): jedno okno na nastavení, která se jinak
//! dělají v terminálu. Návrh: NAVRH-NASTAVENI.md.
//!
//! Normální okno (přesunout, maximalizovat, minimalizovat), vlevo hledání a stránky, vpravo
//! obsah. Stránka se sestaví pokaždé, když se na ni přepne (ukáže čerstvý stav); data se
//! načítají na pozadí. Změny platí hned, potvrzení se ukáže krátce dole v okně. Okno se při
//! zavření zruší celé (démon na pozadí nedrží nic navíc).

mod aktualizace;
mod aplikace;
mod bluetooth;
mod cas;
mod displej;
mod sway_conf;
mod napajeni;
mod sit;
mod o_pocitaci;
mod okna;
mod oznameni;
mod panel_dok;
mod ui;
mod uloziste;
mod uzivatel;
mod vstup;
mod vzhled;
mod wifi;
mod wifi_sit;
mod zkratky;
mod zvuk;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

/// stránka: id (pro `apogee nastaveni <id>`), název, ikona (Font Awesome), slova pro hledání
const STRANKY: &[(&str, &str, &str, &str)] = &[
    ("wifi", "Wi-Fi", "\u{f1eb}", "wifi wi-fi wireless network internet password hidden airplane sit heslo"),
    ("sit", "Network", "\u{f6ff}", "network wired ethernet cable vpn wireguard openvpn hotspot share internet airplane flight sit kabel letadlo"),
    ("bluetooth", "Bluetooth", "\u{f293}", "bluetooth headphones headset speaker mouse keyboard pair devices sluchatka"),
    ("zvuk", "Sound", "\u{f028}", "sound audio volume speakers headphones microphone output input apps mute zvuk hlasitost mikrofon"),
    ("displej", "Display", "\u{f108}", "display screen brightness night light warm scale size resolution jas displej obrazovka nocni"),
    ("vstup", "Keyboard & Touchpad", "\u{f11c}", "keyboard touchpad mouse layout czech english language typing repeat numlock tap click drag scroll natural speed pointer gestures klavesnice mys"),
    ("napajeni", "Power & Battery", "\u{f240}", "power battery saver balanced performance health conservation charge charging screen lock dim idle sleep blank baterie napajeni zamknout"),
    ("vzhled", "Appearance", "\u{f53f}", "appearance wallpaper background colors theme text size font buttons tapeta pozadi barvy vzhled pismo"),
    ("panel_dok", "Panel & Dock", "\u{f2d0}", "panel bar top clock date seconds 12-hour modules weather dock pinned apps hide autohide lista hodiny"),
    ("okna", "Windows & Workspaces", "\u{f2d2}", "windows workspaces tiling floating gaps focus snap maximize okna plochy dlazdice"),
    ("oznameni", "Notifications", "\u{f0f3}", "notifications do not disturb dnd quiet night mute apps oznameni nerusit"),
    ("zkratky", "Shortcuts", "\u{f0e7}", "shortcuts keys keyboard hotkeys gestures zkratky klavesy"),
    ("aplikace", "Apps", "\u{f00a}", "apps default browser video music pictures pdf email startup autostart login aplikace vychozi"),
    ("cas", "Date & Time", "\u{f017}", "date time timezone zone clock ntp automatic region formats language datum cas"),
    ("uzivatel", "Users", "\u{f007}", "users account user name picture photo avatar password login uzivatel heslo ucet"),
    ("uloziste", "Storage", "\u{f0a0}", "storage disk space free clean trash cache flatpak downloads uloziste misto"),
    ("aktualizace", "Updates", "\u{f019}", "updates upgrade dnf flatpak firmware bios fwupd sway rebuild aktualizace"),
    ("o_pocitaci", "About", "\u{f05a}", "about computer system hardware processor memory ram disk storage graphics nvidia version backup restore fedora sway"),
];

/// nadpisy skupin v postranním panelu: (první stránka skupiny, nadpis)
const SKUPINY: &[(&str, &str)] = &[("wifi", "Connections"), ("zvuk", "Device"), ("vzhled", "Personalization"), ("aplikace", "System")];

pub struct Nastaveni {
    pub okno: gtk::Window,
    obsah: gtk::ScrolledWindow,
    seznam: gtk::ListBox,
    hledat: gtk::SearchEntry,
    potvrzeni: gtk::Revealer,
    potvrzeni_text: gtk::Label,
    potvrzeni_casovac: RefCell<Option<glib::SourceId>>,
    aktivni: RefCell<String>,
}

thread_local! {
    static NASTAVENI: RefCell<Option<Rc<Nastaveni>>> = const { RefCell::new(None) };
}

/// Otevře okno (nebo ho přenese dopředu) na dané stránce (prázdná = naposledy otevřená).
pub fn otevrit(stranka: &str) {
    let n = NASTAVENI.with(|n| n.borrow().clone()).unwrap_or_else(vytvorit);
    if stranka == "eduroam" {
        n.ukazat("wifi");
        wifi_sit::podnikova("");
        n.okno.present();
        return;
    }
    if let Some(sit) = stranka.strip_prefix("wifi:") {
        n.ukazat("wifi");
        wifi_sit::podrobnosti(sit);
        n.okno.present();
        return;
    }
    let id = STRANKY.iter().find(|s| s.0 == stranka).map(|s| s.0.to_string()).unwrap_or_else(|| {
        let a = n.aktivni.borrow().clone();
        if a.is_empty() { STRANKY[0].0.to_string() } else { a }
    });
    n.ukazat(&id);
    n.okno.present();
}

fn vytvorit() -> Rc<Nastaveni> {
    crate::styl::pouzit();
    // podle toho Sway i dok poznají aplikaci (apogee-settings.desktop: Settings, ikona)
    glib::set_prgname(Some("apogee-settings"));
    let okno = gtk::Window::new();
    okno.set_title(Some("Settings"));
    okno.set_default_size(920, 640);
    okno.add_css_class("nastaveni");

    // vlevo hledání a stránky
    let leva = gtk::Box::new(gtk::Orientation::Vertical, 10);
    leva.add_css_class("postranni");
    leva.set_size_request(230, -1);
    let hledat = gtk::SearchEntry::new();
    hledat.set_placeholder_text(Some("Search settings"));
    hledat.set_search_delay(0);
    leva.append(&hledat);
    let seznam = gtk::ListBox::new();
    seznam.add_css_class("stranky");
    seznam.set_selection_mode(gtk::SelectionMode::Single);
    for (id, nazev, ikona, _) in STRANKY {
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let i = gtk::Label::new(Some(ikona));
        i.add_css_class("ikona");
        i.set_width_chars(2);
        r.append(&i);
        r.append(&gtk::Label::new(Some(nazev)));
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&r));
        row.set_widget_name(id);
        seznam.append(&row);
    }
    // nadpisy skupin (Connections, Device…), při hledání schované
    seznam.set_header_func(|row, _| {
        let id = row.widget_name();
        let nadpis = SKUPINY.iter().find(|s| s.0 == id).map(|s| s.1);
        match nadpis {
            Some(n) if NASTAVENI.with(|x| x.borrow().as_ref().is_none_or(|x| x.hledat.text().is_empty())) => {
                let l = gtk::Label::new(Some(n));
                l.add_css_class("skupina-stranek");
                l.set_xalign(0.0);
                row.set_header(Some(&l));
            }
            _ => row.set_header(None::<&gtk::Widget>),
        }
    });
    let posun = gtk::ScrolledWindow::new();
    posun.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    posun.set_vexpand(true);
    posun.set_child(Some(&seznam));
    leva.append(&posun);

    // vpravo obsah stránky
    let obsah = gtk::ScrolledWindow::new();
    obsah.set_hexpand(true);
    obsah.set_vexpand(true);
    obsah.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    obsah.add_css_class("obsah");

    let hlavni = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    hlavni.append(&leva);
    hlavni.append(&obsah);

    // krátké potvrzení dole uprostřed („Connected to …“)
    let potvrzeni_text = gtk::Label::new(None);
    potvrzeni_text.add_css_class("potvrzeni");
    let potvrzeni = gtk::Revealer::new();
    potvrzeni.set_transition_type(gtk::RevealerTransitionType::SlideUp);
    potvrzeni.set_child(Some(&potvrzeni_text));
    potvrzeni.set_halign(gtk::Align::Center);
    potvrzeni.set_valign(gtk::Align::End);
    potvrzeni.set_margin_bottom(18);
    potvrzeni.set_can_target(false);
    let prekryv = gtk::Overlay::new();
    prekryv.set_child(Some(&hlavni));
    prekryv.add_overlay(&potvrzeni);
    okno.set_child(Some(&prekryv));

    let n = Rc::new(Nastaveni {
        okno: okno.clone(),
        obsah,
        seznam: seznam.clone(),
        hledat: hledat.clone(),
        potvrzeni,
        potvrzeni_text,
        potvrzeni_casovac: RefCell::new(None),
        aktivni: RefCell::new(String::new()),
    });

    seznam.connect_row_selected(|_, row| {
        let Some(row) = row else { return };
        let id = row.widget_name().to_string();
        if let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) {
            if *n.aktivni.borrow() != id {
                n.ukazat(&id);
            }
        }
    });
    hledat.connect_search_changed(|h| {
        if let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) {
            n.filtrovat(&h.text());
        }
    });
    // Enter: první nalezená stránka
    hledat.connect_activate(|_| {
        if let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) {
            let mut r = n.seznam.first_child();
            while let Some(w) = r {
                if w.is_visible() {
                    if let Some(row) = w.downcast_ref::<gtk::ListBoxRow>() {
                        n.seznam.select_row(Some(row));
                    }
                    break;
                }
                r = w.next_sibling();
            }
        }
    });
    // Ctrl+F hledání, Esc vyčistí hledání
    let klavesy = gtk::EventControllerKey::new();
    klavesy.connect_key_pressed(|_, k, _, stav| {
        let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) else { return glib::Propagation::Proceed };
        if k == gtk::gdk::Key::f && stav.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
            n.hledat.grab_focus();
            return glib::Propagation::Stop;
        }
        if k == gtk::gdk::Key::Escape && !n.hledat.text().is_empty() {
            n.hledat.set_text("");
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    okno.add_controller(klavesy);
    okno.connect_close_request(|_| {
        // zrušit celé okno, ať démon nedrží stránky v paměti
        if let Some(n) = NASTAVENI.with(|n| n.borrow_mut().take()) {
            if let Some(id) = n.potvrzeni_casovac.take() {
                id.remove();
            }
        }
        glib::idle_add_local_once(crate::spolecne::vratit_pamet);
        glib::Propagation::Proceed
    });

    NASTAVENI.with(|x| x.replace(Some(n.clone())));
    n
}

/// Okno je otevřené (stránky to kontrolují, než ukážou data načtená na pozadí).
pub fn otevreno() -> bool {
    NASTAVENI.with(|n| n.borrow().is_some())
}

/// Krátké potvrzení dole v okně.
pub fn oznamit(text: &str) {
    let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) else { return };
    n.potvrzeni_text.set_text(text);
    n.potvrzeni.set_reveal_child(true);
    if let Some(id) = n.potvrzeni_casovac.take() {
        id.remove();
    }
    let id = glib::timeout_add_local_once(Duration::from_millis(3200), || {
        if let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) {
            n.potvrzeni_casovac.replace(None);
            n.potvrzeni.set_reveal_child(false);
        }
    });
    n.potvrzeni_casovac.replace(Some(id));
}

/// Znovu sestaví aktivní stránku (po změně, která se projeví až za chvíli).
pub fn obnovit(stranka: &str) {
    if let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) {
        if *n.aktivni.borrow() == stranka {
            n.ukazat(stranka);
        }
    }
}

impl Nastaveni {
    fn ukazat(&self, id: &str) {
        self.aktivni.replace(id.to_string());
        // výběr v seznamu (bez opětovného sestavení přes connect_row_selected)
        let mut r = self.seznam.first_child();
        while let Some(w) = r {
            if w.widget_name() == id {
                if let Some(row) = w.downcast_ref::<gtk::ListBoxRow>() {
                    if self.seznam.selected_row().as_ref() != Some(row) {
                        self.seznam.select_row(Some(row));
                    }
                }
            }
            r = w.next_sibling();
        }
        let stranka = match id {
            "bluetooth" => bluetooth::vytvorit(),
            "zvuk" => zvuk::vytvorit(),
            "displej" => displej::vytvorit(),
            "napajeni" => napajeni::vytvorit(),
            "vzhled" => vzhled::vytvorit(),
            "vstup" => vstup::vytvorit(),
            "panel_dok" => panel_dok::vytvorit(),
            "okna" => okna::vytvorit(),
            "zkratky" => zkratky::vytvorit(),
            "oznameni" => oznameni::vytvorit(),
            "aplikace" => aplikace::vytvorit(),
            "cas" => cas::vytvorit(),
            "sit" => sit::vytvorit(),
            "uzivatel" => uzivatel::vytvorit(),
            "uloziste" => uloziste::vytvorit(),
            "aktualizace" => aktualizace::vytvorit(),
            "o_pocitaci" => o_pocitaci::vytvorit(),
            _ => wifi::vytvorit(),
        };
        self.obsah.set_child(Some(&stranka));
        self.obsah.vadjustment().set_value(0.0);
    }

    fn filtrovat(&self, text: &str) {
        let slova: Vec<String> = text.split_whitespace().map(|s| crate::spotlight::bez_diakritiky(&s.to_lowercase())).collect();
        let mut r = self.seznam.first_child();
        while let Some(w) = r {
            let id = w.widget_name();
            let ano = STRANKY.iter().find(|s| s.0 == id).is_some_and(|(_, nazev, _, klic)| {
                let v = format!("{} {}", nazev.to_lowercase(), klic);
                slova.iter().all(|s| v.split_whitespace().any(|x| x.starts_with(s.as_str())))
            });
            w.set_visible(ano);
            r = w.next_sibling();
        }
        self.seznam.invalidate_headers();
    }
}

/// Podstránka (třeba podrobnosti sítě) se šipkou zpět na aktivní stránku.
pub fn podstranka(_zpet: &str, obsah: gtk::Widget) {
    let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) else { return };
    // šipka zpět nese název stránky, ze které podstránka vede (Wi-Fi, Network…)
    let aktivni = n.aktivni.borrow().clone();
    let zpet = STRANKY.iter().find(|s| s.0 == aktivni).map(|s| s.1).unwrap_or("Back");
    let obal = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let b = gtk::Button::with_label(&format!("\u{2039}  {zpet}"));
    b.add_css_class("zpet");
    b.set_halign(gtk::Align::Start);
    b.connect_clicked(|_| {
        if let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) {
            let id = n.aktivni.borrow().clone();
            n.ukazat(&id);
        }
    });
    obal.append(&b);
    obal.append(&obsah);
    n.obsah.set_child(Some(&obal));
    n.obsah.vadjustment().set_value(0.0);
}

/// Okno nastavení (rodič pro dialogy).
pub fn okno() -> Option<gtk::Window> {
    NASTAVENI.with(|n| n.borrow().as_ref().map(|n| n.okno.clone()))
}

/// Aplikace po přihlášení (Settings → Apps), volá démon při startu.
pub fn spustit_po_prihlaseni() {
    aplikace::spustit_po_prihlaseni();
}

/// Znovu sestaví stránku, která je právě otevřená (z podstránky zpět na ni).
pub fn obnovit_aktivni() {
    if let Some(n) = NASTAVENI.with(|n| n.borrow().clone()) {
        let id = n.aktivni.borrow().clone();
        n.ukazat(&id);
    }
}
