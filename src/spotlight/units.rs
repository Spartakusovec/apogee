//! Unit and currency conversions: “10 km to mi”, “5 kg to lb”, “100 °F in °C”, “2 GB in MB”,
//! “100 eur to czk”, “50 usd to czk”; the connecting word is optional (“10 eur usd”), and a currency alone
//! is converted to Czech crowns (“10 eur”), crowns to euros. Rates from the European Central Bank (frankfurter.dev),
//! saved in ~/.cache/apogee/rates.json and refreshed when older than 12 hours.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::calculator::format;
use crate::common::{home, command};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Length,
    Mass,
    Volume,
    Workspace,
    Speed,
    Time,
    Data,
    Energy,
    Temperature,
}

/// (names, kind, how many base units, nice name)
const UNITS: &[(&[&str], Kind, f64, &str)] = &[
    (&["mm", "milimetr", "milimetrů", "millimeter", "millimeters"], Kind::Length, 0.001, "mm"),
    (&["cm", "centimetr", "centimetrů", "centimeter", "centimeters"], Kind::Length, 0.01, "cm"),
    (&["dm"], Kind::Length, 0.1, "dm"),
    (&["m", "metr", "metrů", "metry", "meter", "meters", "metre"], Kind::Length, 1.0, "m"),
    (&["km", "kilometr", "kilometrů", "kilometer", "kilometers"], Kind::Length, 1000.0, "km"),
    (&["in", "inch", "inches", "palec", "palců", "\""], Kind::Length, 0.0254, "in"),
    (&["ft", "foot", "feet", "stopa", "stop"], Kind::Length, 0.3048, "ft"),
    (&["yd", "yard", "yards"], Kind::Length, 0.9144, "yd"),
    (&["mi", "mile", "miles", "míle", "mil"], Kind::Length, 1609.344, "mi"),
    (&["nmi", "námořní míle"], Kind::Length, 1852.0, "nmi"),
    (&["mg", "miligram"], Kind::Mass, 0.001, "mg"),
    (&["g", "gram", "gramů", "grams"], Kind::Mass, 1.0, "g"),
    (&["dkg", "deka"], Kind::Mass, 10.0, "dkg"),
    (&["kg", "kilo", "kilogram", "kilogramů", "kilograms"], Kind::Mass, 1000.0, "kg"),
    (&["t", "tuna", "tun", "tonne", "tonnes"], Kind::Mass, 1_000_000.0, "t"),
    (&["oz", "ounce", "ounces", "unce"], Kind::Mass, 28.349523125, "oz"),
    (&["lb", "lbs", "pound", "pounds", "libra", "liber"], Kind::Mass, 453.59237, "lb"),
    (&["ml", "mililitr"], Kind::Volume, 0.001, "ml"),
    (&["cl"], Kind::Volume, 0.01, "cl"),
    (&["dl", "decilitr"], Kind::Volume, 0.1, "dl"),
    (&["l", "litr", "litrů", "litry", "liter", "liters", "litre"], Kind::Volume, 1.0, "l"),
    (&["m3", "m³"], Kind::Volume, 1000.0, "m³"),
    (&["gal", "gallon", "gallons", "galon", "galonů"], Kind::Volume, 3.785411784, "gal"),
    (&["qt", "quart"], Kind::Volume, 0.946352946, "qt"),
    (&["pt", "pint", "pints"], Kind::Volume, 0.473176473, "pt"),
    (&["cup", "cups", "hrnek", "hrnků"], Kind::Volume, 0.2365882365, "cup"),
    (&["floz", "fl oz"], Kind::Volume, 0.0295735295625, "fl oz"),
    (&["m2", "m²"], Kind::Workspace, 1.0, "m²"),
    (&["km2", "km²"], Kind::Workspace, 1_000_000.0, "km²"),
    (&["ha", "hektar", "hektarů"], Kind::Workspace, 10_000.0, "ha"),
    (&["a", "ar", "arů"], Kind::Workspace, 100.0, "a"),
    (&["ft2", "ft²", "sqft"], Kind::Workspace, 0.09290304, "ft²"),
    (&["acre", "acres", "akr"], Kind::Workspace, 4046.8564224, "acre"),
    (&["km/h", "kmh", "kph"], Kind::Speed, 1.0 / 3.6, "km/h"),
    (&["m/s"], Kind::Speed, 1.0, "m/s"),
    (&["mph"], Kind::Speed, 0.44704, "mph"),
    (&["kn", "knot", "knots", "uzel", "uzlů"], Kind::Speed, 0.514444, "kn"),
    (&["ms", "millisecond", "milisekund"], Kind::Time, 0.001, "ms"),
    (&["s", "sec", "second", "seconds", "sekund", "sekundy"], Kind::Time, 1.0, "s"),
    (&["min", "minute", "minutes", "minut", "minuty"], Kind::Time, 60.0, "min"),
    (&["h", "hod", "hour", "hours", "hodin", "hodiny"], Kind::Time, 3600.0, "h"),
    (&["d", "day", "days", "den", "dní", "dny"], Kind::Time, 86400.0, "days"),
    (&["week", "weeks", "týden", "týdnů", "týdny"], Kind::Time, 604_800.0, "weeks"),
    (&["year", "years", "rok", "roků", "roky", "let"], Kind::Time, 31_557_600.0, "years"),
    (&["b", "byte", "bytes", "bajt", "bajtů"], Kind::Data, 1.0, "B"),
    (&["kb"], Kind::Data, 1e3, "kB"),
    (&["mb"], Kind::Data, 1e6, "MB"),
    (&["gb"], Kind::Data, 1e9, "GB"),
    (&["tb"], Kind::Data, 1e12, "TB"),
    (&["kib"], Kind::Data, 1024.0, "KiB"),
    (&["mib"], Kind::Data, 1048576.0, "MiB"),
    (&["gib"], Kind::Data, 1073741824.0, "GiB"),
    (&["tib"], Kind::Data, 1099511627776.0, "TiB"),
    (&["j", "joule", "joules"], Kind::Energy, 1.0, "J"),
    (&["kj"], Kind::Energy, 1000.0, "kJ"),
    (&["cal", "kalorie"], Kind::Energy, 4.184, "cal"),
    (&["kcal"], Kind::Energy, 4184.0, "kcal"),
    (&["wh"], Kind::Energy, 3600.0, "Wh"),
    (&["kwh"], Kind::Energy, 3_600_000.0, "kWh"),
    (&["c", "°c", "celsius", "stupňů celsia"], Kind::Temperature, 0.0, "°C"),
    (&["f", "°f", "fahrenheit"], Kind::Temperature, 1.0, "°F"),
    (&["k", "kelvin", "kelvinů"], Kind::Temperature, 2.0, "K"),
];

