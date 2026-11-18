//! Převody jednotek a měn: „10 km to mi“, „5 kg na lb“, „100 °F v °C“, „2 GB in MB“,
//! „100 eur na kč“, „50 usd to czk“; spojka není nutná („10 eur usd“) a samotná měna
//! se převede na koruny („10 eur“), koruny na eura. Kurzy jsou z Evropské centrální banky (frankfurter.dev),
//! uložené v ~/.cache/apogee/rates.json a obnovené, když jsou starší než 12 hodin.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::kalkulacka::formatovat;
use crate::spolecne::{domov, prikaz};

#[derive(Clone, Copy, PartialEq)]
enum Druh {
    Delka,
    Hmotnost,
    Objem,
    Plocha,
    Rychlost,
    Cas,
    Data,
    Energie,
    Teplota,
}

/// (názvy, druh, kolik základních jednotek, hezký název)
const JEDNOTKY: &[(&[&str], Druh, f64, &str)] = &[
    (&["mm", "milimetr", "milimetrů", "millimeter", "millimeters"], Druh::Delka, 0.001, "mm"),
    (&["cm", "centimetr", "centimetrů", "centimeter", "centimeters"], Druh::Delka, 0.01, "cm"),
    (&["dm"], Druh::Delka, 0.1, "dm"),
    (&["m", "metr", "metrů", "metry", "meter", "meters", "metre"], Druh::Delka, 1.0, "m"),
    (&["km", "kilometr", "kilometrů", "kilometer", "kilometers"], Druh::Delka, 1000.0, "km"),
    (&["in", "inch", "inches", "palec", "palců", "\""], Druh::Delka, 0.0254, "in"),
    (&["ft", "foot", "feet", "stopa", "stop"], Druh::Delka, 0.3048, "ft"),
    (&["yd", "yard", "yards"], Druh::Delka, 0.9144, "yd"),
    (&["mi", "mile", "miles", "míle", "mil"], Druh::Delka, 1609.344, "mi"),
    (&["nmi", "námořní míle"], Druh::Delka, 1852.0, "nmi"),
    (&["mg", "miligram"], Druh::Hmotnost, 0.001, "mg"),
    (&["g", "gram", "gramů", "grams"], Druh::Hmotnost, 1.0, "g"),
    (&["dkg", "deka"], Druh::Hmotnost, 10.0, "dkg"),
    (&["kg", "kilo", "kilogram", "kilogramů", "kilograms"], Druh::Hmotnost, 1000.0, "kg"),
    (&["t", "tuna", "tun", "tonne", "tonnes"], Druh::Hmotnost, 1_000_000.0, "t"),
    (&["oz", "ounce", "ounces", "unce"], Druh::Hmotnost, 28.349523125, "oz"),
    (&["lb", "lbs", "pound", "pounds", "libra", "liber"], Druh::Hmotnost, 453.59237, "lb"),
    (&["ml", "mililitr"], Druh::Objem, 0.001, "ml"),
    (&["cl"], Druh::Objem, 0.01, "cl"),
    (&["dl", "decilitr"], Druh::Objem, 0.1, "dl"),
    (&["l", "litr", "litrů", "litry", "liter", "liters", "litre"], Druh::Objem, 1.0, "l"),
    (&["m3", "m³"], Druh::Objem, 1000.0, "m³"),
    (&["gal", "gallon", "gallons", "galon", "galonů"], Druh::Objem, 3.785411784, "gal"),
    (&["qt", "quart"], Druh::Objem, 0.946352946, "qt"),
    (&["pt", "pint", "pints"], Druh::Objem, 0.473176473, "pt"),
    (&["cup", "cups", "hrnek", "hrnků"], Druh::Objem, 0.2365882365, "cup"),
    (&["floz", "fl oz"], Druh::Objem, 0.0295735295625, "fl oz"),
    (&["m2", "m²"], Druh::Plocha, 1.0, "m²"),
    (&["km2", "km²"], Druh::Plocha, 1_000_000.0, "km²"),
    (&["ha", "hektar", "hektarů"], Druh::Plocha, 10_000.0, "ha"),
    (&["a", "ar", "arů"], Druh::Plocha, 100.0, "a"),
    (&["ft2", "ft²", "sqft"], Druh::Plocha, 0.09290304, "ft²"),
    (&["acre", "acres", "akr"], Druh::Plocha, 4046.8564224, "acre"),
    (&["km/h", "kmh", "kph"], Druh::Rychlost, 1.0 / 3.6, "km/h"),
    (&["m/s"], Druh::Rychlost, 1.0, "m/s"),
    (&["mph"], Druh::Rychlost, 0.44704, "mph"),
    (&["kn", "knot", "knots", "uzel", "uzlů"], Druh::Rychlost, 0.514444, "kn"),
    (&["ms", "millisecond", "milisekund"], Druh::Cas, 0.001, "ms"),
    (&["s", "sec", "second", "seconds", "sekund", "sekundy"], Druh::Cas, 1.0, "s"),
    (&["min", "minute", "minutes", "minut", "minuty"], Druh::Cas, 60.0, "min"),
    (&["h", "hod", "hour", "hours", "hodin", "hodiny"], Druh::Cas, 3600.0, "h"),
    (&["d", "day", "days", "den", "dní", "dny"], Druh::Cas, 86400.0, "days"),
    (&["week", "weeks", "týden", "týdnů", "týdny"], Druh::Cas, 604_800.0, "weeks"),
    (&["year", "years", "rok", "roků", "roky", "let"], Druh::Cas, 31_557_600.0, "years"),
    (&["b", "byte", "bytes", "bajt", "bajtů"], Druh::Data, 1.0, "B"),
    (&["kb"], Druh::Data, 1e3, "kB"),
    (&["mb"], Druh::Data, 1e6, "MB"),
    (&["gb"], Druh::Data, 1e9, "GB"),
    (&["tb"], Druh::Data, 1e12, "TB"),
    (&["kib"], Druh::Data, 1024.0, "KiB"),
    (&["mib"], Druh::Data, 1048576.0, "MiB"),
    (&["gib"], Druh::Data, 1073741824.0, "GiB"),
    (&["tib"], Druh::Data, 1099511627776.0, "TiB"),
    (&["j", "joule", "joules"], Druh::Energie, 1.0, "J"),
    (&["kj"], Druh::Energie, 1000.0, "kJ"),
    (&["cal", "kalorie"], Druh::Energie, 4.184, "cal"),
    (&["kcal"], Druh::Energie, 4184.0, "kcal"),
    (&["wh"], Druh::Energie, 3600.0, "Wh"),
    (&["kwh"], Druh::Energie, 3_600_000.0, "kWh"),
    (&["c", "°c", "celsius", "stupňů celsia"], Druh::Teplota, 0.0, "°C"),
    (&["f", "°f", "fahrenheit"], Druh::Teplota, 1.0, "°F"),
    (&["k", "kelvin", "kelvinů"], Druh::Teplota, 2.0, "K"),
];

