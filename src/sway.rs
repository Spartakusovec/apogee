//! Komunikace se Swayem: pořadí naposledy použitých oken (pro Alt+Tab), seznam oken
//! a moduly panelu, které závisí na Swayi (název aktivní aplikace, stav dlaždic).

use std::sync::Mutex;
use std::time::Duration;

use serde_json::json;
use swayipc::{Connection, Event, EventType, Node, NodeType, WindowChange};

use crate::panel::{escapovat, publikovat};

/// ID oken od naposledy použitého
static PORADI: Mutex<Vec<i64>> = Mutex::new(Vec::new());

/// Aplikace, jejichž nová okna zůstanou plovoucí i na ploše s dlaždicemi (dialogy, malá okna)
const VZDY_PLOVOUCI: [&str; 5] = ["pavucontrol", "blueman", "nm-connection-editor", "Calculator", "lxpolkit"];

/// Na pozadí sleduje okna a plochy: pořadí oken, moduly panelu a nová okna na plochách
/// s dlaždicemi (místo skriptů aktivni-apps.sh, dlazdice-stav.sh a dlazdice-nova-okna.sh).
pub fn sledovat() {
    std::thread::spawn(|| loop {
        let udalosti = Connection::new().and_then(|c| c.subscribe([EventType::Window, EventType::Workspace, EventType::Tick, EventType::Mode, EventType::Input]));
        if let (Ok(udalosti), Ok(mut sway)) = (udalosti, Connection::new()) {
            aplikace_do_panelu(&mut sway);
            dlazdice_do_panelu(&mut sway);
            plochy_do_panelu(&mut sway);
            rozlozeni_do_panelu(&mut sway);
            publikovat("rezim", json!({"text": ""}));
            for udalost in udalosti.flatten() {
                match udalost {
                    Event::Window(w) => {
                        match w.change {
                            WindowChange::Focus => {
                                let mut poradi = PORADI.lock().unwrap();
                                poradi.retain(|&id| id != w.container.id);
                                poradi.insert(0, w.container.id);
                            }
                            WindowChange::Close => PORADI.lock().unwrap().retain(|&id| id != w.container.id),
                            WindowChange::New => nove_okno(&mut sway, &w.container),
                            // titulky se mění pořád, název aplikace ne
                            WindowChange::Title | WindowChange::Mark | WindowChange::Urgent => continue,
                            _ => {}
                        }
                        aplikace_do_panelu(&mut sway);
                        // ikony aplikací u ploch (focus jen přepíná aktivní plochu, ta má vlastní událost)
                        if matches!(w.change, WindowChange::New | WindowChange::Close | WindowChange::Move) {
                            plochy_do_panelu(&mut sway);
                        }
                        crate::dok::upozornit();
                    }
                    // přepnuté rozložení klávesnice (nebo nové z Settings)
                    Event::Input(i) if matches!(i.change, swayipc::InputChange::XkbLayout | swayipc::InputChange::XkbKeymap) => rozlozeni_do_panelu(&mut sway),
                    Event::Mode(m) => {
                        // režim Swaye (resize): vidět, jen když není výchozí
                        let text = if m.change == "default" { String::new() } else { escapovat(&m.change) };
                        publikovat("rezim", json!({"text": text}));
                    }
                    Event::Workspace(_) => {
                        crate::dok::upozornit();
                        plochy_do_panelu(&mut sway);
                        aplikace_do_panelu(&mut sway);
                        dlazdice_do_panelu(&mut sway);
                    }
                    // tlačítko dlaždic přepnulo režim plochy (prepnout_dlazdice)
                    Event::Tick(t) if t.payload == "dlazdice" => dlazdice_do_panelu(&mut sway),
                    // pravý klik na lištu aplikace (GTK): upravený Sway pošle „window-menu“
                    // okno se táhne (upravený Sway, nejvýš 10× za sekundu): dok se podívá, jestli překáží
                    Event::Tick(t) if t.payload == "window-drag" => crate::dok::upozornit(),
                    Event::Tick(t) if t.payload == "window-menu" => {
                        gtk::glib::MainContext::default().invoke(|| crate::okenka::prepnout("okno"));
                    }
                    _ => {}
                }
            }
        }
        // Sway se restartoval nebo spojení spadlo: zkusit znovu
        std::thread::sleep(Duration::from_secs(1));
    });
}

