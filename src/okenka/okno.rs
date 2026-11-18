//! Nabídka aktivního okna (klik na název aplikace v panelu, Super + M, Super + pravý klik):
//! přesun na plochu (u každé je vidět, co na ní je), maximalizace, celá obrazovka,
//! dlaždice / plovoucí, na všech plochách, zavřít. Dřív okno-menu.sh přes fuzzel.

use std::collections::BTreeMap;
use std::rc::Rc;

use gtk::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use swayipc::{Connection, Node, NodeType};

use crate::spolecne::*;

fn app_id(n: &Node) -> String {
    n.app_id
        .clone()
        .or_else(|| n.window_properties.as_ref().and_then(|w| w.class.clone()))
        .unwrap_or_else(|| "?".into())
}

fn okna(n: &Node, v: &mut Vec<String>) {
    if n.pid.is_some() {
        v.push(app_id(n));
    }
    for d in n.nodes.iter().chain(n.floating_nodes.iter()) {
        okna(d, v);
    }
}

/// Plochy (číslo → názvy aplikací na ní) a aktivní okno s plochou, na které je.
fn zjistit() -> Option<(BTreeMap<String, Vec<String>>, Node, String, bool)> {
    let strom = Connection::new().ok()?.get_tree().ok()?;
    let mut plochy = BTreeMap::new();
    let mut aktivni = None;
    fn hledat<'a>(n: &'a Node) -> Option<&'a Node> {
        if n.focused && n.pid.is_some() {
            return Some(n);
        }
        n.nodes.iter().chain(n.floating_nodes.iter()).find_map(hledat)
    }
    for vystup in &strom.nodes {
        for ws in vystup.nodes.iter().filter(|w| w.node_type == NodeType::Workspace) {
            let jmeno = ws.name.clone().unwrap_or_default();
            if jmeno == "__i3_scratch" {
                continue;
            }
            let mut v = Vec::new();
            okna(ws, &mut v);
            let mut nazvy: Vec<String> = v.iter().map(|a| crate::aplikace::info(a).jmeno).collect();
            nazvy.sort();
            nazvy.dedup();
            if let Some(o) = hledat(ws) {
                // maximalizované = i s lištou vyplňuje celou plochu
                let r = &o.rect;
                let d = o.deco_rect.height;
                let max = (r.x, r.y - d, r.width, r.height + d) == (ws.rect.x, ws.rect.y, ws.rect.width, ws.rect.height);
                aktivni = Some((o.clone(), jmeno.clone(), max));
            }
            plochy.insert(jmeno, nazvy);
        }
    }
    let (okno, plocha, max) = aktivni?;
    Some((plochy, okno, plocha, max))
}

pub fn naplnit(o: &Rc<Okenko>) {
    let Some((plochy, okno, plocha, maximalizovane)) = zjistit() else {
        o.stav("No window is active");
        return;
    };
    // ovládání i klávesnicí (Super + M): šipky, Enter, Esc
    o.okno.set_keyboard_mode(KeyboardMode::Exclusive);
    let id = okno.id;
    let plovouci = okno.node_type == NodeType::FloatingCon;
    let mut titulek = okno.name.clone().unwrap_or_default();
    if titulek.chars().count() > 45 {
        titulek = titulek.chars().take(44).collect::<String>() + "…";
    }
    o.panel.append(&nadpis(&crate::aplikace::info(&app_id(&okno)).jmeno, &titulek));

    let slaby = Rc::downgrade(o);
    let akce = move |prikaz: String| {
        let slaby = slaby.clone();
        move || {
            if let Some(o) = slaby.upgrade() {
                o.zavrit();
            }
            crate::sway::prikaz(&format!("[con_id={id}] {prikaz}"));
        }
    };

    // plochy 1–4 vždycky, další jen když existují; čísla řadit číselně
    let mut cisla: Vec<i64> = (1..=4).collect();
    cisla.extend(plochy.keys().filter_map(|k| k.parse::<i64>().ok()).filter(|n| *n > 4));
    cisla.sort();
    cisla.dedup();
    let mut prvni: Option<gtk::Button> = None;
    for n in &cisla {
        let jmeno = n.to_string();
        let obsah = plochy.get(&jmeno).map(|a| a.join(", ")).unwrap_or_default();
        let b = if jmeno == plocha {
            let b = radek("\u{f111}", &format!("Workspace {n}"), "current", true, || {});
            b.set_sensitive(false);
            b
        } else {
            let b = radek("\u{f061}", &format!("Move to workspace {n}"), &obsah, false, akce(format!("move container to workspace number {n}")));
            prvni.get_or_insert(b.clone());
            b
        };
        o.panel.append(&b);
    }
    let volna = cisla.last().copied().unwrap_or(4).max(4) + 1;
    o.panel.append(&radek("\u{f067}", &format!("Move to a new workspace ({volna})"), "", false, akce(format!("move container to workspace number {volna}"))));
    o.panel.append(&oddelovac());

    o.panel.append(&radek("\u{f2d1}", "Minimize", "", false, akce("minimize".into())));
    if plovouci {
        let (znak, text, zkratka) = if maximalizovane { ("\u{f422}", "Restore size", "Super + ↓") } else { ("\u{f424}", "Maximize", "Super + ↑") };
        o.panel.append(&radek(znak, text, zkratka, false, akce("maximize toggle".into())));
    }
    o.panel.append(&radek("\u{f065}", "Fullscreen", "Super + F", false, akce("fullscreen toggle".into())));
    if plovouci {
        o.panel.append(&radek("\u{f009}", "Tile", "", false, akce("floating disable".into())));
        if okno.sticky {
            o.panel.append(&radek("\u{f08d}", "Only on this workspace", "", false, akce("sticky disable".into())));
        } else {
            o.panel.append(&radek("\u{f08d}", "Show on all workspaces", "", false, akce("sticky enable".into())));
        }
    } else {
        o.panel.append(&radek("\u{f24d}", "Float", "", false, akce("floating enable".into())));
    }
    o.panel.append(&radek("\u{f00d}", "Close", "", false, akce("kill".into())));
    if let Some(b) = prvni {
        b.grab_focus();
    }
}
