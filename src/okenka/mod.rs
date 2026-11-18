//! Okénka pod panelem. Otevřené je vždy nejvýš jedno: stejný příkaz ho zavře, jiný ho vymění.

mod aktualizace;
mod baterie;
mod bluetooth;
pub mod jas;
mod kalendar;
mod notifikace;
mod okno;
pub mod pocasi;
mod pomodoro;
mod prehravac;
mod schranka;
mod snimek;
pub mod tapety;
mod vypnout;
mod wifi;
pub mod zvuk;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::spolecne::{Okenko, Umisteni};

thread_local! {
    static OTEVRENE: RefCell<Option<(String, u64, Rc<Okenko>)>> = const { RefCell::new(None) };
    static POCITADLO: Cell<u64> = const { Cell::new(0) };
}

/// Co běží na pozadí od startu démona: moduly panelu (zvoneček, počasí, hudba, mikrofon,
/// aktualizace, nahrávání, pomodoro) a hlídání baterie.
pub fn spustit_na_pozadi() {
    crate::hudba::spustit();
    crate::mikrofon::sledovat();
    crate::aktualizace::sledovat();
    crate::zaznam::spustit_panel();
    crate::pomodoro::spustit_panel();
    notifikace::obnovit_zvonek();
    pocasi::sledovat();
    baterie::hlidat();
}

/// Příkaz od klienta, který není přepínač oken.
pub fn prikaz(druh: &str, argument: &str) {
    match druh {
        "notifikace-zmena" => notifikace::obnovit_zvonek(),
        "dnd" => notifikace::nastavit_dnd(match argument {
            "on" => Some(true),
            "off" => Some(false),
            _ => None,
        }),
        "pocasi-obnovit" => pocasi::obnovit(),
        "hudba" => crate::hudba::ovladat(argument),
        // Print Screen: snímek hned, bez okénka
        "snimek-hned" => crate::zaznam::snimek(argument, 0, false),
        "nahravani-stop" => crate::zaznam::zastavit(),
        // klávesy hlasitosti a jasu
        "osd" if argument.starts_with("vol") => crate::osd::hlasitost(argument),
        "osd" => crate::osd::jas(argument),
        // maximalizaci umí upravený Sway (sway-patch); Super+↑/↓ volají jeho příkazy přímo
        "maximalizovat" => crate::sway::prikaz(if argument == "obnovit" { "maximize disable" } else { "maximize enable" }),
        "dlazdice" => crate::sway::prepnout_dlazdice(),
        "dok" => crate::dok::prepnout(),
        // aplikace Settings: `apogee nastaveni [wifi|bluetooth|zvuk|napajeni]`
        "nastaveni" => crate::nastaveni::otevrit(argument),
        "spoustec" => crate::spoustec::prepnout(),
        "spotlight" => crate::spotlight::prepnout(argument),
        // set-wallpaper: nové barvy hned i pro dok
        "styl" => crate::styl::pouzit(),
        "nahravat" => {
            let (rezim, zvuk) = argument.split_once(' ').unwrap_or((argument, "vypnuto"));
            crate::zaznam::nahravat(rezim, zvuk)
        }
        "aktualizace-zkontrolovat" => crate::aktualizace::zkontrolovat(|_| {}),
        _ => prepnout(druh),
    }
}

pub fn prepnout(druh: &str) {
    if let Some((stary, _, okenko)) = OTEVRENE.with(|o| o.borrow_mut().take()) {
        okenko.zavrit();
        if stary == druh {
            return;
        }
    }
    let (umisteni, sirka) = match druh {
        "okno" => (Umisteni::Stred, 380),
        "schranka" => (Umisteni::Stred, 560),
        "vypnout" => (Umisteni::Stred, 260),
        "kalendar" | "prehravac" => (Umisteni::Uprostred, 360),
        "pocasi" | "notifikace" | "tapety" | "aktualizace" => (Umisteni::Vpravo, 400),
        "zvuk" | "jas" | "wifi" | "bluetooth" | "baterie" | "snimek" | "nahravani" => (Umisteni::Vpravo, 360),
        _ => {
            eprintln!("apogee: unknown command {druh}");
            return;
        }
    };
    let id = POCITADLO.with(|p| {
        p.set(p.get() + 1);
        p.get()
    });
    // po zavření (Esc, odjetí myší…) zapomenout, ale jen pokud je to pořád to samé okénko
    let o = Okenko::nove(umisteni, sirka, move || {
        OTEVRENE.with(|o| {
            let mut o = o.borrow_mut();
            if o.as_ref().is_some_and(|(_, i, _)| *i == id) {
                *o = None;
            }
        })
    });
    match druh {
        "zvuk" => zvuk::naplnit(&o),
        "jas" => jas::naplnit(&o),
        "wifi" => wifi::naplnit(&o),
        "bluetooth" => bluetooth::naplnit(&o),
        "baterie" => baterie::naplnit(&o),
        "pocasi" => pocasi::naplnit(&o),
        "kalendar" => kalendar::naplnit(&o),
        "notifikace" => notifikace::naplnit(&o),
        "prehravac" => prehravac::naplnit(&o),
        "okno" => okno::naplnit(&o),
        "schranka" => schranka::naplnit(&o),
        "vypnout" => vypnout::naplnit(&o),
        "snimek" | "nahravani" => snimek::naplnit(&o, druh),
        "tapety" => tapety::naplnit(&o),
        "aktualizace" => aktualizace::naplnit(&o),
        _ => {}
    }
    o.ukazat();
    OTEVRENE.with(|x| x.replace(Some((druh.to_string(), id, o))));
}

/// Zavře otevřené okénko (např. před snímkem obrazovky).
pub fn zavrit() {
    if let Some((_, _, okenko)) = OTEVRENE.with(|o| o.borrow_mut().take()) {
        okenko.zavrit();
    }
}