/// Rozložení klávesnice v panelu (jen když je jich víc): zkratka aktivního, třeba „CZ“, „US“.
fn rozlozeni_do_panelu(sway: &mut Connection) {
    let Ok(vstupy) = sway.get_inputs() else { return };
    let Some(k) = vstupy.iter().find(|i| i.input_type == "keyboard" && i.xkb_layout_names.len() > 1) else {
        publikovat("rozlozeni", json!({"text": ""}));
        return;
    };
    let i = k.xkb_active_layout_index.unwrap_or(0).max(0) as usize;
    let kody = crate::spolecne::nastaveni("rozlozeni");
    let kod = kody[i][0].as_str().map(|s| s.to_uppercase()).unwrap_or_else(|| k.xkb_layout_names.get(i).map(|n| n.chars().take(2).collect::<String>().to_uppercase()).unwrap_or_default());
    let nazev = k.xkb_layout_names.get(i).cloned().unwrap_or_default();
    publikovat("rozlozeni", json!({"text": format!("\u{f11c}  {kod}"), "tooltip": escapovat(&nazev)}));
}

fn app_id(uzel: &Node) -> String {
    uzel.app_id
        .clone()
        .or_else(|| uzel.window_properties.as_ref().and_then(|w| w.class.clone()))
        .unwrap_or_default()
}

fn najit<'a>(uzel: &'a Node, podminka: &dyn Fn(&Node) -> bool) -> Option<&'a Node> {
    if podminka(uzel) {
        return Some(uzel);
    }
    uzel.nodes.iter().chain(uzel.floating_nodes.iter()).find_map(|d| najit(d, podminka))
}

/// Název aktivní aplikace vlevo v panelu, jako na Macu (klik = nabídka okna).
/// Na prázdné ploše se schová.
fn aplikace_do_panelu(sway: &mut Connection) {
    let Ok(strom) = sway.get_tree() else { return };
    match najit(&strom, &|n| n.focused && n.pid.is_some()) {
        Some(okno) => {
            let jmeno = crate::aplikace::info(&app_id(okno)).jmeno;
            publikovat(
                "aplikace",
                json!({
                    "text": format!("{}  <span size=\"smaller\" alpha=\"55%\">\u{f078}</span>", escapovat(&jmeno)),
                }),
            );
        }
        None => publikovat("aplikace", json!({"text": ""})),
    }
}

/// Ikona aplikace u čísla plochy v panelu (Font Awesome)
fn ikona_na_plose(app: &str) -> &'static str {
    let a = app.to_lowercase();
    let je = |slova: &[&str]| slova.iter().any(|s| a.contains(s));
    if je(&["firefox"]) {
        "\u{f269}"
    } else if je(&["brave", "chrom"]) {
        "\u{f268}"
    } else if je(&["discord", "vesktop"]) {
        "\u{f392}"
    } else if je(&["spotify"]) {
        "\u{f1bc}"
    } else if je(&["telegram"]) {
        "\u{f2c6}"
    } else if je(&["stremio", "vlc", "player", "mpv", "celluloid"]) {
        "\u{f008}"
    } else if je(&["ptyxis", "terminal", "kitty", "alacritty", "foot"]) {
        "\u{f120}"
    } else if je(&["nautilus", "cosmic-files", "files"]) {
        "\u{f07b}"
    } else if je(&["obsidian"]) {
        "\u{f249}"
    } else if je(&["code", "zed"]) {
        "\u{f121}"
    } else if je(&["onlyoffice", "papers", "inkscape", "gimp", "libreoffice"]) {
        "\u{f15b}"
    } else if je(&["minecraft"]) {
        "\u{f11b}"
    } else if je(&["anki"]) {
        "\u{f02d}"
    } else {
        "\u{f2d0}"
    }
}

/// Plochy, jak jsou v panelu: 1–4 vždycky, další jen když existují, seřazené podle čísla.
fn jmena_ploch(plochy: &[swayipc::Workspace]) -> Vec<String> {
    let mut jmena: Vec<String> = (1..=4).map(|n| n.to_string()).collect();
    for p in plochy {
        if !jmena.contains(&p.name) {
            jmena.push(p.name.clone());
        }
    }
    let poradi = |j: &str| j.parse::<i64>().unwrap_or(i64::MAX);
    jmena.sort_by_key(|j| (poradi(j), j.clone()));
    jmena
}

/// Kolečko nad plochami: sousední plocha z panelu (i prázdná 1–4), bez přetočení dokola.
/// (`workspace next` Swaye zná jen existující plochy, s jedinou otevřenou by nedělal nic.)
pub fn plocha_vedle(dalsi: bool) {
    let Ok(mut sway) = Connection::new() else { return };
    let Ok(plochy) = sway.get_workspaces() else { return };
    let Some(ted) = plochy.iter().find(|p| p.focused) else { return };
    let jmena = jmena_ploch(&plochy);
    let Some(i) = jmena.iter().position(|j| *j == ted.name) else { return };
    let cil = if dalsi { jmena.get(i + 1) } else { i.checked_sub(1).and_then(|i| jmena.get(i)) };
    if let Some(cil) = cil {
        let _ = sway.run_command(format!("workspace \"{}\"", cil.replace('"', "\\\"")));
    }
}

