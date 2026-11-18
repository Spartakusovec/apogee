//! Dok dole na obrazovce: připnuté a spuštěné aplikace (tečky = počet oken) a tlačítko
//! s mřížkou aplikací. Dřív upravený nwg-dock (~/.local/src/nwg-dock-ih).
//!
//! Klik: spustit / přepnout na okno aplikace (další klik přepíná mezi jejími okny),
//! prostřední klik: nové okno, pravý klik: nabídka (okna, nové okno, připnout, zavřít).
//!
//! Chytré schovávání: dok je vidět, dokud ho nepřekrývá okno na aktuální ploše. Schovaný se ukáže,
//! když myš chvilku podrží u spodního okraje obrazovky (funguje i nad oknem přes celou obrazovku).
//! Neschová se, dokud je nad ním myš nebo otevřená nabídka. Super+X ho ukáže / schová.
//! Připnuté aplikace: ~/.config/apogee/settings.json („dok“).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use swayipc::{Connection, Node, NodeType};

use crate::spolecne::*;

const IKONA: i32 = 40;
/// mezera mezi dokem a spodním okrajem obrazovky
const OKRAJ: i32 = 8;

#[derive(Clone)]
struct Okno {
    id: i64,
    titulek: String,
    aktivni: bool,
    /// minimalizované (ve scratchpadu Swaye)
    skryte: bool,
}

#[derive(Clone)]
struct Polozka {
    /// název .desktop souboru bez přípony (org.mozilla.firefox), u neznámých aplikací app_id
    klic: String,
    aplikace: Option<gio::AppInfo>,
    /// od naposledy použitého
    okna: Vec<Okno>,
}

struct Dok {
    okno: gtk::Window,
    rada: gtk::Box,
    /// proužek u spodního okraje, když je dok schovaný (pokaždé nový, viz ukazat_zonu)
    horka_zona: RefCell<Option<gtk::Window>>,
    /// žádné okno dok nepřekrývá
    volno: Cell<bool>,
    mys_v_doku: Cell<bool>,
    nabidka: Cell<bool>,
    /// ukázaný ručně (myš u okraje, Super+X), i když ho okno překrývá
    rucne: Cell<bool>,
    schovat: RefCell<Option<glib::SourceId>>,
    obnoveni: RefCell<Option<glib::SourceId>>,
    /// klíče položek v doku (celý se přestaví, jen když se změní; jinak se jen přebarví tečky)
    stav: RefCell<Vec<String>>,
    /// aktuální položky (klik a nabídka se podívají sem, ne do stavu z doby vytvoření tlačítka)
    polozky: RefCell<Vec<Polozka>>,
    /// tlačítko a tečky každé položky
    tlacitka: RefCell<HashMap<String, (gtk::Button, gtk::Box)>>,
    /// aplikace podle klíče (gio::AppInfo::all() je pomalé volat při každé události)
    aplikace: RefCell<HashMap<String, Option<gio::AppInfo>>>,
    /// app_id okna → klíč položky
    klice: RefCell<HashMap<String, String>>,
}

thread_local! {
    static DOK: RefCell<Option<Rc<Dok>>> = const { RefCell::new(None) };
}

/// Upozornění z jiných vláken (události Swaye): něco se změnilo, obnovit dok.
static UPOZORNENI: OnceLock<async_channel::Sender<()>> = OnceLock::new();

pub fn upozornit() {
    if let Some(s) = UPOZORNENI.get() {
        let _ = s.try_send(());
    }
}

fn dok() -> Option<Rc<Dok>> {
    DOK.with(|d| d.borrow().clone())
}

pub fn pripnute() -> Vec<String> {
    match nastaveni("dok").as_array() {
        Some(a) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        // poprvé: běžné aplikace (a hned uložit, ať se seznam nemění)
        None => {
            let vychozi: Vec<String> = ["org.mozilla.firefox", "org.gnome.Nautilus", "org.gnome.Ptyxis"].map(String::from).to_vec();
            ulozit_pripnute(&vychozi);
            vychozi
        }
    }
}

