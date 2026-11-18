//! Top bar (was waybar): three islands at the top of the screen.
//!
//!   left      app name (click = window menu), tiling, workspaces with window icons, Sway mode
//!   middle    clock, pomodoro, music with ⏮ ⏭ buttons
//!   right     recording, microphone, weather, tray, sound, brightness, network, battery, updates, bell, power
//!
//! Modules are labels with Pango text; their content (JSON {text, class, tooltip}) comes from the sources
//! through panel::publish, the bar only draws it. Empty text = module hidden.
//! Scrolling over sound and brightness changes volume / brightness, over workspaces it switches them.

pub mod sources;
pub mod tray;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;

/// What a click / scroll on a module does.
#[derive(Clone, Copy)]
enum Action {
    Popup(&'static str),
    Fn(fn()),
    Nic,
}

struct Module {
    id: &'static str,
    levy: Action,
    middle: Action,
    right: Action,
    /// scroll up, down
    scroll: Option<(fn(), fn())>,
    /// fixed tooltip (otherwise from the data, only for modules that have one)
    tooltip: Option<&'static str>,
}

const fn m(id: &'static str, levy: Action) -> Module {
    Module { id, levy, middle: Action::Nic, right: Action::Nic, scroll: None, tooltip: None }
}

fn modules_left() -> Vec<Module> {
    vec![
        m("apps", Action::Popup("window")),
        Module { tooltip: None, ..m("tiling", Action::Fn(crate::sway::toggle_tiling)) },
    ]
}

fn modules_centered() -> Vec<Module> {
    vec![
        m("clock", Action::Popup("calendar")),
        m("pomodoro", Action::Popup("notifications")),
        Module { tooltip: Some("Previous track"), ..m("previous", Action::Fn(|| crate::music::control("previous"))) },
        // right click = same as middle: the touchpad knows at most two fingers, a three-finger tap is a right click
        Module { middle: Action::Fn(play_pause), right: Action::Fn(play_pause), ..m("music", Action::Popup("player")) },
        Module { tooltip: Some("Next track"), ..m("next", Action::Fn(|| crate::music::control("next"))) },
    ]
}

fn play_pause() {
    crate::music::control("play-pause");
}

fn pavucontrol() {
    crate::common::background("pavucontrol", &[]);
}

fn modules_right() -> Vec<Module> {
    vec![
        m("recording", Action::Fn(crate::recording::stop)),
        m("microphone", Action::Popup("sound")),
        // keyboard layout (only with more than one): click = next, right click = Settings
        Module {
            right: Action::Fn(|| crate::settings::open("input")),
            ..m("layouts", Action::Fn(|| crate::sway::command("input type:keyboard xkb_switch_layout next")))
        },
        Module { right: Action::Fn(|| crate::popups::command("weather-refresh", "")), ..m("weather", Action::Popup("weather")) },
        // the tray goes here (tray::widget)
        Module {
            middle: Action::Fn(pavucontrol),
            right: Action::Fn(pavucontrol),
            scroll: Some((|| crate::osd::volume("vol-up"), || crate::osd::volume("vol-down"))),
            ..m("sound", Action::Popup("sound"))
        },
        Module { scroll: Some((|| crate::osd::brightness("brightness-up"), || crate::osd::brightness("brightness-down"))), ..m("brightness", Action::Popup("brightness")) },
        m("network", Action::Popup("wifi")),
        m("battery", Action::Popup("battery")),
        m("updates", Action::Popup("updates")),
        Module { right: Action::Fn(|| crate::popups::command("dnd", "")), ..m("notifications", Action::Popup("notifications")) },
        m("power", Action::Popup("power")),
    ]
}

/// Modules whose tooltip comes from the data (the others open a popup instead).
const WITH_TOOLTIP: &[&str] = &["tiling", "notifications", "layouts"];

/// Centre of the module in px from the left screen edge and the screen width (for the popup under it).
fn remember_anchor(w: &gtk::Widget) {
    let Some(koren) = w.root() else { return };
    let Some(point) = w.compute_point(&koren, &gtk::graphene::Point::new(w.width() as f32 / 2.0, 0.0)) else { return };
    let Some(surface) = w.native().and_then(|n| n.surface()) else { return };
    let Some(monitor) = surface.display().monitor_at_surface(&surface) else { return };
    // the bar is 8 px from the left screen edge (start: set_margin)
    crate::common::set_anchor(point.x() as f64 + 8.0, monitor.geometry().width());
}

fn run(a: Action) {
    match a {
        // only after the click is handled (the popup takes the keyboard)
        Action::Popup(kind) => {
            glib::idle_add_local_once(move || crate::popups::toggle(kind));
        }
        Action::Fn(f) => f(),
        Action::Nic => {}
    }
}

struct Bar {
    /// module → (wrapper, label, last class from the data)
    modules: RefCell<HashMap<&'static str, (gtk::Box, gtk::Label, String)>>,
    workspaces: gtk::Box,
}

thread_local! {
    static BAR: RefCell<Option<Rc<Bar>>> = const { RefCell::new(None) };
}

fn create_module(md: &Module, bar: &Bar) -> gtk::Box {
    let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    wrapper.set_widget_name(md.id);
    wrapper.add_css_class("module");
    if !matches!(md.levy, Action::Nic) {
        wrapper.add_css_class("clickable");
    }
    let label = gtk::Label::new(None);
    label.set_use_markup(true);
    label.set_valign(gtk::Align::Center);
    wrapper.append(&label);
    wrapper.set_visible(false);
    if let Some(n) = md.tooltip {
        wrapper.set_tooltip_text(Some(n));
    }
    let (levy, middle, right) = (md.levy, md.middle, md.right);
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.connect_released(move |g, _, _, _| {
        let action = match g.current_button() {
            1 => levy,
            2 => middle,
            3 => right,
            _ => return,
        };
        // the popup opens under this module
        if matches!(action, Action::Popup(_)) {
            if let Some(w) = g.widget() {
                remember_anchor(&w);
            }
        }
        run(action);
    });
    wrapper.add_controller(click);
    if let Some((up, down)) = md.scroll {
        add_scroll(&wrapper, STEP_VOLUME, Duration::ZERO, move |n| if n { up() } else { down() });
    }
    bar.modules.borrow_mut().insert(md.id, (wrapper.clone(), label, String::new()));
    wrapper
}

/// The touchpad sends scroll in pixels (several events per finger move), a mouse wheel in notches.
/// How many pixels of two-finger scrolling make one volume / brightness step or one workspace switch.
const STEP_VOLUME: f64 = 25.0;
const STEP_PLOCH: f64 = 30.0;

/// Wheel / two-finger scroll: `action(true)` = up (fingers up, even with natural scrolling),
/// `action(false)` = down. `delay` = shortest time between two actions (one swipe = one workspace).
fn add_scroll(w: &impl IsA<gtk::Widget>, step_px: f64, delay: Duration, action: impl Fn(bool) + 'static) {
    use gtk::EventControllerScrollFlags as F;
    let c = gtk::EventControllerScroll::new(F::BOTH_AXES | F::PHYSICAL_DIRECTION);
    let accumulated = Rc::new(Cell::new(0.0f64));
    let last = Rc::new(Cell::new(None::<Instant>));
    let n = accumulated.clone();
    c.connect_scroll(move |c, dx, dy| {
        // horizontal scroll (fingers to the right = next) counts as vertical
        let d = if dy.abs() >= dx.abs() { dy } else { dx };
        let step = if c.unit() == gtk::gdk::ScrollUnit::Surface { step_px } else { 1.0 };
        let mut s = n.get();
        if s * d < 0.0 {
            s = 0.0; // direction reversed: start again
        }
        s += d;
        if s.abs() >= step * 0.99 {
            let up = s < 0.0;
            s = 0.0;
            if last.get().is_none_or(|t| t.elapsed() >= delay) {
                last.set(Some(Instant::now()));
                action(up);
            }
        }
        n.set(s);
        glib::Propagation::Stop
    });
    c.connect_scroll_end(move |_| accumulated.set(0.0));
    w.add_controller(c);
}

fn island() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class("island");
    b
}

