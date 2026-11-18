//! Společné kousky okének: okno pod panelem, zavírání, stavební prvky a příkazy na pozadí.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

pub fn domov() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

pub fn run_dir() -> PathBuf {
    PathBuf::from(std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()))
}

pub fn skripty() -> PathBuf {
    domov().join(".config/sway/scripts")
}

fn soubor_nastaveni() -> PathBuf {
    domov().join(".config/apogee/settings.json")
}

/// Zapamatované volby (snímky, pomodoro…) v ~/.config/apogee/settings.json.
pub fn nastaveni(klic: &str) -> serde_json::Value {
    std::fs::read_to_string(soubor_nastaveni())
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .map(|v| v[klic].clone())
        .unwrap_or(serde_json::Value::Null)
}

pub fn ulozit_nastaveni(klic: &str, hodnota: serde_json::Value) {
    let soubor = soubor_nastaveni();
    let mut vse = std::fs::read_to_string(&soubor)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    vse[klic] = hodnota;
    if let Some(slozka) = soubor.parent() {
        let _ = std::fs::create_dir_all(slozka);
    }
    let _ = std::fs::write(soubor, serde_json::to_string_pretty(&vse).unwrap_or_default());
}

/// Spustí příkaz a vrátí jeho výstup (oříznutý).
pub fn prikaz(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .stderr(Stdio::null())
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Spustí několik příkazů najednou a vrátí jejich výstupy (čeká se jen na ten nejpomalejší).
pub fn soucasne(prikazy: &[(&str, &[&str])]) -> Vec<String> {
    let deti: Vec<_> = prikazy
        .iter()
        .map(|(program, args)| Command::new(program).args(*args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok())
        .collect();
    deti.into_iter()
        .map(|d| {
            d.and_then(|d| d.wait_with_output().ok())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default()
        })
        .collect()
}

/// Vlastnost přes D-Bus (systémová nebo uživatelská sběrnice): rychlejší než spouštět nmcli,
/// bluetoothctl a spol.
pub fn dbus_vlastnost(systemova: bool, cil: &str, cesta: &str, rozhrani: &str, nazev: &str) -> Option<glib::Variant> {
    let typ = if systemova { gtk::gio::BusType::System } else { gtk::gio::BusType::Session };
    let sbernice = gtk::gio::bus_get_sync(typ, gtk::gio::Cancellable::NONE).ok()?;
    let v = sbernice
        .call_sync(
            Some(cil),
            cesta,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&(rozhrani, nazev).to_variant()),
            None,
            gtk::gio::DBusCallFlags::NONE,
            500,
            gtk::gio::Cancellable::NONE,
        )
        .ok()?;
    v.get::<(glib::Variant,)>().map(|(v,)| v)
}

/// Všechny vlastnosti rozhraní přes D-Bus (prázdné, když to nejde).
pub fn dbus_vse(systemova: bool, cil: &str, cesta: &str, rozhrani: &str) -> std::collections::HashMap<String, glib::Variant> {
    let typ = if systemova { gtk::gio::BusType::System } else { gtk::gio::BusType::Session };
    gtk::gio::bus_get_sync(typ, gtk::gio::Cancellable::NONE)
        .ok()
        .and_then(|s| {
            s.call_sync(
                Some(cil),
                cesta,
                "org.freedesktop.DBus.Properties",
                "GetAll",
                Some(&(rozhrani,).to_variant()),
                None,
                gtk::gio::DBusCallFlags::NONE,
                500,
                gtk::gio::Cancellable::NONE,
            )
            .ok()
        })
        .and_then(|v| v.get::<(std::collections::HashMap<String, glib::Variant>,)>())
        .map(|(m,)| m)
        .unwrap_or_default()
}

/// Baterie podle UPower (ten ji sleduje sám; čtení /sys/class/power_supply trvá až 70 ms).
pub struct Baterie {
    pub procenta: f64,
    /// 1 nabíjí, 2 vybíjí, 4 nabitá, 5 připojená a nenabíjí
    pub stav: u32,
    pub do_vybiti: i64,
    pub do_nabiti: i64,
    /// W
    pub prikon: f64,
    /// zdraví baterie v %
    pub zdravi: f64,
}

pub fn baterie() -> Option<Baterie> {
    let m = dbus_vse(true, "org.freedesktop.UPower", "/org/freedesktop/UPower/devices/battery_BAT0", "org.freedesktop.UPower.Device");
    let d = |k: &str| m.get(k).and_then(|v| v.get::<f64>()).unwrap_or(0.0);
    let x = |k: &str| m.get(k).and_then(|v| v.get::<i64>()).unwrap_or(0);
    Some(Baterie {
        procenta: m.get("Percentage")?.get::<f64>()?,
        stav: m.get("State").and_then(|v| v.get::<u32>()).unwrap_or(0),
        do_vybiti: x("TimeToEmpty"),
        do_nabiti: x("TimeToFull"),
        prikon: d("EnergyRate"),
        zdravi: d("Capacity"),
    })
}

/// Je zapnutá Wi-Fi (NetworkManager)?
pub fn wifi_zapnuta() -> bool {
    dbus_vlastnost(true, "org.freedesktop.NetworkManager", "/org/freedesktop/NetworkManager", "org.freedesktop.NetworkManager", "WirelessEnabled")
        .and_then(|v| v.get::<bool>())
        .unwrap_or(false)
}

/// Je zapnutý Bluetooth (bluez)?
pub fn bluetooth_zapnuty() -> bool {
    dbus_vlastnost(true, "org.bluez", "/org/bluez/hci0", "org.bluez.Adapter1", "Powered").and_then(|v| v.get::<bool>()).unwrap_or(false)
}

/// Režim napájení (power-saver, balanced, performance) přes rozhraní power-profiles (tuned).
pub fn rezim_napajeni() -> String {
    dbus_vlastnost(true, "net.hadess.PowerProfiles", "/net/hadess/PowerProfiles", "net.hadess.PowerProfiles", "ActiveProfile")
        .and_then(|v| v.get::<String>())
        .unwrap_or_default()
}

/// Je zapnutý noční režim? (podle nastavení night-light.sh, wlsunset pak běží)
pub fn nocni_rezim() -> bool {
    std::fs::read_to_string(domov().join(".config/sway/night-light.conf"))
        .map(|t| t.lines().any(|r| r.starts_with("rezim=") && r != "rezim=vypnuto"))
        .unwrap_or(false)
}

/// Spustí příkaz na pozadí a nečeká na něj.
pub fn pozadi(program: &str, args: &[&str]) {
    use std::os::unix::process::CommandExt;
    let _ = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn();
}

/// Pomalou práci (nmcli, bluetoothctl…) udělá mimo GTK a výsledek předá zpátky do GTK.
pub fn na_pozadi<T: Send + 'static>(prace: impl FnOnce() -> T + Send + 'static, hotovo: impl FnOnce(T) + 'static) {
    let uloha = gtk::gio::spawn_blocking(prace);
    glib::spawn_future_local(async move {
        if let Ok(vysledek) = uloha.await {
            hotovo(vysledek);
        }
    });
}

