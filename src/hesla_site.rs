//! Hesla k sítím (NetworkManager SecretAgent, náhrada nm-applet): když síť potřebuje heslo
//! (změnilo se na routeru, uložené chybí, eduroam chce jméno a heslo), NetworkManager se zeptá
//! sem a uprostřed obrazovky se ukáže okénko na heslo. Odpověď se vrací až po vyplnění,
//! zavření okénka = zrušeno. Hesla ukládá NetworkManager sám (systémová připojení).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;
use gtk4_layer_shell::LayerShell;

use crate::spolecne::*;

const CESTA: &str = "/org/freedesktop/NetworkManager/SecretAgent";
const ROZHRANI: &str = "org.freedesktop.NetworkManager.SecretAgent";
const XML: &str = r#"<node>
  <interface name="org.freedesktop.NetworkManager.SecretAgent">
    <method name="GetSecrets">
      <arg name="connection" type="a{sa{sv}}" direction="in"/>
      <arg name="connection_path" type="o" direction="in"/>
      <arg name="setting_name" type="s" direction="in"/>
      <arg name="hints" type="as" direction="in"/>
      <arg name="flags" type="u" direction="in"/>
      <arg name="secrets" type="a{sa{sv}}" direction="out"/>
    </method>
    <method name="CancelGetSecrets">
      <arg name="connection_path" type="o" direction="in"/>
      <arg name="setting_name" type="s" direction="in"/>
    </method>
    <method name="SaveSecrets">
      <arg name="connection" type="a{sa{sv}}" direction="in"/>
      <arg name="connection_path" type="o" direction="in"/>
    </method>
    <method name="DeleteSecrets">
      <arg name="connection" type="a{sa{sv}}" direction="in"/>
      <arg name="connection_path" type="o" direction="in"/>
    </method>
  </interface>
</node>"#;

/// NM_SECRET_AGENT_GET_SECRETS_FLAG_ALLOW_INTERACTION / REQUEST_NEW
const SMI_SE_PTAT: u32 = 0x1;
const ZNOVU: u32 = 0x2;

type Pripojeni = HashMap<String, HashMap<String, Variant>>;

/// Rozpracovaná žádost: (cesta připojení, nastavení, odpověď D-Bus, okénko)
struct Zadost {
    cesta: String,
    volani: gio::DBusMethodInvocation,
    okenko: Rc<Okenko>,
}

thread_local! {
    static ZADOST: RefCell<Option<Zadost>> = const { RefCell::new(None) };
}

pub fn spustit() {
    // APOGEE_AGENT_TEST=1: na sběrnici sezení a bez registrace (systémovou smí volat jen NetworkManager)
    let test = std::env::var_os("APOGEE_AGENT_TEST").is_some();
    let typ = if test { gio::BusType::Session } else { gio::BusType::System };
    let Ok(s) = gio::bus_get_sync(typ, gio::Cancellable::NONE) else { return };
    let Ok(uzel) = gio::DBusNodeInfo::for_xml(XML) else { return };
    let Some(rozhrani) = uzel.lookup_interface(ROZHRANI) else { return };
    let ok = s
        .register_object(CESTA, &rozhrani)
        .method_call(|_, _, _, _, metoda, parametry, volani| match metoda {
            "GetSecrets" => ziskat(&parametry, volani),
            "CancelGetSecrets" => {
                let cesta = parametry.child_value(0).str().unwrap_or("").to_string();
                zrusit(Some(&cesta), "org.freedesktop.NetworkManager.SecretAgent.AgentCanceled");
                volani.return_value(None);
            }
            // hesla patří systémovým připojením, ukládá je NetworkManager sám
            "SaveSecrets" | "DeleteSecrets" => volani.return_value(None),
            _ => volani.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", "Unknown method"),
        })
        .build();
    if ok.is_err() {
        return;
    }
    if test {
        return;
    }
    // zaregistrovat se u NetworkManageru hned a znovu, když se restartuje (NameOwnerChanged;
    // bus_watch_name z gio-rs padá, když se spojení zavře)
    registrovat(&s);
    #[allow(deprecated)]
    let odber = s.signal_subscribe(
        Some("org.freedesktop.DBus"),
        Some("org.freedesktop.DBus"),
        Some("NameOwnerChanged"),
        Some("/org/freedesktop/DBus"),
        Some("org.freedesktop.NetworkManager"),
        gio::DBusSignalFlags::NONE,
        |s, _, _, _, _, parametry| {
            if let Some((_, _, novy)) = parametry.get::<(String, String, String)>() {
                if !novy.is_empty() {
                    registrovat(&s);
                }
            }
        },
    );
    std::mem::forget(odber);
}