/// Modules the user hid in Settings (`panel_hidden` in settings.json).
fn hidden(module: &str) -> bool {
    let id = match module {
        "previous" | "next" => "music",
        m => m,
    };
    crate::common::settings("panel_hidden").as_array().is_some_and(|a| a.iter().any(|v| v.as_str() == Some(id)))
}

/// After a change in Settings: show / hide the modules again according to the settings.
pub fn refresh_modules() {
    let Some(l) = BAR.with(|l| l.borrow().clone()) else { return };
    for (id, (wrapper, label, _)) in l.modules.borrow().iter() {
        if *id != "mode" {
            wrapper.set_visible(!label.text().is_empty() && !hidden(id));
        }
    }
    sources::change_format();
}

pub fn start() {
    crate::style::apply();
    let window = gtk::Window::new();
    window.add_css_class("bar");
    window.init_layer_shell();
    window.set_layer(Layer::Top);
    window.set_namespace(Some("bar"));
    window.set_keyboard_mode(KeyboardMode::None);
    for (k, margin) in [(Edge::Top, 8), (Edge::Left, 8), (Edge::Right, 8)] {
        window.set_anchor(k, true);
        window.set_margin(k, margin);
    }
    window.auto_exclusive_zone_enable();

    let workspaces = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    workspaces.add_css_class("workspaces");
    let bar = Rc::new(Bar { modules: RefCell::new(HashMap::new()), workspaces: workspaces.clone() });

    let left = island();
    for md in modules_left() {
        left.append(&create_module(&md, &bar));
    }
    left.append(&workspaces);
    // scrolling over the workspaces switches them
    add_scroll(&workspaces, STEP_PLOCH, Duration::from_millis(250), |up| crate::sway::workspace_next_to(!up));
    let mode = gtk::Label::new(None);
    mode.set_widget_name("mode");
    mode.add_css_class("module");
    mode.set_visible(false);
    left.append(&mode);
    bar.modules.borrow_mut().insert("mode", (gtk::Box::new(gtk::Orientation::Horizontal, 0), mode.clone(), String::new()));
    // the mode has only a label (no wrapper): the wrapper above is a dummy, the label is hidden
    let centered = island();
    for md in modules_centered() {
        centered.append(&create_module(&md, &bar));
    }
    let right = island();
    for md in modules_right() {
        let w = create_module(&md, &bar);
        right.append(&w);
        if md.id == "weather" {
            right.append(&tray::widget());
        }
    }

    let center = gtk::CenterBox::new();
    center.set_start_widget(Some(&left));
    center.set_center_widget(Some(&centered));
    center.set_end_widget(Some(&right));
    window.set_child(Some(&center));
    BAR.with(|l| l.replace(Some(bar)));

    let rx = crate::panel::subscribe();
    glib::spawn_future_local(async move {
        while let Ok((module, json)) = rx.recv().await {
            if let Some(l) = BAR.with(|l| l.borrow().clone()) {
                l.show(&module, &json);
            }
        }
    });
    sources::start();
    window.present();
}