fn ulozit_pripnute(seznam: &[String]) {
    ulozit_nastaveni("dok", serde_json::json!(seznam));
}

/// Nové pořadí / seznam připnutých z aplikace Settings.
pub fn nastavit_pripnute(seznam: &[String]) {
    ulozit_pripnute(seznam);
    if let Some(d) = dok() {
        d.stav.replace(Vec::new());
        d.obnovit();
    }
}

/// Schovávání doku: "chytre" (když překáží oknu, výchozí), "nikdy", "vzdy".
pub fn skryvani() -> String {
    nastaveni("dok_skryvani").as_str().unwrap_or("chytre").to_string()
}

pub fn nastavit_skryvani(rezim: &str) {
    ulozit_nastaveni("dok_skryvani", serde_json::json!(rezim));
    if let Some(d) = dok() {
        d.obnovit();
    }
}

/// Název a ikona připnuté aplikace (pro Settings).
pub fn info_aplikace(klic: &str) -> Option<gio::AppInfo> {
    aplikace(klic)
}

fn aplikace(klic: &str) -> Option<gio::AppInfo> {
    // seznam aplikací si gio drží v paměti a hlídá, tohle je rychlé
    let id = format!("{klic}.desktop");
    gio::AppInfo::all().into_iter().find(|a| a.id().is_some_and(|i| i == id))
}

/// Proužek u spodního okraje: myš na něm chvilku = ukázat schovaný dok.
fn vytvorit_horkou_zonu() -> gtk::Window {
    let okno = gtk::Window::new();
    okno.add_css_class("apogee");
    okno.init_layer_shell();
    okno.set_layer(Layer::Overlay);
    okno.set_keyboard_mode(KeyboardMode::None);
    okno.set_namespace(Some("dok-zona"));
    okno.set_exclusive_zone(-1);
    for k in [Edge::Bottom, Edge::Left, Edge::Right] {
        okno.set_anchor(k, true);
    }
    let vypln = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    vypln.set_size_request(-1, 2);
    // úplně průhledný proužek myš nezachytí
    vypln.add_css_class("prouzek");
    okno.set_child(Some(&vypln));
    let mys = gtk::EventControllerMotion::new();
    let cekani: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let c = cekani.clone();
    mys.connect_enter(move |_, _, _| {
        let id = glib::timeout_add_local_once(Duration::from_millis(150), {
            let c = c.clone();
            move || {
                c.replace(None);
                if let Some(d) = dok() {
                    d.rucne.set(true);
                    d.ukazat();
                    // když na dok nenajede, zase schovat
                    d.naplanovat_schovani(2500);
                }
            }
        });
        if let Some(stare) = c.replace(Some(id)) {
            stare.remove();
        }
    });
    mys.connect_leave(move |_| {
        if let Some(id) = cekani.take() {
            id.remove();
        }
    });
    okno.add_controller(mys);
    okno
}

