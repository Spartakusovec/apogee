//! Player (click on the track in the bar): album cover, track, seeking and ⏮ ⏯ ⏭ buttons.
//! Controls the same player the bar shows (see music.rs).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;

use crate::music::{self, Player};
use crate::common::*;

const WRAPPER: i32 = 96;

struct Window {
    o: Rc<Popup>,
    content: gtk::Box,
    wrapper: gtk::Image,
    without_cover: gtk::Label,
    title: gtk::Label,
    artist: gtk::Label,
    source: gtk::Label,
    time_osa: gtk::Box,
    slider: gtk::Scale,
    time: gtk::Label,
    length: gtk::Label,
    previous: gtk::Button,
    play: gtk::Button,
    next: gtk::Button,
    address_cover: RefCell<String>,
    player: RefCell<Option<Player>>,
    /// after seeking, don't move the slider from the player for a moment (it doesn't know where it is yet)
    seeked: Cell<Option<Instant>>,
    pending_seek: RefCell<Option<glib::SourceId>>,
}

fn button(char: &str, classes: &[&str], action: &'static str) -> gtk::Button {
    let b = gtk::Button::with_label(char);
    b.add_css_class("controls");
    for t in classes {
        b.add_css_class(t);
    }
    b.connect_clicked(move |_| music::control(action));
    b
}

fn time(us: i64) -> String {
    let s = (us.max(0) / 1_000_000) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn fill(o: &Rc<Popup>) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);

    let hore = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    let frame = gtk::Overlay::new();
    frame.add_css_class("wrapper");
    frame.set_overflow(gtk::Overflow::Hidden);
    frame.set_size_request(WRAPPER, WRAPPER);
    frame.set_valign(gtk::Align::Start);
    let without_cover = icon("\u{f001}", "wrapper-fallback");
    frame.set_child(Some(&without_cover));
    let wrapper = gtk::Image::new();
    wrapper.set_pixel_size(WRAPPER);
    frame.add_overlay(&wrapper);
    hore.append(&frame);
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 3);
    texts.set_valign(gtk::Align::Center);
    texts.set_hexpand(true);
    let title = label("", &["title-track"]);
    title.set_wrap(true);
    title.set_lines(2);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(22);
    let artist = label("", &[]);
    artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let source = label("", &["dim"]);
    source.set_ellipsize(gtk::pango::EllipsizeMode::End);
    texts.append(&title);
    texts.append(&artist);
    texts.append(&source);
    hore.append(&texts);
    content.append(&hore);

    let time_osa = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    slider.set_draw_value(false);
    time_osa.append(&slider);
    let times = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let time_lbl = label("0:00", &["small-time"]);
    time_lbl.set_hexpand(true);
    let length = label("", &["small-time"]);
    times.append(&time_lbl);
    times.append(&length);
    time_osa.append(&times);
    content.append(&time_osa);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    buttons.set_halign(gtk::Align::Center);
    let previous = button("\u{f048}", &[], "previous");
    let play = button("\u{f04b}", &["main"], "play-pause");
    let next = button("\u{f051}", &[], "next");
    previous.set_valign(gtk::Align::Center);
    next.set_valign(gtk::Align::Center);
    buttons.append(&previous);
    buttons.append(&play);
    buttons.append(&next);
    content.append(&buttons);
    o.panel.append(&content);

    let window = Rc::new(Window {
        o: o.clone(),
        content,
        wrapper,
        without_cover,
        title,
        artist,
        source,
        time_osa,
        slider: slider.clone(),
        time: time_lbl,
        length,
        previous,
        play,
        next,
        address_cover: RefCell::new(String::new()),
        player: RefCell::new(None),
        seeked: Cell::new(None),
        pending_seek: RefCell::new(None),
    });

    // seeking: dragging the slider sends the new position once the hand stops for a moment
    let weak = Rc::downgrade(&window);
    slider.connect_change_value(move |_, _, value| {
        if let Some(w) = weak.upgrade() {
            w.seeked.set(Some(Instant::now()));
            w.time.set_text(&time((value * 1e6) as i64));
            if let Some(id) = w.pending_seek.take() {
                id.remove();
            }
            let w2 = w.clone();
            let id = glib::timeout_add_local_once(Duration::from_millis(150), move || {
                w2.pending_seek.replace(None);
                if let Some(p) = w2.player.borrow().as_ref() {
                    music::seek(p, (value * 1e6) as i64);
                }
            });
            w.pending_seek.replace(Some(id));
        }
        glib::Propagation::Proceed
    });

    let weak = Rc::downgrade(&window);
    music::on_change(Some(Box::new(move || {
        if let Some(w) = weak.upgrade() {
            w.refresh();
        }
    })));
    o.window.connect_destroy(|_| music::on_change(None));

    // the position isn't reported by itself, ask every second while the popup is open
    let weak = Rc::downgrade(&window);
    glib::timeout_add_local(Duration::from_secs(1), move || match weak.upgrade() {
        Some(w) if w.o.window.is_visible() => {
            w.position();
            glib::ControlFlow::Continue
        }
        _ => glib::ControlFlow::Break,
    });

    window.refresh();
}