impl Bar {
    fn show(&self, module: &str, json: &str) {
        let v: Value = serde_json::from_str(json).unwrap_or_default();
        if module == "workspaces" {
            self.workspaces_draw(&v);
            return;
        }
        let mut modules = self.modules.borrow_mut();
        let Some((wrapper, label, old_class)) = modules.get_mut(module) else { return };
        let text = v["text"].as_str().unwrap_or("");
        if module == "mode" {
            label.set_markup(text);
            label.set_visible(!text.is_empty());
            return;
        }
        label.set_markup(text);
        wrapper.set_visible(!text.is_empty() && !hidden(module));
        let class = v["class"].as_str().unwrap_or("").to_string();
        if *old_class != class {
            for t in old_class.split_whitespace() {
                wrapper.remove_css_class(t);
            }
            for t in class.split_whitespace() {
                wrapper.add_css_class(t);
            }
            *old_class = class;
        }
        if WITH_TOOLTIP.contains(&module) {
            let n = v["tooltip"].as_str().unwrap_or("");
            wrapper.set_tooltip_markup(if n.is_empty() { None } else { Some(n) });
        }
    }

    /// Workspaces: number and window icons, active highlighted, empty dimmed; click switches.
    fn workspaces_draw(&self, v: &Value) {
        let workspaces = v["workspaces"].as_array().cloned().unwrap_or_default();
        // rebuild only when the count changed, otherwise update in place
        let mut buttons: Vec<gtk::Box> = Vec::new();
        let mut d = self.workspaces.first_child();
        while let Some(w) = d {
            d = w.next_sibling();
            if let Ok(b) = w.downcast::<gtk::Box>() {
                buttons.push(b);
            }
        }
        if buttons.len() != workspaces.len() {
            for t in &buttons {
                self.workspaces.remove(t);
            }
            buttons.clear();
            for _ in &workspaces {
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                b.add_css_class("workspace");
                let l = gtk::Label::new(None);
                l.set_valign(gtk::Align::Center);
                b.append(&l);
                let click = gtk::GestureClick::new();
                let slaby = b.downgrade();
                click.connect_released(move |_, _, _, _| {
                    if let Some(b) = slaby.upgrade() {
                        crate::sway::command(&format!("workspace \"{}\"", b.widget_name().replace('"', "")));
                    }
                });
                b.add_controller(click);
                self.workspaces.append(&b);
                buttons.push(b);
            }
        }
        for (b, p) in buttons.iter().zip(workspaces.iter()) {
            let name = p["name"].as_str().unwrap_or("");
            b.set_widget_name(name);
            let icons = p["icons"].as_str().unwrap_or("");
            let text = if icons.is_empty() { name.to_string() } else { format!("{name}  {icons}") };
            if let Some(l) = b.first_child().and_then(|w| w.downcast::<gtk::Label>().ok()) {
                l.set_text(&text);
            }
            for (class, key) in [("active", "active"), ("empty", "empty"), ("urgent", "urgent")] {
                if p[key].as_bool().unwrap_or(false) {
                    b.add_css_class(class);
                } else {
                    b.remove_css_class(class);
                }
            }
        }
    }
}