/// Okénko pod panelem.
pub struct Okenko {
    pub okno: gtk::Window,
    pub panel: gtk::Box,
    /// při psaní hesla nezavírat, ani když myš odjede
    pub drzet: Cell<bool>,
    zaviraci: RefCell<Option<glib::SourceId>>,
    stav_lbl: RefCell<Option<gtk::Label>>,
    pri_zavreni: RefCell<Option<Box<dyn Fn()>>>,
    zavreno: Cell<bool>,
    /// pod kterým modulem panelu (střed, šířka obrazovky), když se otevřelo klikem na panel
    kotva: Option<(f64, i32)>,
}

thread_local! {
    /// Kam patří příští okénko: (střed modulu panelu, na který se kliklo, v px od levého okraje
    /// obrazovky; šířka obrazovky). Nastaví panel při kliknutí, spotřebuje první otevřené okénko.
    /// (platí jen chvilku po kliknutí: klik, který okénko zavřel, ji nesmí nechat dalšímu)
    static KOTVA: Cell<Option<(f64, i32, std::time::Instant)>> = const { Cell::new(None) };
}

pub fn nastavit_kotvu(stred: f64, sirka_obrazovky: i32) {
    KOTVA.with(|k| k.set(Some((stred, sirka_obrazovky, std::time::Instant::now()))));
}

/// Okénko vodorovně pod modulem (střed pod středem), ale celé na obrazovce.
fn umistit_pod(okno: &gtk::Window, stred: f64, sirka_obrazovky: i32, sirka: i32) {
    let vlevo = (stred - sirka as f64 / 2.0).round() as i32;
    okno.set_margin(Edge::Left, vlevo.clamp(8, (sirka_obrazovky - sirka - 8).max(8)));
}

