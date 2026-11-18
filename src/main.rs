//! apogee-daemon – démon pomocníka: okénka pod horním panelem, přepínač oken Alt+Tab
//! a moduly panelu pro Sway. Příkazy dostává od klienta `apogee` (klient.rs)
//! přes $XDG_RUNTIME_DIR/apogee.sock.

mod aktualizace;
mod aplikace;
mod dok;
mod hesla_site;
mod hudba;
mod lista;
mod mikrofon;
mod nastaveni;
mod okenka;
mod osd;
mod panel;
mod pomodoro;
mod prepinac;
mod spolecne;
mod styl;
mod upozorneni;
mod spoustec;
mod spotlight;
mod sway;
mod zaznam;

use std::io::{BufRead, BufReader};
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::time::Duration;

use gtk::glib;

fn soket() -> PathBuf {
    spolecne::run_dir().join("apogee.sock")
}

fn main() {
    demon();
}

fn demon() {
    // pád démona zapsat do ~/.cache/apogee/errors.log (démona spouští panel, jeho výstup se ztrácí)
    std::panic::set_hook(Box::new(|info| {
        use std::io::Write;
        let soubor = spolecne::domov().join(".cache/apogee/errors.log");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(soubor) {
            let _ = writeln!(f, "{} {info}", chrono::Local::now().format("%F %T"));
        }
        eprintln!("{info}");
    }));
    // jen jeden démon: zámek drží po celou dobu běhu (moduly panelu ho při přihlášení
    // můžou spouštět několikrát najednou)
    let zamek = match std::fs::File::create(spolecne::run_dir().join("apogee.lock")) {
        Ok(z) => z,
        Err(_) => return,
    };
    // --wait (hlídač apogee-watch.sh): počkat, až předchozí démon skončí, a převzít po něm
    let cekat = std::env::args().any(|a| a == "--wait");
    let rezim = if cekat { libc::LOCK_EX } else { libc::LOCK_EX | libc::LOCK_NB };
    if unsafe { libc::flock(zamek.as_raw_fd(), rezim) } != 0 {
        return;
    }
    let _ = std::fs::remove_file(soket());
    let posluchac = match UnixListener::bind(soket()) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("apogee: cannot listen on {}: {e}", soket().display());
            return;
        }
    };
    // Okénka jsou malá: vykreslovat procesorem. Vykreslování přes grafickou kartu načte ovladače
    // OpenGL/Vulkan a démon by po otevření všech okének bral ~150 MB místo ~40 MB.
    let puvodni_renderer = std::env::var_os("GSK_RENDERER");
    let puvodni_disable = std::env::var_os("GDK_DISABLE");
    std::env::set_var("GSK_RENDERER", "cairo");
    std::env::set_var("GDK_DISABLE", "gl,vulkan,dmabuf");
    gtk::init().expect("GTK init");
    // GTK si GDK_DISABLE přečetl při startu (OpenGL a Vulkan už nepoužije, kreslí procesorem).
    // Programy spouštěné démonem (dok, odkazy v okénkách…) ale mají dostat normální prostředí.
    match puvodni_renderer {
        Some(v) => std::env::set_var("GSK_RENDERER", v),
        None => std::env::remove_var("GSK_RENDERER"),
    }
    match puvodni_disable {
        Some(v) => std::env::set_var("GDK_DISABLE", v),
        None => std::env::remove_var("GDK_DISABLE"),
    }

    // příkazy ze soketu → hlavní vlákno GTK; moduly panelu zůstanou připojené
    let (odesilatel, prijemce) = async_channel::unbounded::<String>();
    std::thread::spawn(move || {
        for spojeni in posluchac.incoming().flatten() {
            let mut radek = String::new();
            if BufReader::new(&spojeni).read_line(&mut radek).is_err() {
                continue;
            }
            let radek = radek.trim();
            if let Some(nazev) = radek.strip_prefix("modul ") {
                // zaseknutý modul nesmí zdržet démona
                let _ = spojeni.set_write_timeout(Some(Duration::from_secs(1)));
                panel::prihlasit(nazev, spojeni);
            } else {
                let _ = odesilatel.send_blocking(radek.to_string());
            }
        }
    });

    sway::sledovat();
    aplikace::nacist_na_pozadi();
    upozorneni::spustit();
    // hesla k Wi-Fi, když o ně NetworkManager požádá (místo nm-applet)
    hesla_site::spustit();
    okenka::spustit_na_pozadi();
    dok::spustit();
    // návrat k waybaru: "lista": false v ~/.config/apogee/settings.json (a bar v konfiguraci Swaye)
    if spolecne::nastaveni("lista").as_bool() != Some(false) {
        lista::spustit();
    }
    spoustec::pripravit();
    spotlight::pripravit();
    nastaveni::spustit_po_prihlaseni();

    glib::spawn_future_local(async move {
        while let Ok(prikaz) = prijemce.recv().await {
            let (prikaz, argument) = prikaz.split_once(' ').unwrap_or((&prikaz, ""));
            let (prikaz, argument) = cesky(prikaz, argument);
            let (prikaz, argument) = (prikaz.as_str(), argument.as_str());
            match prikaz {
                "alttab" => prepinac::alttab(1),
                "alttab-zpet" => prepinac::alttab(-1),
                druh => okenka::prikaz(druh, argument),
            }
        }
    });
    let _zamek = zamek;
    glib::MainLoop::new(None, false).run();
}