/// Currencies written as a symbol or in Czech
fn mena(name: &str) -> Option<String> {
    let n = name.to_lowercase();
    let code = match n.as_str() {
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
    Some(code.to_string())
}

fn unit(name: &str) -> Option<&'static (&'static [&'static str], Kind, f64, &'static str)> {
    let n = name.to_lowercase();
    // exactly (m ≠ M for data, so lower case, “mb” = megabytes)
    UNITS.iter().find(|(names, _, _, _)| names.contains(&n.as_str()))
}

fn file_rates() -> std::path::PathBuf {
    home().join(".cache/apogee/rates.json")
}

/// Rates against the euro (None = not downloaded yet).
fn rates() -> Option<HashMap<String, f64>> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(file_rates()).ok()?).ok()?;
    let mut m: HashMap<String, f64> = v["rates"].as_object()?.iter().filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x))).collect();
    m.insert("EUR".into(), 1.0);
    Some(m)
}

/// Are the rates older than 12 hours (or missing)?
pub fn rates_stale() -> bool {
    std::fs::metadata(file_rates())
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_none_or(|d| d > Duration::from_secs(12 * 3600))
}

/// Downloads the rates (slow, call outside GTK). true = it worked.
pub fn download_rates() -> bool {
    let text = command("curl", &["-sfL", "--max-time", "8", "https://api.frankfurter.dev/v1/latest?base=EUR"]);
    if serde_json::from_str::<Value>(&text).ok().is_some_and(|v| v["rates"].is_object()) {
        let _ = std::fs::create_dir_all(home().join(".cache/apogee"));
        return std::fs::write(file_rates(), text).is_ok();
    }
    false
}