/// Měny zapsané symbolem nebo česky
fn mena(nazev: &str) -> Option<String> {
    let n = nazev.to_lowercase();
    let kod = match n.as_str() {
        "kč" | "kc" | "korun" | "koruny" | "korunu" => "CZK",
        "€" | "euro" | "eur" | "eura" => "EUR",
        "$" | "dolar" | "dolarů" | "dolary" | "usd" => "USD",
        "£" | "libra šterlinků" => "GBP",
        "zł" | "zlotý" | "zlotých" => "PLN",
        "¥" => "JPY",
        "ft." | "forint" | "forintů" => "HUF",
        _ if n.len() == 3 && n.chars().all(|c| c.is_ascii_alphabetic()) => return Some(n.to_uppercase()),
        _ => return None,
    };
    Some(kod.to_string())
}

fn jednotka(nazev: &str) -> Option<&'static (&'static [&'static str], Druh, f64, &'static str)> {
    let n = nazev.to_lowercase();
    // přesně (m ≠ M u dat, proto malými písmeny, „mb“ = megabajty)
    JEDNOTKY.iter().find(|(nazvy, _, _, _)| nazvy.contains(&n.as_str()))
}

fn soubor_kurzu() -> std::path::PathBuf {
    domov().join(".cache/apogee/rates.json")
}

/// Kurzy vůči euru (None = ještě nestažené).
fn kurzy() -> Option<HashMap<String, f64>> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(soubor_kurzu()).ok()?).ok()?;
    let mut m: HashMap<String, f64> = v["rates"].as_object()?.iter().filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x))).collect();
    m.insert("EUR".into(), 1.0);
    Some(m)
}

/// Jsou kurzy starší než 12 hodin (nebo chybí)?
pub fn kurzy_zastarale() -> bool {
    std::fs::metadata(soubor_kurzu())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_none_or(|d| d > Duration::from_secs(12 * 3600))
}

/// Stáhne kurzy (pomalé, volat mimo GTK). true = povedlo se.
pub fn stahnout_kurzy() -> bool {
    let text = prikaz("curl", &["-sfL", "--max-time", "8", "https://api.frankfurter.dev/v1/latest?base=EUR"]);
    if serde_json::from_str::<Value>(&text).ok().is_some_and(|v| v["rates"].is_object()) {
        let _ = std::fs::create_dir_all(domov().join(".cache/apogee"));
        return std::fs::write(soubor_kurzu(), text).is_ok();
    }
    false
}

pub struct Prevod {
    /// „10 mi = 16,093 km“
    pub nadpis: String,
    /// co se zkopíruje (jen číslo)
    pub vysledek: String,
    pub popis: String,
    /// převod měn, ale kurzy ještě nejsou stažené
    pub ceka_na_kurzy: bool,
}

