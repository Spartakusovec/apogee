//! Pomodoro in the notification popup: work and break times, start, pause, stop.

use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::pomodoro::{self, Phase};
use crate::common::*;

fn field(minutes: Option<u64>) -> gtk::Entry {
    let e = gtk::Entry::new();
    e.set_width_chars(3);
    e.set_max_width_chars(3);
    e.set_max_length(3);
    e.set_input_purpose(gtk::InputPurpose::Digits);
    if let Some(m) = minutes {
        e.set_text(&m.to_string());
    }
    e
}

fn minutes(e: &gtk::Entry) -> Option<u64> {
    e.text().trim().parse::<u64>().ok().filter(|&m| (1..=600).contains(&m))
}

pub fn section(o: &Rc<Popup>) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.append(&icon("\u{f2f2}", "icon"));
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
    texts.set_hexpand(true);
    texts.set_valign(gtk::Align::Center);
    texts.append(&label("Focus timer", &[]));
    let state = label("", &["dim"]);
    texts.append(&state);
    header.append(&texts);
    let pause = gtk::Button::with_label("Pause");
    pause.set_valign(gtk::Align::Center);
    let stop = gtk::Button::with_label("Stop");
    stop.set_valign(gtk::Align::Center);
    header.append(&pause);
    header.append(&stop);
    section.append(&header);

    // times: nothing starts until you set them
    let settings = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let (work, break_time) = pomodoro::times().map_or((None, None), |(a, b)| (Some(a), Some(b)));
    let field_work = field(work);
    let field_break_time = field(break_time);
    settings.append(&label("Work", &["dim"]));
    settings.append(&field_work);
    settings.append(&label("min", &["dim"]));
    let gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gap.set_size_request(6, -1);
    settings.append(&gap);
    settings.append(&label("Break", &["dim"]));
    settings.append(&field_break_time);
    settings.append(&label("min", &["dim"]));
    let gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gap.set_hexpand(true);
    settings.append(&gap);
    let start = gtk::Button::with_label("Start");
    start.add_css_class("enabled");
    settings.append(&start);
    section.append(&settings);

    let refresh = {
        let (pp, pr) = (&field_work, &field_break_time);
        Rc::new(glib::clone!(
            #[weak]
            state,
            #[weak]
            pause,
            #[weak]
            stop,
            #[weak]
            settings,
            #[weak]
            start,
            #[weak]
            pp,
            #[weak]
            pr,
            move || {
            match pomodoro::info() {
                Some((phase, remaining, paused)) => {
                    let cast = if phase == Phase::Work { "Work" } else { "Break" };
                    let count = pomodoro::countdown(remaining);
                    state.set_text(&if paused { format!("{cast} · paused at {count}") } else { format!("{cast} · {count} left") });
                    pause.set_label(if paused { "Resume" } else { "Pause" });
                    pause.set_visible(true);
                    stop.set_visible(true);
                    settings.set_visible(false);
                }
                None => {
                    state.set_text("Set your work and break minutes");
                    pause.set_visible(false);
                    stop.set_visible(false);
                    settings.set_visible(true);
                    start.set_sensitive(minutes(&pp).is_some() && minutes(&pr).is_some());
                }
            }
        }))
    };
    refresh();
    for e in [&field_work, &field_break_time] {
        let (refresh, o) = (refresh.clone(), Rc::downgrade(o));
        e.connect_changed(move |_| refresh());
        // don't close the popup while typing when the mouse leaves
        e.connect_has_focus_notify(move |e| {
            if let Some(o) = o.upgrade() {
                o.keep_open.set(e.has_focus());
            }
        });
    }
    let (ob, o2, pp, pr) = (refresh.clone(), Rc::downgrade(o), field_work.downgrade(), field_break_time.downgrade());
    start.connect_clicked(move |_| {
        let (Some(pp), Some(pr)) = (pp.upgrade(), pr.upgrade()) else { return };
        if let (Some(a), Some(b), Some(o)) = (minutes(&pp), minutes(&pr), o2.upgrade()) {
            o.keep_open.set(false);
            pomodoro::start(a, b);
            ob();
            o.recompute();
        }
    });
    let ob = refresh.clone();
    pause.connect_clicked(move |_| {
        pomodoro::pause_nebo_resume();
        ob();
    });
    let (ob, o2) = (refresh.clone(), Rc::downgrade(o));
    stop.connect_clicked(move |_| {
        pomodoro::stop();
        ob();
        if let Some(o) = o2.upgrade() {
            o.recompute();
        }
    });
    // countdown while the popup is open
    let weak = Rc::downgrade(o);
    glib::timeout_add_local(Duration::from_millis(500), move || match weak.upgrade() {
        Some(o) if o.window.is_visible() => {
            if pomodoro::info().is_some() {
                refresh();
            }
            glib::ControlFlow::Continue
        }
        _ => glib::ControlFlow::Break,
    });
    section
}
