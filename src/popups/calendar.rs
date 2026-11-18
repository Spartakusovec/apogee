//! Calendar under the clock: the month from Monday, week numbers, today highlighted.
//! Paging: ‹ › arrows, scroll / swipe on the touchpad, arrow keys; Today jumps back.

use std::cell::Cell;
use std::rc::Rc;

use chrono::{Datelike, Local, Months, NaiveDate};
use gtk::glib;
use gtk::prelude::*;

use crate::common::*;

struct Kalendar {
    o: Rc<Popup>,
    today: NaiveDate,
    month: Cell<NaiveDate>,
    name: gtk::Label,
    grid: gtk::Grid,
    offset: Cell<f64>,
}

pub fn fill(o: &Rc<Popup>) {
    let today = Local::now().date_naive();
    o.panel.append(&label(&format!("{}  ·  Week {}", today.format("%A, %-d %B %Y"), today.iso_week().week()), &["dim"]));

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let name = label("", &["title"]);
    name.set_hexpand(true);
    header.append(&name);
    let k = Rc::new(Kalendar {
        o: o.clone(),
        today,
        month: Cell::new(today.with_day(1).unwrap_or(today)),
        name,
        grid: gtk::Grid::new(),
        offset: Cell::new(0.0),
    });
    for (text, step, class) in [("\u{f053}", -1, "arrow"), ("Today", 0, "link"), ("\u{f054}", 1, "arrow")] {
        let b = gtk::Button::with_label(text);
        b.add_css_class(class);
        if class == "arrow" {
            b.add_css_class("quick-icon");
        }
        let k2 = k.clone();
        b.connect_clicked(move |_| k2.page(step));
        header.append(&b);
    }
    o.panel.append(&header);
    k.grid.set_column_spacing(2);
    k.grid.set_row_spacing(2);
    o.panel.append(&k.grid);
    o.panel.append(&separator());
    o.panel.append(&link(o, "Open the Calendar app", "gnome-calendar", &[]));

    // mouse wheel / two-finger swipe
    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let k2 = k.clone();
    scroll.connect_scroll(move |_, _, dy| {
        // the touchpad sends lots of small offsets: page only after a proper swipe
        let total = k2.offset.get() + dy;
        if total.abs() >= 1.0 {
            k2.offset.set(0.0);
            k2.page(if total > 0.0 { 1 } else { -1 });
        } else {
            k2.offset.set(total);
        }
        glib::Propagation::Stop
    });
    o.window.add_controller(scroll);

    // arrow keys
    let keys = gtk::EventControllerKey::new();
    let k2 = k.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        use gtk::gdk::Key;
        let step = match key {
            Key::Left | Key::Up => -1,
            Key::Right | Key::Down => 1,
            _ => return glib::Propagation::Proceed,
        };
        k2.page(step);
        glib::Propagation::Stop
    });
    o.window.add_controller(keys);
    k.draw();
}

impl Kalendar {
    fn page(self: &Rc<Self>, step: i32) {
        let m = self.month.get();
        self.month.set(match step {
            0 => self.today.with_day(1).unwrap_or(self.today),
            k if k > 0 => m.checked_add_months(Months::new(1)).unwrap_or(m),
            _ => m.checked_sub_months(Months::new(1)).unwrap_or(m),
        });
        self.draw();
    }

    fn draw(self: &Rc<Self>) {
        while let Some(d) = self.grid.first_child() {
            self.grid.remove(&d);
        }
        let month = self.month.get();
        self.name.set_text(&month.format("%B %Y").to_string());
        for (column, day) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].iter().enumerate() {
            let l = label(day, &["day-header"]);
            l.set_xalign(0.5);
            self.grid.attach(&l, column as i32 + 1, 0, 1, 1);
        }
        // from the Monday of the week with the 1st of the month, six rows like calendar launchers have
        let mut day = month - chrono::Duration::days(month.weekday().num_days_from_monday() as i64);
        let last = month.checked_add_months(Months::new(1)).unwrap_or(month) - chrono::Duration::days(1);
        let mut row = 1;
        while day <= last || day.weekday() != chrono::Weekday::Mon {
            if day.weekday() == chrono::Weekday::Mon {
                let t = label(&day.iso_week().week().to_string(), &["week"]);
                t.set_xalign(0.5);
                self.grid.attach(&t, 0, row, 1, 1);
            }
            let l = label(&day.day().to_string(), &["day"]);
            l.set_xalign(0.5);
            if day.month() != month.month() {
                l.add_css_class("other-month");
            } else if day == self.today {
                l.add_css_class("today");
            } else if day.weekday().num_days_from_monday() >= 5 {
                l.add_css_class("weekend");
            }
            self.grid.attach(&l, day.weekday().num_days_from_monday() as i32 + 1, row, 1, 1);
            if day.weekday() == chrono::Weekday::Sun {
                row += 1;
            }
            day += chrono::Duration::days(1);
        }
        self.o.recompute();
    }
}
