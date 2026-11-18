//! Nastavení Swaye z aplikace Settings: `~/.config/sway/settings.conf`, který hlavní konfigurace
//! načte na konci (`include`), takže přepíše výchozí hodnoty. Klíčem řádku je začátek příkazu
//! (třeba `output eDP-1 scale` nebo `input type:touchpad tap`). Změna se uloží a hned pošle
//! běžícímu Swayi.

use crate::spolecne::domov;

fn soubor() -> std::path::PathBuf {
    domov().join(".config/sway/settings.conf")
}

const HLAVICKA: &str = "# Generuje aplikace Settings (apogee settings); ruční úpravy přepíše.\n# Načítá se na konci ~/.config/sway/config.\n";

fn radky() -> Vec<String> {
    std::fs::read_to_string(soubor())
        .unwrap_or_default()
        .lines()
        .filter(|r| !r.starts_with('#') && !r.trim().is_empty())
        .map(String::from)
        .collect()
}

fn je_klic(radek: &str, klic: &str) -> bool {
    radek.starts_with(klic) && radek[klic.len()..].starts_with(' ')
}

fn zapsat(radky: &[String]) {
    let _ = std::fs::write(soubor(), format!("{HLAVICKA}{}\n", radky.join("\n")));
}

/// Uložená hodnota (řádek bez klíče), když ji Settings někdy nastavila.
pub fn hodnota(klic: &str) -> Option<String> {
    radky().iter().find(|r| je_klic(r, klic)).map(|r| r[klic.len()..].trim().to_string())
}

/// Uloží `klic hodnota` (nahradí starý řádek), Swayi nepošle nic.
pub fn ulozit(klic: &str, hodnota: &str) {
    let mut r: Vec<String> = radky().into_iter().filter(|r| !je_klic(r, klic)).collect();
    r.push(format!("{klic} {hodnota}"));
    zapsat(&r);
}

/// Uloží `klic hodnota` a pošle ho Swayi.
pub fn nastavit(klic: &str, hodnota: &str) {
    ulozit(klic, hodnota);
    crate::sway::prikaz(&format!("{klic} {hodnota}"));
}
