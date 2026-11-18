//! Notification bubbles at the top right under the bar. A click runs the default action (e.g. “Edit” for a screenshot),
//! otherwise it closes the bubble; a right click closes it. A bubble doesn't disappear while the mouse is over it. At most five,
//! newest on top.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use super::{Reason, Notification};

const MAX: usize = 5;
const ICON: i32 = 40;

struct Bubble {
    id: u32,
    card: gtk::Box,
    urgency: u8,
    duration_ms: u32,
    timer: RefCell<Option<glib::SourceId>>,
}

thread_local! {
    static WINDOW: RefCell<Option<(gtk::Window, gtk::Box)>> = const { RefCell::new(None) };
    static BUBBLES: RefCell<Vec<Rc<Bubble>>> = const { RefCell::new(Vec::new()) };
}

fn window() -> (gtk::Window, gtk::Box) {
    if let Some(o) = WINDOW.with(|o| o.borrow().clone()) {
        return o;
    }
    crate::style::apply();
    let window = gtk::Window::new();
    window.add_css_class("apogee");
    window.add_css_class("bubbles");
    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.set_namespace(Some("notifications"));
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Right, true);
    window.set_margin(Edge::Top, 6);
    window.set_margin(Edge::Right, 8);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    window.set_child(Some(&column));
    WINDOW.with(|o| o.replace(Some((window.clone(), column.clone()))));
    (window, column)
}

pub fn is_visible(id: u32) -> bool {
    BUBBLES.with(|b| b.borrow().iter().any(|x| x.id == id))
}

fn image(n: &Notification) -> gtk::Image {
    let name = if !n.icon.is_empty() { n.icon.clone() } else if !n.desktop.is_empty() { crate::apps::info(&n.desktop).icon } else { "dialog-information".into() };
    let i = if name.starts_with('/') { gtk::Image::from_file(&name) } else { gtk::Image::from_icon_name(&name) };
    i.set_pixel_size(ICON);
    i.set_valign(gtk::Align::Start);
    i
}

fn content(n: &Notification, card: &gtk::Box) {
    while let Some(d) = card.first_child() {
        card.remove(&d);
    }
    card.append(&image(n));
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_hexpand(true);
    let row = |text: &str, class: &str, tags: bool| {
        let l = gtk::Label::new(None);
        l.set_xalign(0.0);
        l.set_wrap(true);
        l.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        l.set_max_width_chars(36);
        l.add_css_class(class);
        // the body may have markup (<b>…); broken markup is shown as plain text
        if tags && gtk::pango::parse_markup(text, '\0').is_ok() {
            l.set_markup(text);
        } else {
            l.set_text(text);
        }
        l
    };
    if !n.apps.is_empty() {
        texts.append(&row(&n.apps, "apps", false));
    }
    if !n.title.is_empty() {
        texts.append(&row(&n.title, "title", false));
    }
    if !n.body.is_empty() {
        let t = row(&n.body, "body", true);
        t.set_lines(4);
        t.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texts.append(&t);
    }
    if let Some(p) = n.progress {
        let u = gtk::LevelBar::for_interval(0.0, 100.0);
        u.set_mode(gtk::LevelBarMode::Continuous);
        for name in ["low", "high", "full"] {
            u.remove_offset_value(Some(name));
        }
        u.set_value(p.clamp(0, 100) as f64);
        u.set_margin_top(6);
        texts.append(&u);
    }
    // other actions as buttons (“default” is a click on the bubble)
    let buttons: Vec<&(String, String)> = n.action.iter().filter(|(k, _)| k != "default").collect();
    if !buttons.is_empty() {
        let rada = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        rada.set_margin_top(6);
        for (key, label) in buttons {
            let b = gtk::Button::with_label(label);
            b.add_css_class("action");
            let (id, key) = (n.id, key.clone());
            b.connect_clicked(move |_| {
                super::invoke_action(id, &key);
                close(id, Reason::Closed);
            });
            rada.append(&b);
        }
        texts.append(&rada);
    }
    card.append(&texts);
}

fn schedule(b: &Rc<Bubble>) {
    if let Some(id) = b.timer.take() {
        id.remove();
    }
    if b.duration_ms == 0 {
        return;
    }
    let id = b.id;
    let c = glib::timeout_add_local_once(Duration::from_millis(b.duration_ms as u64), move || {
        if let Some(x) = BUBBLES.with(|v| v.borrow().iter().find(|x| x.id == id).cloned()) {
            x.timer.replace(None);
        }
        close(id, Reason::Expired);
    });
    b.timer.replace(Some(c));
}

pub fn show(n: &Notification) {
    let (window, column) = window();
    // same id (replacement): only new content and a new time
    if let Some(b) = BUBBLES.with(|v| v.borrow().iter().find(|x| x.id == n.id).cloned()) {
        content(n, &b.card);
        schedule(&b);
        return;
    }
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("bubble");
    if n.urgency == 2 {
        card.add_css_class("critical");
    }
    card.set_size_request(380, -1);
    content(n, &card);
    let b = Rc::new(Bubble { id: n.id, card: card.clone(), urgency: n.urgency, duration_ms: n.duration_ms, timer: RefCell::new(None) });

    let (id, ma_default) = (n.id, n.action.iter().any(|(k, _)| k == "default"));
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.connect_released(move |g, _, _, _| match g.current_button() {
        1 => {
            if ma_default {
                super::invoke_action(id, "default");
            }
            close(id, Reason::Closed);
        }
        3 => close(id, Reason::Closed),
        _ => {}
    });
    card.add_controller(click);
    // mouse over the bubble: it stays
    let mouse = gtk::EventControllerMotion::new();
    let weak = Rc::downgrade(&b);
    mouse.connect_enter(move |_, _, _| {
        if let Some(b) = weak.upgrade() {
            if let Some(c) = b.timer.take() {
                c.remove();
            }
        }
    });
    let weak = Rc::downgrade(&b);
    mouse.connect_leave(move |_| {
        if let Some(b) = weak.upgrade() {
            schedule(&b);
        }
    });
    card.add_controller(mouse);

    column.prepend(&card);
    schedule(&b);
    BUBBLES.with(|v| v.borrow_mut().insert(0, b));
    // more than five: the oldest goes (it stays in the history)
    let extra: Vec<u32> = BUBBLES.with(|v| v.borrow().iter().skip(MAX).map(|x| x.id).collect());
    for id in extra {
        close(id, Reason::Expired);
    }
    window.set_default_size(1, 1);
    window.set_visible(true);
}

/// Closes the bubble (if visible) and tells the app.
pub fn close(id: u32, reason: Reason) {
    let Some(b) = BUBBLES.with(|v| {
        let mut v = v.borrow_mut();
        let i = v.iter().position(|x| x.id == id)?;
        Some(v.remove(i))
    }) else {
        return;
    };
    if let Some(c) = b.timer.take() {
        c.remove();
    }
    if let Some((window, column)) = WINDOW.with(|o| o.borrow().clone()) {
        column.remove(&b.card);
        if column.first_child().is_none() {
            window.set_visible(false);
            crate::common::return_memory();
        } else {
            window.set_default_size(1, 1);
        }
    }
    super::announce_close(id, reason);
}

/// Do Not Disturb: close all bubbles except critical ones.
pub fn hide_non_critical() {
    let id: Vec<u32> = BUBBLES.with(|v| v.borrow().iter().filter(|x| x.urgency != 2).map(|x| x.id).collect());
    for i in id {
        close(i, Reason::Closed);
    }
}