pub enum Umisteni {
    /// uprostřed obrazovky (nabídka okna)
    Stred,
    Vpravo,
    Uprostred,
}

impl Okenko {
    pub fn nove(umisteni: Umisteni, sirka: i32, pri_zavreni: impl Fn() + 'static) -> Rc<Okenko> {
        crate::styl::pouzit();
        let okno = gtk::Window::new();
        okno.add_css_class("apogee");
        okno.init_layer_shell();
        okno.set_layer(Layer::Overlay);
        okno.set_anchor(Edge::Top, true);
        // panel má vyhrazený pruh nahoře, odsazení se počítá až pod ním
        okno.set_margin(Edge::Top, 6);
        let kotva = KOTVA
            .with(|k| k.take())
            .filter(|(_, _, kdy)| kdy.elapsed() < std::time::Duration::from_millis(700))
            .map(|(stred, sirka, _)| (stred, sirka));
        match (kotva, umisteni) {
            // otevřené klikem na panel: pod modulem, na který se kliklo
            // (přesnou pozici podle změřené šířky dopočítá ukazat(), ještě před zobrazením)
            (Some((stred, obrazovka)), _) => {
                okno.set_anchor(Edge::Left, true);
                umistit_pod(&okno, stred, obrazovka, sirka + 38);
            }
            (None, Umisteni::Vpravo) => {
                okno.set_anchor(Edge::Right, true);
                okno.set_margin(Edge::Right, 8);
            }
            // bez ukotvení ho Sway dá doprostřed obrazovky
            (None, Umisteni::Stred) => okno.set_anchor(Edge::Top, false),
            (None, Umisteni::Uprostred) => {}
        }
        okno.set_keyboard_mode(KeyboardMode::OnDemand);
        okno.set_namespace(Some("apogee"));

        let panel = gtk::Box::new(gtk::Orientation::Vertical, 10);
        panel.set_widget_name("panel");
        panel.set_size_request(sirka, -1);
        okno.set_child(Some(&panel));

        let o = Rc::new(Okenko {
            okno,
            panel,
            drzet: Cell::new(false),
            zaviraci: RefCell::new(None),
            stav_lbl: RefCell::new(None),
            pri_zavreni: RefCell::new(Some(Box::new(pri_zavreni))),
            zavreno: Cell::new(false),
            kotva,
        });

        // Esc zavře
        let klavesy = gtk::EventControllerKey::new();
        let slaby = Rc::downgrade(&o);
        klavesy.connect_key_pressed(move |_, klavesa, _, _| {
            if klavesa == gtk::gdk::Key::Escape {
                if let Some(o) = slaby.upgrade() {
                    o.zavrit();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        o.okno.add_controller(klavesy);

        // zavřít, když myš odjede (s prodlevou), nebo když se do okénka vůbec nedostane
        let mys = gtk::EventControllerMotion::new();
        let slaby = Rc::downgrade(&o);
        mys.connect_enter(move |_, _, _| {
            if let Some(o) = slaby.upgrade() {
                o.zrusit_zavirani();
            }
        });
        let slaby = Rc::downgrade(&o);
        mys.connect_leave(move |_| {
            if let Some(o) = slaby.upgrade() {
                if !o.drzet.get() {
                    o.zavrit_za(900);
                }
            }
        });
        o.okno.add_controller(mys);
        o.zavrit_za(8000);

        o
    }

    pub fn ukazat(&self) {
        // šířku obsahu změřit před zobrazením, ať okénko neposkočí na místo až po vykreslení
        if let Some((stred, obrazovka)) = self.kotva {
            let (_, sirka, _, _) = self.okno.measure(gtk::Orientation::Horizontal, -1);
            if sirka > 0 {
                umistit_pod(&self.okno, stred, obrazovka, sirka);
            }
        }
        self.okno.present();
    }

    pub fn zavrit(&self) {
        self.zrusit_zavirani();
        if self.zavreno.replace(true) {
            return;
        }
        self.okno.destroy();
        if let Some(f) = self.pri_zavreni.take() {
            f();
        }
        // GTK 4 okno uvolní, až na něj nic neodkazuje, jenže closures tlačítek a ovladačů drží
        // data okénka a ta zase okno. Rozebrat obsah a odebrat ovladače (až po dokončení
        // obsluhy události, která okénko zavřela), jinak by každé otevření zůstalo v paměti.
        let (okno, panel) = (self.okno.clone(), self.panel.clone());
        glib::idle_add_local_once(move || {
            // okno drží widget s fokusem (a ten svou closure zase data okénka)
            gtk::prelude::GtkWindowExt::set_focus(&okno, None::<&gtk::Widget>);
            okno.set_default_widget(None::<&gtk::Widget>);
            rozebrat(panel.upcast_ref());
            odebrat_ovladace(&okno);
            vratit_pamet();
        });
    }

    /// Okénko ovládané klávesnicí (schránka): nezavírat samo, ani když myš odjede.
    pub fn nezavirat_samo(&self) {
        self.zrusit_zavirani();
        self.drzet.set(true);
    }

    /// Okénko už je zavřené (pomalá práce na pozadí ho nemá znovu plnit).
    pub fn zavreno(&self) -> bool {
        self.zavreno.get()
    }

    fn zavrit_za(self: &Rc<Self>, ms: u64) {
        self.zrusit_zavirani();
        let slaby = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(ms), move || {
            if let Some(o) = slaby.upgrade() {
                o.zaviraci.replace(None);
                o.zavrit();
            }
        });
        self.zaviraci.replace(Some(id));
    }

    fn zrusit_zavirani(&self) {
        if let Some(id) = self.zaviraci.take() {
            id.remove();
        }
    }

    /// Okno se po změně obsahu zmenší / zvětší podle něj.
    pub fn prepocitat(&self) {
        self.okno.set_default_size(1, 1);
        self.okno.queue_resize();
    }

    /// Malý řádek se stavem dole (Connecting…, chyba…).
    pub fn stav(&self, text: &str) {
        let mut lbl = self.stav_lbl.borrow_mut();
        if lbl.is_none() {
            let l = popisek("", &["dim"]);
            self.panel.append(&l);
            *lbl = Some(l);
        }
        if let Some(l) = lbl.as_ref() {
            l.set_text(text);
            l.set_visible(true);
        }
    }

    pub fn smazat_obsah(&self) {
        while let Some(dite) = self.panel.first_child() {
            self.panel.remove(&dite);
        }
        self.stav_lbl.replace(None);
    }
}

/// Vyndá z každého Boxu a Gridu jeho děti (od listů nahoru): widgety bez rodiče se uvolní
/// i se svými closures, a tím pustí data okénka, která drží.
fn rozebrat(w: &gtk::Widget) {
    let mut dite = w.first_child();
    while let Some(d) = dite {
        dite = d.next_sibling();
        rozebrat(&d);
    }
    if let Some(b) = w.downcast_ref::<gtk::Box>() {
        while let Some(d) = b.first_child() {
            b.remove(&d);
        }
    } else if let Some(g) = w.downcast_ref::<gtk::Grid>() {
        while let Some(d) = g.first_child() {
            g.remove(&d);
        }
    }
}

/// Odebere oknu ovladače (myš, klávesy…) i s jejich closures.
pub fn odebrat_ovladace(okno: &gtk::Window) {
    let seznam = okno.observe_controllers();
    let ovladace: Vec<gtk::EventController> = (0..seznam.n_items()).filter_map(|i| seznam.item(i).and_then(|c| c.downcast().ok())).collect();
    for c in ovladace {
        okno.remove_controller(&c);
    }
}

/// Po zavření okénka vrátit systému paměť, kterou si glibc nechává pro příště
/// (bez toho démon po otevření pár okének drží desítky MB navíc).
pub fn vratit_pamet() {
    glib::timeout_add_local_once(Duration::from_secs(2), || unsafe {
        libc::malloc_trim(0);
    });
}

// ---------- stavební prvky ----------

pub fn popisek(text: &str, tridy: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.set_xalign(0.0);
    for t in tridy {
        l.add_css_class(t);
    }
    l
}

pub fn ikona(znak: &str, trida: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(znak));
    l.add_css_class(trida);
    l
}

pub fn oddelovac() -> gtk::Separator {
    gtk::Separator::new(gtk::Orientation::Horizontal)
}

pub fn nadpis(text: &str, popis: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
    b.append(&popisek(text, &["nadpis"]));
    if !popis.is_empty() {
        b.append(&popisek(popis, &["dim"]));
    }
    b
}

/// Odkaz dole v okénku (např. „Sound settings…“): spustí program a zavře okénko.
pub fn odkaz(o: &Rc<Okenko>, text: &str, program: &'static str, args: &'static [&'static str]) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("odkaz");
    b.set_halign(gtk::Align::Start);
    let slaby = Rc::downgrade(o);
    b.connect_clicked(move |_| {
        pozadi(program, args);
        if let Some(o) = slaby.upgrade() {
            o.zavrit();
        }
    });
    b
}

/// Odkaz z okénka na stránku aplikace Settings (zavře okénko).
pub fn odkaz_nastaveni(o: &Rc<Okenko>, text: &str, stranka: &'static str) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("odkaz");
    b.set_halign(gtk::Align::Start);
    let slaby = Rc::downgrade(o);
    b.connect_clicked(move |_| {
        if let Some(o) = slaby.upgrade() {
            o.zavrit();
        }
        glib::idle_add_local_once(move || crate::nastaveni::otevrit(stranka));
    });
    b
}

/// Nadpis se spínačem vpravo (Wi-Fi, Bluetooth).
pub fn hlavicka_s_prepinacem(text: &str, zapnuto: bool, pri_zmene: impl Fn(bool) + 'static) -> gtk::Box {
    let radek = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let lbl = popisek(text, &["nadpis"]);
    lbl.set_hexpand(true);
    radek.append(&lbl);
    let sw = gtk::Switch::new();
    sw.set_active(zapnuto);
    sw.set_valign(gtk::Align::Center);
    sw.connect_active_notify(move |s| pri_zmene(s.is_active()));
    radek.append(&sw);
    radek
}

/// Řádek seznamu: ikona, text, šedý text vpravo; klik spustí akci.
pub fn radek(znak: &str, text: &str, vpravo: &str, aktivni: bool, akce: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("radek");
    if aktivni {
        b.add_css_class("aktivni");
    }
    let obsah = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let ik = ikona(znak, "ikona");
    ik.set_size_request(22, -1);
    obsah.append(&ik);
    let t = popisek(text, &[]);
    t.set_hexpand(true);
    t.set_ellipsize(gtk::pango::EllipsizeMode::End);
    obsah.append(&t);
    if !vpravo.is_empty() {
        obsah.append(&popisek(vpravo, &["vpravo"]));
    }
    b.set_child(Some(&obsah));
    b.connect_clicked(move |_| akce());
    b
}

/// Posuvník s ikonou (nebo tlačítkem) vlevo a hodnotou vpravo; zmena se volá s malou prodlevou.
pub fn posuvnik(
    vlevo: &impl IsA<gtk::Widget>,
    hodnota: f64,
    min: f64,
    max: f64,
    format_hodnoty: fn(f64) -> String,
    zmena: impl Fn(f64) + 'static,
) -> (gtk::Box, gtk::Scale) {
    let radek = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    radek.append(vlevo);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, 1.0);
    scale.set_draw_value(false);
    scale.set_value(hodnota);
    scale.set_hexpand(true);
    let hodnota_lbl = popisek(&format_hodnoty(hodnota), &["hodnota"]);
    hodnota_lbl.set_xalign(1.0);
    radek.append(&scale);
    radek.append(&hodnota_lbl);
    let cekani: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let zmena = Rc::new(zmena);
    scale.connect_value_changed(move |s| {
        let v = s.value();
        hodnota_lbl.set_text(&format_hodnoty(v));
        if let Some(id) = cekani.take() {
            id.remove();
        }
        let zmena = zmena.clone();
        let c = cekani.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(40), move || {
            c.replace(None);
            zmena(v);
        });
        cekani.replace(Some(id));
    });
    (radek, scale)
}

pub fn procenta(v: f64) -> String {
    format!("{} %", v.round() as i64)
}

/// Podsvícení obrazovky: první zařízení v /sys/class/backlight (intel_backlight, amdgpu_bl0,
/// acpi_video0…); raw/platformní mají přednost před firmware (doporučení jádra).
pub fn podsviceni() -> Option<String> {
    let mut v: Vec<(u8, String)> = std::fs::read_dir("/sys/class/backlight")
        .ok()?
        .flatten()
        .map(|e| {
            let jmeno = e.file_name().to_string_lossy().to_string();
            let typ = std::fs::read_to_string(e.path().join("type")).unwrap_or_default();
            let poradi = match typ.trim() { "raw" => 0, "platform" => 1, _ => 2 };
            (poradi, jmeno)
        })
        .collect();
    v.sort();
    v.into_iter().next().map(|(_, j)| j)
}

/// Hodnota ze souboru podsvícení (brightness, max_brightness).
pub fn cist_podsviceni(soubor: &str) -> Option<f64> {
    let z = podsviceni()?;
    std::fs::read_to_string(format!("/sys/class/backlight/{z}/{soubor}")).ok()?.trim().parse().ok()
}
