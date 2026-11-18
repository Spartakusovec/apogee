//! Pomodoro: alternates work and break with times you set (remembered
//! in ~/.config/apogee/settings.json, there are no defaults). The bar counts down, at the end of each
//! part a notification arrives. It doesn't turn on Do Not Disturb. Controlled in the notification popup.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use gtk::glib;
use serde_json::json;

use crate::panel::publish;
use crate::common::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Phase {
    Work,
    Break,
}

struct State {
    phase: Phase,
    work: Duration,
    break_time: Duration,
    /// when the part ends (while running)
    end: Instant,
    /// how much was left when paused
    paused: Option<Duration>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// The set times in minutes (work, break), if they were ever set.
pub fn times() -> Option<(u64, u64)> {
    let v = settings("pomodoro");
    Some((v["work"].as_u64().filter(|&m| m > 0)?, v["break_time"].as_u64().filter(|&m| m > 0)?))
}

/// What runs now: part, time left, whether paused.
pub fn info() -> Option<(Phase, Duration, bool)> {
    STATE.with(|s| {
        s.borrow().as_ref().map(|s| match s.paused {
            Some(remaining) => (s.phase, remaining, true),
            None => (s.phase, s.end.saturating_duration_since(Instant::now()), false),
        })
    })
}

pub fn start(work: u64, break_time: u64) {
    save_settings("pomodoro", json!({"work": work, "break_time": break_time}));
    let running = STATE.with(|s| s.borrow().is_some());
    let (work, break_time) = (Duration::from_secs(60 * work), Duration::from_secs(60 * break_time));
    STATE.with(|s| s.replace(Some(State { phase: Phase::Work, work, break_time, end: Instant::now() + work, paused: None })));
    if !running {
        glib::timeout_add_local(Duration::from_millis(500), || {
            if tick() {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
    to_panel();
}

pub fn pause_nebo_resume() {
    STATE.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            match s.paused.take() {
                Some(remaining) => s.end = Instant::now() + remaining,
                None => s.paused = Some(s.end.saturating_duration_since(Instant::now())),
            }
        }
    });
    to_panel();
}

pub fn stop() {
    STATE.with(|s| s.replace(None));
    to_panel();
}

/// Every half second: end of a part → next part and a notification. false = the pomodoro ended.
fn tick() -> bool {
    let next = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut()?;
        if s.paused.is_some() || Instant::now() < s.end {
            return None;
        }
        s.phase = if s.phase == Phase::Work { Phase::Break } else { Phase::Work };
        let length = if s.phase == Phase::Work { s.work } else { s.break_time };
        s.end = Instant::now() + length;
        Some((s.phase, length.as_secs() / 60))
    });
    if let Some((phase, minutes)) = next {
        let (title, text) = match phase {
            Phase::Break => ("Time for a break".to_string(), format!("Work session done. Break: {minutes} min.")),
            Phase::Work => ("Break is over".to_string(), format!("Back to work: {minutes} min.")),
        };
        background("notify-send", &["-i", "alarm-symbolic", &title, &text]);
    }
    to_panel();
    STATE.with(|s| s.borrow().is_some())
}

pub fn countdown(d: Duration) -> String {
    let s = d.as_secs() + u64::from(d.subsec_nanos() > 0);
    format!("{}:{:02}", s / 60, s % 60)
}

/// Bar: countdown, only while the pomodoro runs (click = notification popup with the controls).
fn to_panel() {
    match info() {
        Some((phase, remaining, paused)) => {
            let class = if paused {
                "paused"
            } else if phase == Phase::Work {
                "work"
            } else {
                "break_time"
            };
            let icon = if phase == Phase::Work { "\u{f2f2}" } else { "\u{f0f4}" };
            publish("pomodoro", json!({"text": format!("{icon}  {}", countdown(remaining)), "class": class}));
        }
        None => publish("pomodoro", json!({"text": ""})),
    }
}

pub fn start_panel() {
    to_panel();
}