pub fn spustit() {
    crate::styl::pouzit();
    let okno = gtk::Window::new();
    okno.add_css_class("apogee");
    okno.init_layer_shell();
    okno.set_layer(Layer::Overlay);
    okno.set_keyboard_mode(KeyboardMode::None);
    okno.set_namespace(Some("dok"));
    okno.set_anchor(Edge::Bottom, true);
    okno.set_margin(Edge::Bottom, OKRAJ);
    let rada = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    rada.set_widget_name("dok");
    okno.set_child(Some(&rada));

    let d = Rc::new(Dok {
        okno: okno.clone(),
        rada,
        horka_zona: RefCell::new(None),
        volno: Cell::new(true),
        mys_v_doku: Cell::new(false),
        nabidka: Cell::new(false),
        rucne: Cell::new(false),
        schovat: RefCell::new(None),
        obnoveni: RefCell::new(None),
        stav: RefCell::new(Vec::new()),
        polozky: RefCell::new(Vec::new()),
        tlacitka: RefCell::new(HashMap::new()),
        aplikace: RefCell::new(HashMap::new()),
        klice: RefCell::new(HashMap::new()),
    });

    let mys = gtk::EventControllerMotion::new();
    mys.connect_enter(|_, _, _| {
        if let Some(d) = dok() {
            d.mys_v_doku.set(true);
            d.zrusit_schovani();
        }
    });
    mys.connect_leave(|_| {
        if let Some(d) = dok() {
            d.mys_v_doku.set(false);
            d.naplanovat_schovani(1000);
        }
    });
    okno.add_controller(mys);
    DOK.with(|x| x.replace(Some(d.clone())));

    let (tx, rx) = async_channel::bounded::<()>(1);
    let _ = UPOZORNENI.set(tx);
    glib::spawn_future_local(async move {
        while rx.recv().await.is_ok() {
            if let Some(d) = dok() {
                d.naplanovat_obnoveni();
            }
        }
    });
    d.obnovit();
    d.okno.present();
}

/// Super+X, gesto třemi prsty nahoru: ukázat / schovat.
pub fn prepnout() {
    let Some(d) = dok() else { return };
    if d.okno.is_visible() {
        d.rucne.set(false);
        d.zrusit_schovani();
        d.okno.set_visible(false);
        d.ukazat_zonu(true);
    } else {
        d.rucne.set(true);
        d.ukazat();
        d.naplanovat_schovani(2500);
    }
}

impl Dok {
    fn ukazat(&self) {
        self.zrusit_schovani();
        if !self.okno.is_visible() {
            self.okno.set_visible(true);
        }
        self.ukazat_zonu(false);
    }

    /// Proužek se pokaždé vytvoří nový a pak zruší (ne jen schová): když se schová pod kurzorem,
    /// GTK se nedozví, že kurzor odjel, a příště jeho příjezd nenahlásí (dok by vyjel jen napodruhé).
    fn ukazat_zonu(&self, ukazat: bool) {
        let mut z = self.horka_zona.borrow_mut();
        if ukazat {
            if z.is_none() {
                let okno = vytvorit_horkou_zonu();
                okno.present();
                *z = Some(okno);
            }
        } else if let Some(okno) = z.take() {
            okno.destroy();
        }
    }

    fn zrusit_schovani(&self) {
        if let Some(id) = self.schovat.take() {
            id.remove();
        }
    }

    /// Schovat za chvíli, pokud dok pořád něco překrývá a není nad ním myš ani nabídka.
    fn naplanovat_schovani(&self, ms: u64) {
        self.zrusit_schovani();
        let id = glib::timeout_add_local_once(Duration::from_millis(ms), || {
            if let Some(d) = dok() {
                d.schovat.replace(None);
                d.rucne.set(false);
                d.schovat_kdyz_prekazi();
            }
        });
        self.schovat.replace(Some(id));
    }

    fn schovat_kdyz_prekazi(&self) {
        if self.volno.get() || self.mys_v_doku.get() || self.nabidka.get() || self.rucne.get() {
            return;
        }
        self.okno.set_visible(false);
        self.ukazat_zonu(true);
    }

    /// Víc událostí Swaye za sebou (otevření okna = několik událostí) → obnovit jednou.
    fn naplanovat_obnoveni(&self) {
        if self.obnoveni.borrow().is_some() {
            return;
        }
        let id = glib::timeout_add_local_once(Duration::from_millis(40), || {
            if let Some(d) = dok() {
                d.obnoveni.replace(None);
                d.obnovit();
            }
        });
        self.obnoveni.replace(Some(id));
    }

    fn klic_okna(&self, app_id: &str) -> String {
        if let Some(k) = self.klice.borrow().get(app_id) {
            return k.clone();
        }
        let k = crate::aplikace::cesta(app_id)
            .and_then(|c| c.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| app_id.to_string());
        self.klice.borrow_mut().insert(app_id.to_string(), k.clone());
        k
    }