/// Rozdělí „10km“ / „10 km“ na číslo a zbytek.
fn cislo_a_zbytek(text: &str) -> Option<(f64, String)> {
    let text = text.trim();
    let konec = text.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-')).unwrap_or(text.len());
    if konec == 0 {
        return None;
    }
    let cislo = text[..konec].replace(',', ".").parse::<f64>().ok()?;
    Some((cislo, text[konec..].trim().to_string()))
}

/// „10 km to mi“, „10 km mi“, „100 eur na kč“, „10 eur usd“, a „10 eur“ (= na koruny, koruny na eura).
pub fn prevest(dotaz: &str) -> Option<Prevod> {
    let maly = dotaz.to_lowercase();
    let carka = dotaz.contains(',');
    // „<číslo> <jednotka> to|in|na|v|do|-> <jednotka>“
    if let Some((levo, pravo)) = [" to ", " in ", " na ", " v ", " do ", " -> ", " → ", " = ", " into "].iter().find_map(|s| maly.split_once(s)) {
        let (mnozstvi, z) = cislo_a_zbytek(levo)?;
        return prevest_par(mnozstvi, z.trim(), pravo.trim(), carka);
    }
    // bez spojky: „10 eur usd“, „10 km mi“ (jednotky můžou mít víc slov, zkusit každé rozdělení)
    let (mnozstvi, zbytek) = cislo_a_zbytek(&maly)?;
    let slova: Vec<&str> = zbytek.split_whitespace().collect();
    if slova.len() == 1 {
        // jen měna: na koruny, koruny na eura
        let a = mena(slova[0])?;
        kurzy().filter(|k| k.contains_key(&a))?;
        let cil = if a == "CZK" { "eur" } else { "czk" };
        return prevest_par(mnozstvi, slova[0], cil, carka);
    }
    (1..slova.len()).find_map(|i| prevest_par(mnozstvi, &slova[..i].join(" "), &slova[i..].join(" "), carka))
}

fn prevest_par(mnozstvi: f64, z: &str, na: &str, carka: bool) -> Option<Prevod> {
    if z.is_empty() || na.is_empty() {
        return None;
    }
    if let (Some(a), Some(b)) = (jednotka(z), jednotka(na)) {
        if a.1 != b.1 {
            return None;
        }
        let vysledek = if a.1 == Druh::Teplota {
            let na_celsia = match a.2 as i32 {
                0 => mnozstvi,
                1 => (mnozstvi - 32.0) * 5.0 / 9.0,
                _ => mnozstvi - 273.15,
            };
            match b.2 as i32 {
                0 => na_celsia,
                1 => na_celsia * 9.0 / 5.0 + 32.0,
                _ => na_celsia + 273.15,
            }
        } else {
            mnozstvi * a.2 / b.2
        };
        let v = formatovat(vysledek, carka);
        return Some(Prevod {
            nadpis: format!("{} {} = {v} {}", formatovat(mnozstvi, carka), a.3, b.3),
            vysledek: v,
            popis: "Unit conversion · Enter copies the result".into(),
            ceka_na_kurzy: false,
        });
    }
    let (a, b) = (mena(z)?, mena(na)?);
    let Some(k) = kurzy() else {
        return Some(Prevod { nadpis: format!("{} {a} → {b}", formatovat(mnozstvi, carka)), vysledek: String::new(), popis: "Loading exchange rates…".into(), ceka_na_kurzy: true });
    };
    let (ka, kb) = (k.get(&a)?, k.get(&b)?);
    let vysledek = (mnozstvi / ka * kb * 100.0).round() / 100.0;
    let v = formatovat(vysledek, carka);
    Some(Prevod {
        nadpis: format!("{} {a} = {v} {b}", formatovat(mnozstvi, carka)),
        vysledek: v,
        popis: "Currency (European Central Bank rate) · Enter copies the result".into(),
        ceka_na_kurzy: false,
    })
}

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn prevody() {
        let p = |t: &str| prevest(t).map(|p| p.nadpis);
        assert_eq!(p("10 km to mi").as_deref(), Some("10 km = 6.213711922 mi"));
        assert_eq!(p("5kg na lb").as_deref(), Some("5 kg = 11.02311311 lb"));
        assert_eq!(p("100 °f v °c").as_deref(), Some("100 °F = 37.77777778 °C"));
        assert_eq!(p("2 gb in mb").as_deref(), Some("2 GB = 2000 MB"));
        assert_eq!(p("1,5 l na ml").as_deref(), Some("1,5 l = 1500 ml"));
        assert_eq!(p("10 km mi").as_deref(), Some("10 km = 6.213711922 mi"));
        assert_eq!(p("1,5 l ml").as_deref(), Some("1,5 l = 1500 ml"));
        assert_eq!(p("100 fl oz ml").as_deref(), Some("100 fl oz = 2957.352956 ml"));
        assert_eq!(p("10 km to kg"), None);
        assert_eq!(p("10 km"), None);
        assert_eq!(p("firefox"), None);
    }
}