/// Plochy v panelu: 1–4 vždycky, další jen když existují; u každé ikony jejích oken.
fn plochy_do_panelu(sway: &mut Connection) {
    let (Ok(strom), Ok(plochy)) = (sway.get_tree(), sway.get_workspaces()) else { return };
    let mut uzly: Vec<&Node> = Vec::new();
    fn najdi<'a>(n: &'a Node, v: &mut Vec<&'a Node>) {
        if n.node_type == NodeType::Workspace {
            v.push(n);
            return;
        }
        for d in &n.nodes {
            najdi(d, v);
        }
    }
    najdi(&strom, &mut uzly);
    let mut seznam: Vec<serde_json::Value> = Vec::new();
    for jmeno in jmena_ploch(&plochy) {
        let p = plochy.iter().find(|p| p.name == jmeno);
        let mut ikony: Vec<&str> = Vec::new();
        if let Some(uzel) = uzly.iter().find(|u| u.name.as_deref() == Some(jmeno.as_str())) {
            fn okna<'a>(n: &'a Node, v: &mut Vec<String>) {
                if n.pid.is_some() {
                    v.push(n.app_id.clone().or_else(|| n.window_properties.as_ref().and_then(|w| w.class.clone())).unwrap_or_default());
                }
                for d in n.nodes.iter().chain(n.floating_nodes.iter()) {
                    okna(d, v);
                }
            }
            let mut v = Vec::new();
            okna(uzel, &mut v);
            ikony = v.iter().map(|a| ikona_na_plose(a)).collect();
        }
        seznam.push(json!({
            "jmeno": jmeno,
            "ikony": ikony.join(" "),
            "aktivni": p.is_some_and(|p| p.focused),
            "videt": p.is_some_and(|p| p.visible),
            "nalehava": p.is_some_and(|p| p.urgent),
            "prazdna": ikony.is_empty(),
        }));
    }
    publikovat("plochy", json!({"plochy": seznam}));
}

/// Nové plochy v dlaždicích (Settings → Windows & Workspaces), jinak plovoucí okna.
pub fn vychozi_dlazdice() -> bool {
    crate::spolecne::nastaveni("dlazdice_vychozi").as_bool().unwrap_or(false)
}

/// Plocha je v dlaždicích: výchozí stav, pokud ji uživatel tlačítkem v panelu nepřepnul
/// (výjimka je značka v run_dir: sway-dlazdice-N, nebo sway-plovouci-N při výchozích dlaždicích).
fn plocha_dlazdice(plocha: &str) -> bool {
    let d = crate::spolecne::run_dir();
    if vychozi_dlazdice() {
        !d.join(format!("sway-plovouci-{plocha}")).exists()
    } else {
        d.join(format!("sway-dlazdice-{plocha}")).exists()
    }
}

fn zapsat_dlazdice(plocha: &str, dlazdice: bool) {
    let d = crate::spolecne::run_dir();
    let (vyjimka, zapnout) = if vychozi_dlazdice() { (format!("sway-plovouci-{plocha}"), !dlazdice) } else { (format!("sway-dlazdice-{plocha}"), dlazdice) };
    if zapnout {
        let _ = std::fs::write(d.join(vyjimka), "");
    } else {
        let _ = std::fs::remove_file(d.join(vyjimka));
    }
}

/// Tlačítko dlaždic v panelu: dlaždice = zvýrazněná mřížka, plovoucí okna = ztlumená ikona.
fn dlazdice_do_panelu(sway: &mut Connection) {
    let Ok(plochy) = sway.get_workspaces() else { return };
    let Some(plocha) = plochy.iter().find(|p| p.focused) else { return };
    let p = &plocha.name;
    publikovat(
        "dlazdice",
        if plocha_dlazdice(p) {
            json!({"text": "\u{f009}", "class": "dlazdice", "tooltip": format!("Workspace {p}: tiled\nClick: floating windows")})
        } else {
            json!({"text": "\u{f24d}", "class": "plovouci", "tooltip": format!("Workspace {p}: floating windows\nClick: tile them")})
        },
    );
}

/// Nové okno na ploše v režimu dlaždic se taky zařadí do dlaždic. Výjimky: dialogy a malá okna
/// a druhé okno stejné aplikace (obvykle dialog, třeba výběr souboru) zůstanou plovoucí.
fn nove_okno(sway: &mut Connection, okno: &Node) {
    let Ok(strom) = sway.get_tree() else { return };
    let Some(plocha) = najit(&strom, &|n| n.node_type == NodeType::Workspace && najit(n, &|d| d.id == okno.id).is_some()) else {
        return;
    };
    if !plocha_dlazdice(plocha.name.as_deref().unwrap_or("")) {
        return;
    }
    let app = app_id(okno);
    let role = okno.window_properties.as_ref().and_then(|w| w.window_role.clone()).unwrap_or_default();
    let titulek = okno.name.as_deref().unwrap_or("");
    if VZDY_PLOVOUCI.iter().any(|v| app.contains(v))
        || role == "pop-up"
        || role == "dialog"
        || titulek == "Picture-in-Picture"
        || titulek == "Obraz v obraze"
    {
        return;
    }
    if najit(plocha, &|n| n.pid.is_some() && n.id != okno.id && app_id(n) == app).is_some() {
        return;
    }
    let _ = sway.run_command(format!("[con_id={}] floating disable", okno.id));
}