    /// Načte okna ze Swaye, překreslí dok (když se změnil) a podle oken ho ukáže / schová.
    fn obnovit(self: &Rc<Self>) {
        let Ok(mut sway) = Connection::new() else { return };
        let (Ok(strom), Ok(plochy), Ok(vystupy)) = (sway.get_tree(), sway.get_workspaces(), sway.get_outputs()) else { return };

        // okna podle aplikací, od naposledy použitého
        let mut okna: Vec<(String, Okno, i64)> = Vec::new();
        fn projdi(n: &Node, v: &mut Vec<(String, Okno, i64)>, skryte: bool) {
            // i minimalizovaná okna (scratchpad): klik v doku je vrátí
            let skryte = skryte || (n.node_type == NodeType::Workspace && n.name.as_deref() == Some("__i3_scratch"));
            if n.pid.is_some() && matches!(n.node_type, NodeType::Con | NodeType::FloatingCon) {
                let app = n.app_id.clone().or_else(|| n.window_properties.as_ref().and_then(|w| w.class.clone())).unwrap_or_default();
                v.push((app, Okno { id: n.id, titulek: n.name.clone().unwrap_or_default(), aktivni: n.focused, skryte }, n.id));
            }
            for d in n.nodes.iter().chain(n.floating_nodes.iter()) {
                projdi(d, v, skryte);
            }
        }
        projdi(&strom, &mut okna, false);
        let poradi = crate::sway::poradi();
        let mut podle_aplikace: Vec<(String, Vec<(Okno, i64)>)> = Vec::new();
        okna.sort_by_key(|(_, _, id)| *id); // spuštěné nepřipnuté v pořadí, jak se otevřely
        for (app, o, id) in okna {
            let klic = self.klic_okna(&app);
            match podle_aplikace.iter_mut().find(|(k, _)| *k == klic) {
                Some((_, v)) => v.push((o, id)),
                None => podle_aplikace.push((klic, vec![(o, id)])),
            }
        }
        let mut polozky: Vec<Polozka> = Vec::new();
        let pripnute = pripnute();
        for klic in pripnute.iter().chain(podle_aplikace.iter().map(|(k, _)| k).filter(|k| !pripnute.contains(k))) {
            let mut o: Vec<Okno> = podle_aplikace.iter().find(|(k, _)| k == klic).map(|(_, v)| v.iter().map(|(o, _)| o.clone()).collect()).unwrap_or_default();
            o.sort_by_key(|x| poradi.iter().position(|&id| id == x.id).unwrap_or(usize::MAX));
            if polozky.iter().any(|p| &p.klic == klic) {
                continue;
            }
            let app = self.aplikace.borrow_mut().entry(klic.clone()).or_insert_with(|| aplikace(klic)).clone();
            polozky.push(Polozka { klic: klic.clone(), aplikace: app, okna: o });
        }

        let klice: Vec<String> = polozky.iter().map(|p| p.klic.clone()).collect();
        if *self.stav.borrow() != klice {
            self.prestavet(&polozky);
            self.stav.replace(klice);
        } else {
            // jen přebarvit: aktivní aplikace a počet oken
            for p in &polozky {
                if let Some((b, tecky)) = self.tlacitka.borrow().get(&p.klic) {
                    let aktivni = p.okna.iter().any(|o| o.aktivni);
                    if aktivni != b.has_css_class("aktivni") {
                        if aktivni {
                            b.add_css_class("aktivni");
                        } else {
                            b.remove_css_class("aktivni");
                        }
                    }
                    nastavit_tecky(tecky, &p.okna);
                }
            }
        }
        self.polozky.replace(polozky);

        // překrývá dok nějaké okno na ploše, která je pod ním?
        let Some(plocha) = plochy.iter().find(|p| p.focused) else { return };
        let Some(vystup) = vystupy.iter().find(|o| o.name == plocha.output) else { return };
        let (sirka, vyska) = (self.okno.width().max(1), self.okno.height().max(1));
        let (sirka, vyska) = if self.okno.is_visible() { (sirka, vyska) } else { (self.rada.preferred_size().1.width(), self.rada.preferred_size().1.height()) };
        let r = &vystup.rect;
        let oblast = (r.x + (r.width - sirka) / 2, r.y + r.height - vyska - OKRAJ, sirka, vyska);
        let ws = plochy.iter().filter(|p| p.output == plocha.output && p.visible).map(|p| p.name.clone()).collect::<Vec<_>>();
        fn prekryva(n: &Node, oblast: (i32, i32, i32, i32)) -> bool {
            let r = &n.rect;
            if n.pid.is_some() && n.visible != Some(false) && r.x < oblast.0 + oblast.2 && r.x + r.width > oblast.0 && r.y < oblast.1 + oblast.3 && r.y + r.height > oblast.1 {
                return true;
            }
            n.nodes.iter().chain(n.floating_nodes.iter()).any(|d| prekryva(d, oblast))
        }
        fn plochy_uzlu<'a>(n: &'a Node, jmena: &[String], v: &mut Vec<&'a Node>) {
            if n.node_type == NodeType::Workspace && n.name.as_ref().is_some_and(|j| jmena.contains(j)) {
                v.push(n);
                return;
            }
            for d in &n.nodes {
                plochy_uzlu(d, jmena, v);
            }
        }
        let mut uzly = Vec::new();
        plochy_uzlu(&strom, &ws, &mut uzly);
        let prekryto = match skryvani().as_str() {
            "nikdy" => false,
            "vzdy" => true,
            _ => uzly.iter().any(|w| prekryva(w, oblast)),
        };
        self.volno.set(!prekryto);
        if prekryto {
            self.schovat_kdyz_prekazi();
        } else {
            self.ukazat();
        }
    }

    fn prestavet(self: &Rc<Self>, polozky: &[Polozka]) {
        while let Some(d) = self.rada.first_child() {
            self.rada.remove(&d);
        }
        self.tlacitka.borrow_mut().clear();
        for p in polozky {
            self.rada.append(&self.tlacitko(p));
        }
        // mřížka aplikací
        let mrizka = gtk::Button::new();
        mrizka.add_css_class("dok-polozka");
        let obsah = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // 3×3 zaoblené čtverečky, prostřední v barvě akcentu (jako dřív v nwg-docku)
        let ctverce = gtk::Grid::new();
        ctverce.add_css_class("dok-mrizka");
        ctverce.set_row_spacing(3);
        ctverce.set_column_spacing(3);
        ctverce.set_halign(gtk::Align::Center);
        ctverce.set_valign(gtk::Align::Center);
        for i in 0..9 {
            let c = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            c.add_css_class("ctverec");
            if i == 4 {
                c.add_css_class("stred");
            }
            ctverce.attach(&c, i % 3, i / 3, 1, 1);
        }
        let misto = gtk::Box::new(gtk::Orientation::Vertical, 0);
        misto.set_size_request(IKONA, IKONA);
        misto.append(&ctverce);
        ctverce.set_vexpand(true);
        obsah.append(&misto);
        // prázdné místo stejně vysoké jako tečky u aplikací, ať je mřížka ve stejné výšce jako ikony
        let misto_tecek = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        misto_tecek.add_css_class("tecky");
        obsah.append(&misto_tecek);
        mrizka.set_child(Some(&obsah));
        mrizka.set_tooltip_text(Some("All apps"));
        mrizka.connect_clicked(|_| crate::spoustec::prepnout());
        self.rada.append(&mrizka);
        // okno si jinak pamatuje dosavadní (větší) šířku: po zavřené aplikaci by zůstal černý pruh
        self.okno.set_default_size(1, 1);
    }

    fn tlacitko(self: &Rc<Self>, p: &Polozka) -> gtk::Button {
        let b = gtk::Button::new();
        b.add_css_class("dok-polozka");
        if p.okna.iter().any(|o| o.aktivni) {
            b.add_css_class("aktivni");
        }
        let obsah = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let obrazek = match p.aplikace.as_ref().and_then(|a| a.icon()) {
            Some(i) => gtk::Image::from_gicon(&i),
            None => gtk::Image::from_icon_name(&crate::aplikace::info(&p.klic).ikona),
        };
        obrazek.set_pixel_size(IKONA);
        obsah.append(&obrazek);
        let tecky = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        tecky.set_halign(gtk::Align::Center);
        tecky.add_css_class("tecky");
        nastavit_tecky(&tecky, &p.okna);
        obsah.append(&tecky);
        self.tlacitka.borrow_mut().insert(p.klic.clone(), (b.clone(), tecky.clone()));
        b.set_child(Some(&obsah));
        let jmeno = p.aplikace.as_ref().map(|a| a.name().to_string()).unwrap_or_else(|| crate::aplikace::info(&p.klic).jmeno);
        b.set_tooltip_text(Some(&jmeno));

        // klik i nabídka vezmou aktuální stav položky (okna se mezitím mohla změnit)
        let klic = p.klic.clone();
        b.connect_clicked(move |_| {
            if let Some(p) = aktualni(&klic) {
                klik(&p);
            }
        });
        let prostredni = gtk::GestureClick::new();
        prostredni.set_button(2);
        let klic = p.klic.clone();
        prostredni.connect_released(move |_, _, _, _| {
            if let Some(p) = aktualni(&klic) {
                spustit_aplikaci(&p);
            }
        });
        b.add_controller(prostredni);
        let pravy = gtk::GestureClick::new();
        pravy.set_button(3);
        let (klic, slaby) = (p.klic.clone(), b.downgrade());
        pravy.connect_released(move |_, _, _, _| {
            if let (Some(b), Some(p)) = (slaby.upgrade(), aktualni(&klic)) {
                let pripnuta = pripnute().contains(&p.klic);
                nabidka(&b, &p, pripnuta);
            }
        });
        b.add_controller(pravy);
        b
    }
}

