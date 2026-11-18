//! Stavební prvky stránek nastavení: stránka s nadpisem, skupina (nadpis nad zaoblenou kartou),
//! řádek „název a popis vlevo, ovládání vpravo“, klikací řádek, řádek s přepínačem.

use gtk::prelude::*;

/// Stránka: nadpis a pod ním skupiny; obsah v sloupci rozumné šířky uprostřed.
pub fn stranka(nadpis: &str) -> gtk::Box {
    let s = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.add_css_class("stranka");
    s.set_halign(gtk::Align::Center);
    s.set_size_request(600, -1);
    let n = gtk::Label::new(Some(nadpis));
    n.add_css_class("nadpis-stranky");
    n.set_xalign(0.0);
    s.append(&n);
    s
}

/// Skupina s nadpisem (a volitelným popisem pod ním); řádky se přidávají do vrácené karty.
pub fn skupina(stranka: &gtk::Box, nadpis: &str, popis: &str) -> gtk::ListBox {
    let obal = gtk::Box::new(gtk::Orientation::Vertical, 6);
    if !nadpis.is_empty() {
        let n = gtk::Label::new(Some(nadpis));
        n.add_css_class("nadpis-skupiny");
        n.set_xalign(0.0);
        obal.append(&n);
    }
    if !popis.is_empty() {
        let p = gtk::Label::new(Some(popis));
        p.add_css_class("popis");
        p.set_xalign(0.0);
        p.set_wrap(true);
        p.set_max_width_chars(80);
        obal.append(&p);
    }
    let karta = gtk::ListBox::new();
    karta.add_css_class("karta");
    karta.set_selection_mode(gtk::SelectionMode::None);
    // zaoblené rohy i pro zvýraznění prvního a posledního řádku
    karta.set_overflow(gtk::Overflow::Hidden);
    obal.append(&karta);
    stranka.append(&obal);
    karta
}

/// Obsah řádku: ikona (Font Awesome, nepovinná), název a popis, vpravo widget.
fn obsah_radku(ikona: &str, nazev: &str, popis: &str, vpravo: Option<&gtk::Widget>) -> gtk::Box {
    let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    r.add_css_class("radek");
    if !ikona.is_empty() {
        let i = gtk::Label::new(Some(ikona));
        i.add_css_class("ikona");
        i.set_width_chars(2);
        r.append(&i);
    }
    let texty = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texty.set_hexpand(true);
    texty.set_valign(gtk::Align::Center);
    let n = gtk::Label::new(Some(nazev));
    n.set_xalign(0.0);
    n.set_ellipsize(gtk::pango::EllipsizeMode::End);
    texty.append(&n);
    if !popis.is_empty() {
        let p = gtk::Label::new(Some(popis));
        p.add_css_class("popis");
        p.set_xalign(0.0);
        p.set_wrap(true);
        p.set_max_width_chars(60);
        texty.append(&p);
    }
    r.append(&texty);
    if let Some(w) = vpravo {
        w.set_valign(gtk::Align::Center);
        r.append(w);
    }
    r
}

/// Řádek s ovládáním vpravo (přepínač, posuvník, tlačítko, text).
pub fn radek(karta: &gtk::ListBox, ikona: &str, nazev: &str, popis: &str, vpravo: Option<&gtk::Widget>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&obsah_radku(ikona, nazev, popis, vpravo)));
    karta.append(&row);
    row
}

/// Klikací řádek (síť, zařízení, výstup): celý řádek je tlačítko.
pub fn radek_akce(karta: &gtk::ListBox, ikona: &str, nazev: &str, popis: &str, vpravo: &str, akce: impl Fn() + 'static) -> gtk::ListBoxRow {
    let b = gtk::Button::new();
    b.add_css_class("radek-akce");
    let v = gtk::Label::new(Some(vpravo));
    v.add_css_class("popis");
    b.set_child(Some(&obsah_radku(ikona, nazev, popis, Some(v.upcast_ref()))));
    b.connect_clicked(move |_| akce());
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&b));
    karta.append(&row);
    row
}

/// Řádek s přepínačem; `pri_zmene` dostane nový stav (jen při změně uživatelem).
pub fn radek_prepinac(karta: &gtk::ListBox, nazev: &str, popis: &str, zapnuto: bool, pri_zmene: impl Fn(bool) + 'static) -> gtk::Switch {
    let s = gtk::Switch::new();
    s.set_active(zapnuto);
    s.connect_state_set(move |_, stav| {
        pri_zmene(stav);
        gtk::glib::Propagation::Proceed
    });
    radek(karta, "", nazev, popis, Some(s.upcast_ref()));
    s
}

/// Řádek s textem „načítá se“ (než přijdou data z pozadí).
pub fn nacitani(karta: &gtk::ListBox, text: &str) {
    radek(karta, "", text, "", None);
}

pub fn vycistit(karta: &gtk::ListBox) {
    while let Some(r) = karta.first_child() {
        karta.remove(&r);
    }
}

/// Malé tlačítko v řádku (Disconnect, Forget…); `hlavni` = barvou akcentu.
pub fn tlacitko(text: &str, hlavni: bool, akce: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class(if hlavni { "hlavni" } else { "vedlejsi" });
    b.connect_clicked(move |_| akce());
    b
}
