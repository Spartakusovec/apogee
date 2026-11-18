//! Pomodoro: střídá práci a přestávku podle časů, které si nastavíš (pamatují se
//! v ~/.config/apogee/settings.json, výchozí žádné nejsou). Panel odpočítává, na konci každé
//! části přijde notifikace. Do Not Disturb nezapíná. Ovládá se v okénku notifikací.

use std::cell::RefCell;
use std::time::{Duration, Instant};

use gtk::glib;
use serde_json::json;

use crate::panel::publikovat;
use crate::spolecne::*;

#[derive(Clone, Copy, PartialEq)]
pub enum Faze {
    Prace,
    Prestavka,
}

struct Stav {
    faze: Faze,
    prace: Duration,
    prestavka: Duration,
    /// kdy část skončí (když běží)
    konec: Instant,
    /// kolik zbývalo při pozastavení
    pozastaveno: Option<Duration>,
}

thread_local! {
    static STAV: RefCell<Option<Stav>> = const { RefCell::new(None) };
}

/// Nastavené časy v minutách (práce, přestávka), když už je někdy nastavil.
pub fn casy() -> Option<(u64, u64)> {
    let v = nastaveni("pomodoro");
    Some((v["prace"].as_u64().filter(|&m| m > 0)?, v["prestavka"].as_u64().filter(|&m| m > 0)?))
}

/// Co teď běží: část, kolik zbývá, jestli je pozastaveno.
pub fn info() -> Option<(Faze, Duration, bool)> {
    STAV.with(|s| {
        s.borrow().as_ref().map(|s| match s.pozastaveno {
            Some(zbyva) => (s.faze, zbyva, true),
            None => (s.faze, s.konec.saturating_duration_since(Instant::now()), false),
        })
    })
}

pub fn spustit(prace: u64, prestavka: u64) {
    ulozit_nastaveni("pomodoro", json!({"prace": prace, "prestavka": prestavka}));
    let bezi = STAV.with(|s| s.borrow().is_some());
    let (prace, prestavka) = (Duration::from_secs(60 * prace), Duration::from_secs(60 * prestavka));
    STAV.with(|s| s.replace(Some(Stav { faze: Faze::Prace, prace, prestavka, konec: Instant::now() + prace, pozastaveno: None })));
    if !bezi {
        glib::timeout_add_local(Duration::from_millis(500), || {
            if tik() {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
    do_panelu();
}

pub fn pozastavit_nebo_pokracovat() {
    STAV.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            match s.pozastaveno.take() {
                Some(zbyva) => s.konec = Instant::now() + zbyva,
                None => s.pozastaveno = Some(s.konec.saturating_duration_since(Instant::now())),
            }
        }
    });
    do_panelu();
}

pub fn zastavit() {
    STAV.with(|s| s.replace(None));
    do_panelu();
}

/// Každou půlsekundu: konec části → další část a notifikace. false = pomodoro skončilo.
fn tik() -> bool {
    let dalsi = STAV.with(|s| {
        let mut s = s.borrow_mut();
        let s = s.as_mut()?;
        if s.pozastaveno.is_some() || Instant::now() < s.konec {
            return None;
        }
        s.faze = if s.faze == Faze::Prace { Faze::Prestavka } else { Faze::Prace };
        let delka = if s.faze == Faze::Prace { s.prace } else { s.prestavka };
        s.konec = Instant::now() + delka;
        Some((s.faze, delka.as_secs() / 60))
    });
    if let Some((faze, minuty)) = dalsi {
        let (nadpis, text) = match faze {
            Faze::Prestavka => ("Time for a break".to_string(), format!("Work session done. Break: {minuty} min.")),
            Faze::Prace => ("Break is over".to_string(), format!("Back to work: {minuty} min.")),
        };
        pozadi("notify-send", &["-i", "alarm-symbolic", &nadpis, &text]);
    }
    do_panelu();
    STAV.with(|s| s.borrow().is_some())
}

pub fn odpocet(d: Duration) -> String {
    let s = d.as_secs() + u64::from(d.subsec_nanos() > 0);
    format!("{}:{:02}", s / 60, s % 60)
}

/// Panel: odpočet, jen když pomodoro běží (klik = okénko notifikací s ovládáním).
fn do_panelu() {
    match info() {
        Some((faze, zbyva, pozastaveno)) => {
            let trida = if pozastaveno {
                "pozastaveno"
            } else if faze == Faze::Prace {
                "prace"
            } else {
                "prestavka"
            };
            let ikona = if faze == Faze::Prace { "\u{f2f2}" } else { "\u{f0f4}" };
            publikovat("pomodoro", json!({"text": format!("{ikona}  {}", odpocet(zbyva)), "class": trida}));
        }
        None => publikovat("pomodoro", json!({"text": ""})),
    }
}

pub fn spustit_panel() {
    do_panelu();
}