/// Tečky pod ikonou: jedna za okno (nejvýš 3), minimalizovaná jako kroužek; mění se, jen když je
/// potřeba. Viditelná okna první.
fn nastavit_tecky(tecky: &gtk::Box, okna: &[Okno]) {
    let mut skryte: Vec<bool> = okna.iter().map(|o| o.skryte).collect();
    skryte.sort();
    skryte.truncate(3);
    let mut ted = 0;
    let mut d = tecky.first_child();
    while let Some(w) = d {
        ted += 1;
        d = w.next_sibling();
    }
    while ted < skryte.len() {
        let t = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        t.add_css_class("tecka");
        tecky.append(&t);
        ted += 1;
    }
    while ted > skryte.len() {
        if let Some(w) = tecky.last_child() {
            tecky.remove(&w);
        }
        ted -= 1;
    }
    let mut d = tecky.first_child();
    for s in skryte {
        let Some(w) = d else { break };
        if s != w.has_css_class("skryta") {
            if s {
                w.add_css_class("skryta");
            } else {
                w.remove_css_class("skryta");
            }
        }
        d = w.next_sibling();
    }
}

fn aktualni(klic: &str) -> Option<Polozka> {
    dok()?.polozky.borrow().iter().find(|p| p.klic == klic).cloned()
}

