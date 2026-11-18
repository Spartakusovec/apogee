//! Building blocks of the settings pages: page with a title, group (heading above a rounded card),
//! row “name and description on the left, control on the right”, clickable row, row with a switch.

use gtk::prelude::*;

/// Page: a title and the groups under it; content in a column of reasonable width in the middle.
pub fn page(title: &str) -> gtk::Box {
    let s = gtk::Box::new(gtk::Orientation::Vertical, 22);
    s.add_css_class("page");
    s.set_halign(gtk::Align::Center);
    s.set_size_request(600, -1);
    let n = gtk::Label::new(Some(title));
    n.add_css_class("title-pages");
    n.set_xalign(0.0);
    s.append(&n);
    s
}

/// A group with a heading (and an optional description under it); rows are added to the returned card.
pub fn group(page: &gtk::Box, title: &str, description: &str) -> gtk::ListBox {
    let wrapper = gtk::Box::new(gtk::Orientation::Vertical, 6);
    if !title.is_empty() {
        let n = gtk::Label::new(Some(title));
        n.add_css_class("title-groups");
        n.set_xalign(0.0);
        wrapper.append(&n);
    }
    if !description.is_empty() {
        let p = gtk::Label::new(Some(description));
        p.add_css_class("description");
        p.set_xalign(0.0);
        p.set_wrap(true);
        p.set_max_width_chars(80);
        wrapper.append(&p);
    }
    let card = gtk::ListBox::new();
    card.add_css_class("card");
    card.set_selection_mode(gtk::SelectionMode::None);
    // rounded corners for the highlight of the first and last row too
    card.set_overflow(gtk::Overflow::Hidden);
    wrapper.append(&card);
    page.append(&wrapper);
    card
}

/// Row content: icon (Font Awesome, optional), name and description, a widget on the right.
fn content_rows(icon: &str, name: &str, description: &str, right: Option<&gtk::Widget>) -> gtk::Box {
    let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    r.add_css_class("row");
    if !icon.is_empty() {
        let i = gtk::Label::new(Some(icon));
        i.add_css_class("icon");
        i.set_width_chars(2);
        r.append(&i);
    }
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_hexpand(true);
    texts.set_valign(gtk::Align::Center);
    let n = gtk::Label::new(Some(name));
    n.set_xalign(0.0);
    n.set_ellipsize(gtk::pango::EllipsizeMode::End);
    texts.append(&n);
    if !description.is_empty() {
        let p = gtk::Label::new(Some(description));
        p.add_css_class("description");
        p.set_xalign(0.0);
        p.set_wrap(true);
        p.set_max_width_chars(60);
        texts.append(&p);
    }
    r.append(&texts);
    if let Some(w) = right {
        w.set_valign(gtk::Align::Center);
        r.append(w);
    }
    r
}

/// A row with a control on the right (switch, slider, button, text).
pub fn add_row(card: &gtk::ListBox, icon: &str, name: &str, description: &str, right: Option<&gtk::Widget>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&content_rows(icon, name, description, right)));
    card.append(&row);
    row
}

/// A clickable row (network, device, output): the whole row is a button.
pub fn action_row(card: &gtk::ListBox, icon: &str, name: &str, description: &str, right: &str, action: impl Fn() + 'static) -> gtk::ListBoxRow {
    let b = gtk::Button::new();
    b.add_css_class("row-action");
    let v = gtk::Label::new(Some(right));
    v.add_css_class("description");
    b.set_child(Some(&content_rows(icon, name, description, Some(v.upcast_ref()))));
    b.connect_clicked(move |_| action());
    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&b));
    card.append(&row);
    row
}

/// A row with a switch; `on_change` gets the new state (only when the user changes it).
pub fn switch_row(card: &gtk::ListBox, name: &str, description: &str, enabled: bool, on_change: impl Fn(bool) + 'static) -> gtk::Switch {
    let s = gtk::Switch::new();
    s.set_active(enabled);
    s.connect_state_set(move |_, state| {
        on_change(state);
        gtk::glib::Propagation::Proceed
    });
    add_row(card, "", name, description, Some(s.upcast_ref()));
    s
}

/// A row with a “loading” text (until the data arrives from the background).
pub fn loading(card: &gtk::ListBox, text: &str) {
    add_row(card, "", text, "", None);
}

pub fn clear(card: &gtk::ListBox) {
    while let Some(r) = card.first_child() {
        card.remove(&r);
    }
}

/// A small button in a row (Disconnect, Forget…); `main` = in the accent colour.
pub fn button(text: &str, main: bool, action: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class(if main { "main" } else { "secondary" });
    b.connect_clicked(move |_| action());
    b
}
