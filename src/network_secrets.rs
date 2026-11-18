//! Network passwords (NetworkManager SecretAgent, replaces nm-applet): when a network needs a password
//! (it changed on the router, the saved one is missing, eduroam wants a username and password), NetworkManager asks
//! here and a password dialog appears in the middle of the screen. The reply goes back only once it's filled in;
//! closing the dialog = cancelled. NetworkManager stores the passwords itself (system connections).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;
use gtk4_layer_shell::LayerShell;

use crate::common::*;

const PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";
const INTERFACE: &str = "org.freedesktop.NetworkManager.SecretAgent";
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
const ALLOW_INTERACTION: u32 = 0x1;
const REQUEST_NEW: u32 = 0x2;

type Connection = HashMap<String, HashMap<String, Variant>>;

/// The request being answered: (connection path, D-Bus reply, dialog)
struct Request {
    path: String,
    invocation: gio::DBusMethodInvocation,
    popup: Rc<Popup>,
}

thread_local! {
    static REQUEST: RefCell<Option<Request>> = const { RefCell::new(None) };
}

pub fn start() {
    // APOGEE_AGENT_TEST=1: on the session bus and without registering (only NetworkManager may call it on the system bus)
    let test = std::env::var_os("APOGEE_AGENT_TEST").is_some();
    let typ = if test { gio::BusType::Session } else { gio::BusType::System };
    let Ok(s) = gio::bus_get_sync(typ, gio::Cancellable::NONE) else { return };
    let Ok(node) = gio::DBusNodeInfo::for_xml(XML) else { return };
    let Some(interface) = node.lookup_interface(INTERFACE) else { return };
    let ok = s
        .register_object(PATH, &interface)
        .method_call(|_, _, _, _, method, params, invocation| match method {
            "GetSecrets" => get(&params, invocation),
            "CancelGetSecrets" => {
                let path = params.child_value(0).str().unwrap_or("").to_string();
                cancel(Some(&path), "org.freedesktop.NetworkManager.SecretAgent.AgentCanceled");
                invocation.return_value(None);
            }
            // the passwords belong to system connections, NetworkManager stores them itself
            "SaveSecrets" | "DeleteSecrets" => invocation.return_value(None),
            _ => invocation.return_dbus_error("org.freedesktop.DBus.Error.UnknownMethod", "Unknown method"),
        })
        .build();
    if ok.is_err() {
        return;
    }
    if test {
        return;
    }
    // register with NetworkManager right away and again whenever it restarts (NameOwnerChanged;
    // gio-rs's bus_watch_name panics when the connection closes)
    register(&s);
    #[allow(deprecated)]
    let subscription = s.signal_subscribe(
        Some("org.freedesktop.DBus"),
        Some("org.freedesktop.DBus"),
        Some("NameOwnerChanged"),
        Some("/org/freedesktop/DBus"),
        Some("org.freedesktop.NetworkManager"),
        gio::DBusSignalFlags::NONE,
        |s, _, _, _, _, params| {
            if let Some((_, _, new)) = params.get::<(String, String, String)>() {
                if !new.is_empty() {
                    register(&s);
                }
            }
        },
    );
    std::mem::forget(subscription);
}

fn register(s: &gio::DBusConnection) {
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

/// Cancels the pending request (leaves another path alone) and returns an error to NetworkManager.
fn cancel(path: Option<&str>, error: &str) {
    let z = REQUEST.with(|z| {
        let mut z = z.borrow_mut();
        if path.is_none() || z.as_ref().map(|x| x.path.as_str()) == path { z.take() } else { None }
    });
    if let Some(z) = z {
        z.invocation.return_dbus_error(error, "Canceled");
        z.popup.close();
    }
}

fn get(params: &Variant, invocation: gio::DBusMethodInvocation) {
    let Some((connection, path, settings, _hints, flags)) =
        params.get::<(Connection, glib::variant::ObjectPath, String, Vec<String>, u32)>()
    else {
        invocation.return_dbus_error("org.freedesktop.NetworkManager.SecretAgent.InvalidConnection", "Bad arguments");
        return;
    };
    let name = connection.get("connection").and_then(|c| c.get("id")).and_then(|v| v.str().map(String::from)).unwrap_or_default();
    let typ = connection.get("connection").and_then(|c| c.get("type")).and_then(|v| v.str().map(String::from)).unwrap_or_default();
    let enterprise = settings == "802-1x";
    let wifi = settings == "802-11-wireless-security" && typ == "802-11-wireless";
    // not allowed to ask, or something we can't do (VPNs have their own dialogs): we have nothing
    if flags & (ALLOW_INTERACTION | REQUEST_NEW) == 0 || !(wifi || enterprise) {
        invocation.return_dbus_error("org.freedesktop.NetworkManager.SecretAgent.NoSecrets", "No secrets");
        return;
    }
    // give up an older request (maybe for another network)
    cancel(None, "org.freedesktop.NetworkManager.SecretAgent.AgentCanceled");

    let o = Popup::new(Placement::Center, 380, || {
        // closed without an answer = cancelled by the user
        cancel(None, "org.freedesktop.NetworkManager.SecretAgent.UserCanceled");
    });
    o.keep_open.set(true);
    o.window.set_keyboard_mode(gtk4_layer_shell::KeyboardMode::Exclusive);
    let description = if flags & REQUEST_NEW != 0 { "The saved password didn't work. Type the current one." } else { "This network needs a password." };
    o.panel.append(&title(&format!("Wi-Fi: {name}"), description));

    let name = gtk::Entry::new();
    if enterprise {
        name.set_placeholder_text(Some("Username"));
        let now = connection.get("802-1x").and_then(|c| c.get("identity")).and_then(|v| v.str().map(String::from)).unwrap_or_default();
        name.set_text(&now);
        o.panel.append(&name);
    }
    let password = gtk::PasswordEntry::new();
    password.set_show_peek_icon(true);
    password.set_property("placeholder-text", "Password");
    o.panel.append(&password);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::End);
    let cancel_b = gtk::Button::with_label("Cancel");
    let connect = gtk::Button::with_label("Connect");
    connect.add_css_class("enabled");
    row.append(&cancel_b);
    row.append(&connect);
    o.panel.append(&row);

    let send = {
        let (password, name, settings) = (password.clone(), name.clone(), settings.clone());
        move || {
            let h = password.text().to_string();
            if h.is_empty() {
                return;
            }
            let mut secrets: HashMap<String, Variant> = HashMap::new();
            if enterprise {
                secrets.insert("password".into(), h.to_variant());
                let j = name.text().to_string();
                if !j.is_empty() {
                    secrets.insert("identity".into(), j.to_variant());
                }
            } else {
                secrets.insert("psk".into(), h.to_variant());
            }
            let mut reply: Connection = HashMap::new();
            reply.insert(settings.clone(), secrets);
            if let Some(z) = REQUEST.with(|z| z.borrow_mut().take()) {
                z.invocation.return_value(Some(&(reply,).to_variant()));
                z.popup.close();
            }
        }
    };
    let o2 = send.clone();
    password.connect_activate(move |_| o2());
    connect.connect_clicked(move |_| send());
    cancel_b.connect_clicked(|_| cancel(None, "org.freedesktop.NetworkManager.SecretAgent.UserCanceled"));

    REQUEST.with(|z| z.replace(Some(Request { path: path.as_str().to_string(), invocation, popup: o.clone() })));
    o.show();
    if enterprise && name.text().is_empty() {
        name.grab_focus();
    } else {
        password.grab_focus();
    }
}