fn spustit_aplikaci(p: &Polozka) {
    if let Some(a) = &p.aplikace {
        let kontext = gtk::gdk::Display::default().map(|d| d.app_launch_context());
        let _ = a.launch(&[], kontext.as_ref());
    }
}

/// Klik (jako ve Windows): nic neběží = spustit; aplikace není aktivní = naposledy použité okno
/// (minimalizované se vrátí); aktivní aplikace s jedním viditelným oknem = minimalizovat, s víc
/// okny = další viditelné okno.
fn klik(p: &Polozka) {
    if p.okna.is_empty() {
        spustit_aplikaci(p);
        return;
    }
    let viditelna: Vec<&Okno> = p.okna.iter().filter(|o| !o.skryte).collect();
    match viditelna.iter().position(|o| o.aktivni) {
        Some(i) if viditelna.len() > 1 => {
            let cil = viditelna[(i + 1) % viditelna.len()];
            crate::sway::prikaz(&format!("[con_id={}] focus", cil.id));
        }
        Some(i) => crate::sway::prikaz(&format!("[con_id={}] minimize", viditelna[i].id)),
        None => crate::sway::prikaz(&format!("[con_id={}] focus", p.okna[0].id)),
    }
}

fn nabidka(tlacitko: &gtk::Button, p: &Polozka, pripnuta: bool) {
    let pop = gtk::Popover::new();
    pop.add_css_class("dok-nabidka");
    pop.set_has_arrow(false);
    pop.set_position(gtk::PositionType::Top);
    let obsah = gtk::Box::new(gtk::Orientation::Vertical, 2);
    obsah.set_widget_name("panel");
    let zavrit = {
        let pop = pop.downgrade();
        move || {
            if let Some(p) = pop.upgrade() {
                p.popdown();
            }
        }
    };
    for o in &p.okna {
        let mut t = o.titulek.clone();
        if t.chars().count() > 40 {
            t = t.chars().take(39).collect::<String>() + "…";
        }
        let (id, z) = (o.id, zavrit.clone());
        obsah.append(&radek("\u{f2d0}", &t, "", o.aktivni, move || {
            z();
            crate::sway::prikaz(&format!("[con_id={id}] focus"));
        }));
    }
    if !p.okna.is_empty() {
        obsah.append(&oddelovac());
    }
    if p.aplikace.is_some() {
        let (polozka, z) = (p.clone(), zavrit.clone());
        let text = if p.okna.is_empty() { "Open" } else { "New window" };
        obsah.append(&radek("\u{f067}", text, "", false, move || {
            z();
            spustit_aplikaci(&polozka);
        }));
        let (klic, z) = (p.klic.clone(), zavrit.clone());
        let (znak, text) = if pripnuta { ("\u{f08d}", "Unpin from dock") } else { ("\u{f08d}", "Pin to dock") };
        obsah.append(&radek(znak, text, "", false, move || {
            z();
            let mut seznam = pripnute();
            if pripnuta {
                seznam.retain(|k| *k != klic);
            } else {
                seznam.push(klic.clone());
            }
            ulozit_pripnute(&seznam);
            if let Some(d) = dok() {
                d.stav.replace(Vec::new());
                d.obnovit();
            }
        }));
    }
    if !p.okna.is_empty() {
        let (okna, z) = (p.okna.clone(), zavrit.clone());
        let text = if p.okna.len() > 1 { "Close all windows" } else { "Close" };
        obsah.append(&radek("\u{f00d}", text, "", false, move || {
            z();
            for o in &okna {
                crate::sway::prikaz(&format!("[con_id={}] kill", o.id));
            }
        }));
    }
    pop.set_child(Some(&obsah));
    pop.set_parent(tlacitko);
    if let Some(d) = dok() {
        d.nabidka.set(true);
    }
    pop.connect_closed(|pop| {
        if let Some(d) = dok() {
            d.nabidka.set(false);
            if !d.mys_v_doku.get() {
                d.naplanovat_schovani(1000);
            }
        }
        // po zavření popover uvolnit (až po dokončení obsluhy)
        let pop = pop.clone();
        glib::idle_add_local_once(move || pop.unparent());
    });
    pop.popup();
}