impl Window {
    fn refresh(self: &Rc<Self>) {
        let Some(p) = music::active() else {
            self.content.set_visible(false);
            self.o.state("Nothing is playing");
            self.player.replace(None);
            return;
        };
        self.content.set_visible(true);
        self.o.state("");
        self.title.set_text(if p.title.is_empty() { &p.identity } else { &p.title });
        self.artist.set_text(&p.artist);
        self.artist.set_visible(!p.artist.is_empty());
        let source: Vec<&str> = [p.album.as_str(), p.identity.as_str()].into_iter().filter(|t| !t.is_empty()).collect();
        self.source.set_text(&source.join(" · "));
        self.play.set_label(if p.state == "Playing" { "\u{f04c}" } else { "\u{f04b}" });
        self.previous.set_sensitive(p.can_previous);
        self.next.set_sensitive(p.can_next);
        self.time_osa.set_visible(p.length > 0);
        self.slider.set_sensitive(p.can_seek);
        self.slider.set_range(0.0, (p.length as f64 / 1e6).max(1.0));
        self.length.set_text(&time(p.length));
        self.set_wrapper(&p.wrapper);
        self.player.replace(Some(p));
        self.position();
        self.o.recompute();
    }

    fn position(self: &Rc<Self>) {
        let Some(p) = self.player.borrow().clone() else { return };
        if p.length <= 0 {
            return;
        }
        let w = self.clone();
        music::position(&p, move |us| {
            if w.seeked.get().is_some_and(|t| t.elapsed() < Duration::from_millis(1500)) {
                return;
            }
            w.slider.set_value(us as f64 / 1e6);
            w.time.set_text(&time(us));
        });
    }

    /// Album cover: a local file directly, from the internet it's downloaded to ~/.cache/apogee/covers.
    fn set_wrapper(self: &Rc<Self>, address: &str) {
        if *self.address_cover.borrow() == address {
            return;
        }
        self.address_cover.replace(address.to_string());
        self.wrapper.clear();
        self.without_cover.set_visible(true);
        if let Some(path) = address.starts_with("file://").then(|| glib::filename_from_uri(address).ok()).flatten() {
            self.show_wrapper(&path.0);
        } else if address.starts_with("http://") || address.starts_with("https://") {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            address.hash(&mut h);
            let folder = home().join(".cache/apogee/covers");
            let path = folder.join(format!("{:016x}", h.finish()));
            if path.exists() {
                self.show_wrapper(&path);
                return;
            }
            let (w, a, c, wanted) = (self.clone(), address.to_string(), path.clone(), address.to_string());
            in_background(
                move || {
                    let _ = std::fs::create_dir_all(&folder);
                    let temporary = path.with_extension("downloading");
                    let ok = std::process::Command::new("curl")
                        .args(["-sf", "--max-time", "10", "-o"])
                        .arg(&temporary)
                        .arg(&a)
                        .status()
                        .is_ok_and(|s| s.success());
                    ok && std::fs::rename(&temporary, &path).is_ok()
                },
                move |ok| {
                    // the track may have changed meanwhile
                    if ok && *w.address_cover.borrow() == wanted {
                        w.show_wrapper(&c);
                    }
                },
            );
        }
    }

    fn show_wrapper(&self, path: &std::path::Path) {
        if path.exists() {
            self.wrapper.set_from_file(Some(path));
            self.without_cover.set_visible(false);
        }
    }
}