/// ID oken od naposledy použitého (pro dok: které okno aplikace přepnout).
pub fn poradi() -> Vec<i64> {
    PORADI.lock().unwrap().clone()
}

pub struct Okno {
    pub id: i64,
    pub app: String,
    pub titulek: String,
    pub plocha: String,
    pub aktivni: bool,
}

/// Okna ze všech ploch: aktivní první, pak podle posledního použití.
pub fn okna() -> Vec<Okno> {
    let Ok(strom) = Connection::new().and_then(|mut c| c.get_tree()) else {
        return Vec::new();
    };
    let mut vysledek = Vec::new();
    fn projdi(uzel: &Node, plocha: &str, vysledek: &mut Vec<Okno>) {
        let plocha = if uzel.node_type == NodeType::Workspace {
            // minimalizovaná okna (scratchpad): Alt+Tab je vrátí na jejich plochu
            if uzel.name.as_deref() == Some("__i3_scratch") {
                "minimized".to_string()
            } else {
                uzel.name.clone().unwrap_or_default()
            }
        } else {
            plocha.to_string()
        };
        if uzel.pid.is_some() && matches!(uzel.node_type, NodeType::Con | NodeType::FloatingCon) {
            vysledek.push(Okno {
                id: uzel.id,
                app: uzel
                    .app_id
                    .clone()
                    .or_else(|| uzel.window_properties.as_ref().and_then(|w| w.class.clone()))
                    .unwrap_or_else(|| "?".into()),
                titulek: uzel.name.clone().unwrap_or_default(),
                plocha: plocha.clone(),
                aktivni: uzel.focused,
            });
        }
        for dite in uzel.nodes.iter().chain(uzel.floating_nodes.iter()) {
            projdi(dite, &plocha, vysledek);
        }
    }
    projdi(&strom, "", &mut vysledek);
    let poradi = PORADI.lock().unwrap().clone();
    vysledek.sort_by_key(|o| (!o.aktivni, poradi.iter().position(|&id| id == o.id).unwrap_or(usize::MAX)));
    vysledek
}

pub fn prikaz(prikaz: &str) {
    if let Ok(mut c) = Connection::new() {
        let _ = c.run_command(prikaz);
    }
}

/// Tlačítko dlaždic v panelu: celá aktuální plocha plovoucí okna ↔ dlaždice (nikdy namíchané).
/// Režim plochy se pamatuje, nová okna se pak přidají ve stejném režimu. Dřív plocha-dlazdice.sh.
/// Settings změnila výchozí režim: výjimky zrušit, aktuální plochy přepnout a panel obnovit.
pub fn zmenit_vychozi_dlazdice(dlazdice: bool) {
    crate::spolecne::ulozit_nastaveni("dlazdice_vychozi", json!(dlazdice));
    if let Ok(d) = std::fs::read_dir(crate::spolecne::run_dir()) {
        for e in d.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with("sway-dlazdice-") || n.starts_with("sway-plovouci-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let Ok(mut sway) = Connection::new() else { return };
    let _ = sway.run_command(if dlazdice { "[floating] floating disable" } else { "[tiling] floating enable" });
    let _ = sway.send_tick("dlazdice");
}

pub fn prepnout_dlazdice() {
    let Ok(mut sway) = Connection::new() else { return };
    let Ok(plochy) = sway.get_workspaces() else { return };
    let Some(plocha) = plochy.into_iter().find(|p| p.focused) else { return };
    let n = plocha.name;
    if plocha_dlazdice(&n) {
        zapsat_dlazdice(&n, false);
        let _ = sway.run_command(format!("[workspace=\"^{n}$\" tiling] floating enable"));
        crate::osd::ukazat("\u{f24d}", &format!("Workspace {n}: floating windows"), None);
    } else {
        zapsat_dlazdice(&n, true);
        let _ = sway.run_command(format!("[workspace=\"^{n}$\" floating] floating disable"));
        crate::osd::ukazat("\u{f009}", &format!("Workspace {n}: tiled"), None);
    }
    // ať se tlačítko v panelu hned přebarví (sleduje to vlákno s událostmi Swaye)
    let _ = sway.send_tick("dlazdice");
}