/// Anglické příkazy (`apogee settings wifi`, `apogee osd brightness-up`…) na vnitřní české názvy;
/// staré české příkazy projdou beze změny.
fn cesky(prikaz: &str, argument: &str) -> (String, String) {
    let p = match prikaz {
        "alttab-back" => "alttab-zpet",
        "notifications" => "notifikace",
        "notifications-changed" => "notifikace-zmena",
        "weather-refresh" => "pocasi-obnovit",
        "music" => "hudba",
        "screenshot-now" => "snimek-hned",
        "recording-stop" => "nahravani-stop",
        "maximize" => "maximalizovat",
        "tiling" => "dlazdice",
        "dock" => "dok",
        "settings" => "nastaveni",
        "launcher" => "spoustec",
        "style" => "styl",
        "record" => "nahravat",
        "check-updates" => "aktualizace-zkontrolovat",
        "window" => "okno",
        "clipboard" => "schranka",
        "power" => "vypnout",
        "calendar" => "kalendar",
        "player" => "prehravac",
        "weather" => "pocasi",
        "wallpapers" => "tapety",
        "updates" => "aktualizace",
        "sound" => "zvuk",
        "brightness" => "jas",
        "battery" => "baterie",
        "screenshot" => "snimek",
        "recording" => "nahravani",
        p => p,
    };
    let a = match (p, argument) {
        ("hudba", "next") => "dalsi",
        ("hudba", "previous") => "predchozi",
        ("snimek-hned", "area") => "oblast",
        ("snimek-hned", "screen") => "obrazovka",
        ("snimek-hned", "window") => "okno",
        ("osd", "brightness-up") => "jas-up",
        ("osd", "brightness-down") => "jas-down",
        ("nastaveni", "network") => "sit",
        ("nastaveni", "sound") => "zvuk",
        ("nastaveni", "display") => "displej",
        ("nastaveni", "keyboard") => "vstup",
        ("nastaveni", "power") => "napajeni",
        ("nastaveni", "appearance") => "vzhled",
        ("nastaveni", "panel") => "panel_dok",
        ("nastaveni", "windows") => "okna",
        ("nastaveni", "notifications") => "oznameni",
        ("nastaveni", "shortcuts") => "zkratky",
        ("nastaveni", "apps") => "aplikace",
        ("nastaveni", "time") => "cas",
        ("nastaveni", "users") => "uzivatel",
        ("nastaveni", "storage") => "uloziste",
        ("nastaveni", "updates") => "aktualizace",
        ("nastaveni", "about") => "o_pocitaci",
        (_, a) => a,
    };
    (p.to_string(), a.to_string())
}