fn registrovat(s: &gio::DBusConnection) {
    s.call(
        Some("org.freedesktop.NetworkManager"),
        "/org/freedesktop/NetworkManager/AgentManager",
        "org.freedesktop.NetworkManager.AgentManager",
        "Register",
        Some(&("org.apogee.SecretAgent",).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        5000,
        gio::Cancellable::NONE,
        |v| {
            if let Err(e) = v {
                eprintln!("apogee: NetworkManager secret agent: {e}");
            }
        },
    );
}

/// Zruší rozpracovanou žádost (jinou cestu nechá být) a vrátí NetworkManageru chybu.
fn zrusit(cesta: Option<&str>, chyba: &str) {
    let z = ZADOST.with(|z| {
        let mut z = z.borrow_mut();
        if cesta.is_none() || z.as_ref().map(|x| x.cesta.as_str()) == cesta { z.take() } else { None }
    });
    if let Some(z) = z {
        z.volani.return_dbus_error(chyba, "Canceled");
        z.okenko.zavrit();
    }
}

fn ziskat(parametry: &Variant, volani: gio::DBusMethodInvocation) {
    let Some((pripojeni, cesta, nastaveni, _napovedy, priznaky)) =
        parametry.get::<(Pripojeni, glib::variant::ObjectPath, String, Vec<String>, u32)>()
    else {
        volani.return_dbus_error("org.freedesktop.NetworkManager.SecretAgent.InvalidConnection", "Bad arguments");
        return;
    };
    let nazev = pripojeni.get("connection").and_then(|c| c.get("id")).and_then(|v| v.str().map(String::from)).unwrap_or_default();
    let typ = pripojeni.get("connection").and_then(|c| c.get("type")).and_then(|v| v.str().map(String::from)).unwrap_or_default();
    let podnikova = nastaveni == "802-1x";
    let wifi = nastaveni == "802-11-wireless-security" && typ == "802-11-wireless";
    // bez dovolení se ptát, nebo něco, co neumíme (VPN má vlastní dialogy): nic nemáme
    if priznaky & (SMI_SE_PTAT | ZNOVU) == 0 || !(wifi || podnikova) {
        volani.return_dbus_error("org.freedesktop.NetworkManager.SecretAgent.NoSecrets", "No secrets");
        return;
    }
    // starší žádost (třeba pro jinou síť) vzdát
    zrusit(None, "org.freedesktop.NetworkManager.SecretAgent.AgentCanceled");

    let o = Okenko::nove(Umisteni::Stred, 380, || {
        // zavřené bez odpovědi = zrušeno uživatelem
        zrusit(None, "org.freedesktop.NetworkManager.SecretAgent.UserCanceled");
    });
    o.drzet.set(true);
    o.okno.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);
    let popis = if priznaky & ZNOVU != 0 { "The saved password didn't work. Type the current one." } else { "This network needs a password." };
    o.panel.append(&nadpis(&format!("Wi-Fi: {nazev}"), popis));

    let jmeno = gtk::Entry::new();
    if podnikova {
        jmeno.set_placeholder_text(Some("Username"));
        let ted = pripojeni.get("802-1x").and_then(|c| c.get("identity")).and_then(|v| v.str().map(String::from)).unwrap_or_default();
        jmeno.set_text(&ted);
        o.panel.append(&jmeno);
    }
    let heslo = gtk::PasswordEntry::new();
    heslo.set_show_peek_icon(true);
    heslo.set_property("placeholder-text", "Password");
    o.panel.append(&heslo);

    let radek = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    radek.set_halign(gtk::Align::End);
    let zrusit_b = gtk::Button::with_label("Cancel");
    let pripojit = gtk::Button::with_label("Connect");
    pripojit.add_css_class("zapnuto");
    radek.append(&zrusit_b);
    radek.append(&pripojit);
    o.panel.append(&radek);

    let odeslat = {
        let (heslo, jmeno, nastaveni) = (heslo.clone(), jmeno.clone(), nastaveni.clone());
        move || {
            let h = heslo.text().to_string();
            if h.is_empty() {
                return;
            }
            let mut tajemstvi: HashMap<String, Variant> = HashMap::new();
            if podnikova {
                tajemstvi.insert("password".into(), h.to_variant());
                let j = jmeno.text().to_string();
                if !j.is_empty() {
                    tajemstvi.insert("identity".into(), j.to_variant());
                }
            } else {
                tajemstvi.insert("psk".into(), h.to_variant());
            }
            let mut odpoved: Pripojeni = HashMap::new();
            odpoved.insert(nastaveni.clone(), tajemstvi);
            if let Some(z) = ZADOST.with(|z| z.borrow_mut().take()) {
                z.volani.return_value(Some(&(odpoved,).to_variant()));
                z.okenko.zavrit();
            }
        }
    };
    let o2 = odeslat.clone();
    heslo.connect_activate(move |_| o2());
    pripojit.connect_clicked(move |_| odeslat());
    zrusit_b.connect_clicked(|_| zrusit(None, "org.freedesktop.NetworkManager.SecretAgent.UserCanceled"));

    ZADOST.with(|z| z.replace(Some(Zadost { cesta: cesta.as_str().to_string(), volani, okenko: o.clone() })));
    o.ukazat();
    if podnikova && jmeno.text().is_empty() {
        jmeno.grab_focus();
    } else {
        heslo.grab_focus();
    }
}
