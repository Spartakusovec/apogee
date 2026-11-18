//! Moduly horního panelu: každý zdroj (zvoneček, počasí, plochy…) sem pošle svůj stav jako JSON
//! {text, class, tooltip}. Vlastní lišta démona (lista/) ho dostane přímo; kvůli waybaru jako záloze
//! umí `apogee modul <název>` vypisovat totéž jako JSON řádky (return-type json).

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::Mutex;

static ODBERATELE: Mutex<Option<HashMap<String, Vec<UnixStream>>>> = Mutex::new(None);
/// lišta démona (hlavní vlákno GTK): (modul, JSON)
static LISTA: std::sync::OnceLock<async_channel::Sender<(String, String)>> = std::sync::OnceLock::new();
static HODNOTY: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

/// Nová hodnota modulu: uloží ji a pošle všem připojeným (odpojené zahodí).
pub fn publikovat(modul: &str, json: serde_json::Value) {
    let radek = json.to_string() + "\n";
    {
        let mut h = HODNOTY.lock().unwrap();
        let h = h.get_or_insert_with(HashMap::new);
        if h.get(modul) == Some(&radek) {
            return; // nic se nezměnilo, panel nepřekreslovat
        }
        h.insert(modul.to_string(), radek.clone());
    }
    if let Some(tx) = LISTA.get() {
        let _ = tx.try_send((modul.to_string(), radek.trim_end().to_string()));
    }
    let mut o = ODBERATELE.lock().unwrap();
    if let Some(seznam) = o.get_or_insert_with(HashMap::new).get_mut(modul) {
        seznam.retain_mut(|s| s.write_all(radek.as_bytes()).is_ok());
    }
}

/// Modul panelu se připojil: hned mu poslat poslední hodnotu.
pub fn prihlasit(modul: &str, mut spojeni: UnixStream) {
    if let Some(radek) = HODNOTY.lock().unwrap().get_or_insert_with(HashMap::new).get(modul) {
        if spojeni.write_all(radek.as_bytes()).is_err() {
            return;
        }
    }
    ODBERATELE.lock().unwrap().get_or_insert_with(HashMap::new).entry(modul.to_string()).or_default().push(spojeni);
}

/// HTML escapování pro text ve waybaru (pango markup).
pub fn escapovat(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Lišta démona: změny všech modulů (nejdřív všechny dosavadní hodnoty).
pub fn odebirat() -> async_channel::Receiver<(String, String)> {
    let (tx, rx) = async_channel::unbounded();
    // pod zámkem: co se publikuje mezitím, přijde až po těchhle hodnotách
    let mut h = HODNOTY.lock().unwrap();
    for (m, r) in h.get_or_insert_with(HashMap::new).iter() {
        let _ = tx.try_send((m.clone(), r.trim_end().to_string()));
    }
    let _ = LISTA.set(tx);
    rx
}