pub struct Prevod {
    /// „10 mi = 16,093 km“
    pub title: String,
    /// what gets copied (just the number)
    pub result: String,
    pub description: String,
    /// a currency conversion, but the rates aren't downloaded yet
    pub ceka_on_rates: bool,
}

/// Splits “10km” / “10 km” into the number and the rest.
fn number_a_rest(text: &str) -> Option<(f64, String)> {
    let text = text.trim();
    let end = text.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ',' || c == '-')).unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    let number = text[..end].replace(',', ".").parse::<f64>().ok()?;
    Some((number, text[end..].trim().to_string()))
}

/// “10 km to mi”, “10 km mi”, “100 eur to czk”, “10 eur usd”, and “10 eur” (= to crowns, crowns to euros).
pub fn convert(query: &str) -> Option<Prevod> {
    let small = query.to_lowercase();
    let comma = query.contains(',');
    // “<number> <unit> to|in|na|v|do|-> <unit>” (Czech connecting words work too)
    if let Some((levo, pravo)) = [" to ", " in ", " na ", " v ", " do ", " -> ", " → ", " = ", " into "].iter().find_map(|s| small.split_once(s)) {
        let (mnozstvi, z) = number_a_rest(levo)?;
        return prevest_par(mnozstvi, z.trim(), pravo.trim(), comma);
    }
    // without a connecting word: “10 eur usd”, “10 km mi” (units can have several words, try every split)
    let (mnozstvi, rest) = number_a_rest(&small)?;
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.len() == 1 {
        // a currency alone: to crowns, crowns to euros
        let a = mena(words[0])?;
        rates().filter(|k| k.contains_key(&a))?;
        let target = if a == "CZK" { "eur" } else { "czk" };
        return prevest_par(mnozstvi, words[0], target, comma);
    }
    (1..words.len()).find_map(|i| prevest_par(mnozstvi, &words[..i].join(" "), &words[i..].join(" "), comma))
}

fn prevest_par(mnozstvi: f64, z: &str, on: &str, comma: bool) -> Option<Prevod> {
    if z.is_empty() || on.is_empty() {
        return None;
    }
    if let (Some(a), Some(b)) = (unit(z), unit(on)) {
        if a.1 != b.1 {
            return None;
        }
        let result = if a.1 == Kind::Temperature {
            let on_celsia = match a.2 as i32 {
                0 => mnozstvi,
                1 => (mnozstvi - 32.0) * 5.0 / 9.0,
                _ => mnozstvi - 273.15,
            };
            match b.2 as i32 {
                0 => on_celsia,
                1 => on_celsia * 9.0 / 5.0 + 32.0,
                _ => on_celsia + 273.15,
            }
        } else {
            mnozstvi * a.2 / b.2
        };
        let v = format(result, comma);
        return Some(Prevod {
            title: format!("{} {} = {v} {}", format(mnozstvi, comma), a.3, b.3),
            result: v,
            description: "Unit conversion · Enter copies the result".into(),
            ceka_on_rates: false,
        });
    }
    let (a, b) = (mena(z)?, mena(on)?);
    let Some(k) = rates() else {
        return Some(Prevod { title: format!("{} {a} → {b}", format(mnozstvi, comma)), result: String::new(), description: "Loading exchange rates…".into(), ceka_on_rates: true });
    };
    let (ka, kb) = (k.get(&a)?, k.get(&b)?);
    let result = (mnozstvi / ka * kb * 100.0).round() / 100.0;
    let v = format(result, comma);
    Some(Prevod {
        title: format!("{} {a} = {v} {b}", format(mnozstvi, comma)),
        result: v,
        description: "Currency (European Central Bank rate) · Enter copies the result".into(),
        ceka_on_rates: false,
    })
}

#[cfg(test)]
mod testy {
    use super::*;

    #[test]
    fn prevody() {
        let p = |t: &str| convert(t).map(|p| p.title);
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
