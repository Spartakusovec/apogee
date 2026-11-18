//! Barvy z tématu (~/.config/theme/palette.json, generuje set-wallpaper) a CSS okének.
//! Paleta se čte při každém otevření okénka, takže změna tapety se projeví hned.

use std::collections::HashMap;

pub struct Paleta(HashMap<String, String>);

impl Paleta {
    pub fn nacist() -> Paleta {
        let vychozi = [
            ("base", "#1e1e2e"), ("mantle", "#181825"), ("surface", "#313244"), ("overlay", "#6c7086"),
            ("text", "#cdd6f4"), ("dim", "#9399b2"), ("accent", "#89b4fa"), ("accent2", "#f5c2e7"),
            ("red", "#f38ba8"), ("green", "#a6e3a1"), ("yellow", "#f9e2af"),
        ];
        let mut barvy: HashMap<String, String> =
            vychozi.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let soubor = crate::spolecne::domov().join(".config/theme/palette.json");
        if let Ok(text) = std::fs::read_to_string(soubor) {
            if let Ok(serde_json::Value::Object(mapa)) = serde_json::from_str(&text) {
                for (k, v) in mapa {
                    if let Some(s) = v.as_str() {
                        if s.starts_with('#') {
                            barvy.insert(k, s.to_string());
                        }
                    }
                }
            }
        }
        Paleta(barvy)
    }

    pub fn barva(&self, nazev: &str) -> &str {
        self.0.get(nazev).map(String::as_str).unwrap_or("#888888")
    }

    /// #rrggbb → (r, g, b) 0–1 pro kreslení
    pub fn rgb(&self, nazev: &str) -> (f64, f64, f64) {
        let h = self.barva(nazev).trim_start_matches('#');
        let kanal = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("88"), 16).unwrap_or(136) as f64 / 255.0;
        (kanal(0), kanal(2), kanal(4))
    }
}

pub fn css(p: &Paleta) -> String {
    let b = |n| p.barva(n);
    format!(
        r#"
window.apogee {{ background: transparent; }}
#panel {{
    background: alpha({base}, 0.96);
    border: 1px solid {surface};
    border-radius: 18px;
    padding: 16px 18px;
}}
#panel label {{ color: {text}; font-family: "Adwaita Sans"; font-size: 14px; }}
#panel .nadpis {{ font-size: 16px; font-weight: 700; }}
#panel .dim {{ color: {dim}; font-size: 12px; }}
#panel .ikona {{ font-family: "Font Awesome 6 Free"; font-size: 16px; color: {accent2}; }}
#panel .hodnota {{ min-width: 46px; }}
#panel scale trough {{ background: {mantle}; border-radius: 8px; min-height: 8px; border: none; }}
#panel scale highlight {{ background: {accent}; border-radius: 8px; border: none; }}
#panel scale slider {{ background: {text}; border-radius: 100%; min-width: 16px; min-height: 16px;
                       margin: -5px; border: none; box-shadow: none; outline: none; }}
#panel button {{ background: {surface}; border: none; border-radius: 10px; padding: 6px 12px;
                 box-shadow: none; color: {text}; outline: none; }}
#panel button:hover {{ background: {overlay}; }}
#panel button.zapnuto {{ background: {accent}; }}
#panel button.zapnuto label {{ color: {base}; }}
#panel button.radek {{ background: transparent; padding: 7px 10px; }}
#panel button.radek:hover, #panel button.radek:focus-visible {{ background: {surface}; }}
#panel button.odkaz {{ background: transparent; padding: 4px 6px; }}
#panel button.odkaz label {{ color: {accent}; font-size: 13px; }}
#panel switch {{ background: {mantle}; border: none; border-radius: 14px; outline: none; }}
#panel switch:checked {{ background: {accent}; }}
#panel switch slider {{ background: {text}; border-radius: 14px; border: none; min-width: 22px; min-height: 22px; box-shadow: none; }}
#panel separator {{ background: {surface}; margin: 12px 0; min-height: 1px; }}
#panel .vpravo {{ color: {dim}; font-size: 12px; }}
#panel .aktivni label {{ color: {accent}; font-weight: 700; }}
#panel entry {{ background: {mantle}; color: {text}; border: 1px solid {surface};
                border-radius: 10px; padding: 6px 10px; caret-color: {accent}; box-shadow: none; outline: none; }}
#panel entry:focus-within {{ border-color: {accent}; }}
#panel .velke {{ font-size: 30px; font-weight: 700; }}
#panel .obri {{ font-size: 44px; font-weight: 300; }}
#panel .pocasi-ikona {{ font-family: "Font Awesome 6 Free"; font-size: 34px; color: {accent2}; }}
#panel .mala-ikona {{ font-family: "Font Awesome 6 Free"; font-size: 15px; color: {accent2}; }}
#panel .dest {{ color: {accent}; font-size: 11px; }}
#panel levelbar trough {{ background: {mantle}; border-radius: 8px; min-height: 10px; border: none; padding: 0; }}
#panel levelbar block.filled {{ background: {accent}; border-radius: 8px; border: none; }}
#panel levelbar block.empty {{ background: transparent; border: none; }}
#panel levelbar.nizka block.filled {{ background: {red}; }}
#panel .den {{ min-width: 38px; min-height: 34px; border-radius: 10px; }}
#panel .den.jiny-mesic {{ color: alpha({dim}, 0.45); }}
#panel .den.vikend {{ color: {accent2}; }}
#panel .den.dnes {{ background: {accent}; color: {base}; font-weight: 700; }}
#panel .den-hlavicka {{ color: {dim}; font-size: 12px; }}
#panel .tyden {{ color: alpha({dim}, 0.6); font-size: 11px; min-width: 26px; }}
#panel button.sipka {{ background: transparent; padding: 2px 10px; }}
#panel button.sipka:hover {{ background: {surface}; }}
#panel button.rychle {{ min-height: 42px; padding: 0; }}
#panel button.rychle label {{ font-family: "Font Awesome 6 Free"; font-size: 16px; }}
#panel button.rychle-ikona label {{ font-family: "Font Awesome 6 Free"; font-size: 15px; }}
#panel .notifikace {{ background: alpha({surface}, 0.55); border-radius: 12px; padding: 10px 12px; }}
#panel .notifikace .nadpis-notifikace {{ font-weight: 700; font-size: 13px; }}
#panel .notifikace .telo {{ color: {dim}; font-size: 13px; }}
#panel button.zavrit {{ background: transparent; padding: 0 4px; min-height: 0; }}
#panel button.zavrit label {{ color: {dim}; font-size: 12px; font-family: "Font Awesome 6 Free"; }}
#panel button.zavrit:hover label {{ color: {red}; }}
#panel .obal {{ background: {surface}; border-radius: 12px; }}
#panel .obal-nahradni {{ font-family: "Font Awesome 6 Free"; font-size: 34px; color: {dim}; }}
#panel .titulek-skladby {{ font-size: 16px; font-weight: 700; }}
#panel .maly-cas {{ color: {dim}; font-size: 11px; }}
#panel button.ovladani {{ background: transparent; min-width: 44px; min-height: 44px; padding: 0; border-radius: 22px; }}
#panel button.ovladani:hover {{ background: {surface}; }}
#panel button.ovladani label {{ font-family: "Font Awesome 6 Free"; font-size: 17px; }}
#panel button.ovladani:disabled label {{ color: alpha({dim}, 0.5); }}
#panel button.ovladani.hlavni {{ background: {accent}; min-width: 54px; min-height: 54px; border-radius: 27px; }}
#panel button.ovladani.hlavni:hover {{ background: alpha({accent}, 0.85); }}
#panel button.ovladani.hlavni label {{ color: {base}; font-size: 20px; }}
#panel button.hlavni-akce {{ background: {accent}; min-height: 38px; border-radius: 12px; }}
#panel button.hlavni-akce:hover {{ background: alpha({accent}, 0.85); }}
#panel button.hlavni-akce label {{ color: {base}; font-weight: 700; }}
#panel button.tapeta {{ padding: 0; border: 2px solid transparent; border-radius: 12px; background: {mantle}; }}
#panel button.tapeta:hover {{ border-color: {overlay}; }}
#panel button.tapeta.aktivni-tapeta {{ border-color: {accent}; }}
#panel entry {{ min-height: 0; }}

#panel .osd {{ padding: 12px 20px; }}
#panel list.schranka {{ background: transparent; }}
#panel list.schranka row {{ border-radius: 10px; padding: 7px 10px; outline: none; }}
#panel list.schranka row:hover {{ background: alpha({surface}, 0.6); }}
#panel list.schranka row:selected {{ background: {surface}; }}

/* dok */
#dok {{ background: alpha({base}, 0.88); border: 1px solid alpha({surface}, 0.9); border-radius: 20px;
        padding: 5px 8px 3px 8px; }}
#dok button.dok-polozka {{ background: none; border: none; box-shadow: none; outline: none;
        padding: 4px 4px 2px 4px; margin: 0 2px; border-radius: 14px; transition: background 150ms ease; }}
#dok button.dok-polozka:hover {{ background: {surface}; }}
#dok .tecky {{ min-height: 6px; margin-top: 2px; }}
#dok .tecka {{ min-width: 5px; min-height: 5px; border-radius: 3px; margin: 0 1px; background: {dim}; }}
#dok button.aktivni .tecka {{ background: {accent}; }}
/* minimalizované okno: jen kroužek */
#dok .tecka.skryta {{ background: transparent; box-shadow: inset 0 0 0 1px {dim}; }}
#dok .dok-mrizka .ctverec {{ min-width: 7px; min-height: 7px; border-radius: 3px; background: {text}; }}
#dok .dok-mrizka .ctverec.stred {{ background: {accent}; }}
popover.dok-nabidka > contents {{ background: transparent; border: none; box-shadow: none; padding: 0; }}
popover.dok-nabidka #panel {{ padding: 8px; }}

/* mřížka aplikací */
window.spoustec .ztmaveni {{ background: rgba(0, 0, 0, 0.45); }}
window.spoustec #panel {{ padding: 20px 22px; }}
window.spoustec flowbox.aplikace {{ background: transparent; }}
window.spoustec flowbox.aplikace flowboxchild {{ border-radius: 14px; padding: 12px 4px; outline: none; }}
window.spoustec flowbox.aplikace flowboxchild:hover {{ background: alpha({surface}, 0.6); }}
window.spoustec flowbox.aplikace flowboxchild:selected {{ background: {surface}; }}
window.spoustec .aplikace-dlazdice label {{ color: {text}; font-family: "Adwaita Sans"; font-size: 13px; }}

/* spotlight */
window.spotlight #panel {{ padding: 12px 14px; border-radius: 22px; }}
window.spotlight .spotlight-pole {{ padding: 6px 10px; }}
window.spotlight .spotlight-lupa {{ font-family: "Font Awesome 6 Free"; font-size: 22px; color: {dim}; }}
window.spotlight entry {{ background: transparent; border: none; box-shadow: none; outline: none;
        font-size: 24px; min-height: 48px; padding: 0; color: {text}; }}
window.spotlight list.spotlight-vysledky {{ background: transparent; margin-top: 6px; }}
window.spotlight list.spotlight-vysledky row {{ border-radius: 12px; padding: 8px 12px; outline: none; }}
window.spotlight list.spotlight-vysledky row.skupina {{ padding: 8px 10px 2px 10px; }}
window.spotlight list.spotlight-vysledky row:hover {{ background: alpha({surface}, 0.6); }}
window.spotlight list.spotlight-vysledky row:selected {{ background: {surface}; }}
window.spotlight .vysledek {{ font-size: 17px; }}
window.spotlight list.spotlight-vysledky .dim {{ font-size: 13px; }}
window.spotlight .spotlight-ikona {{ font-family: "Font Awesome 6 Free"; font-size: 24px; color: {accent2}; }}

/* horní lišta (dřív waybar) */
window.lista {{ background: transparent; color: {text};
        font-family: "Adwaita Sans", "Font Awesome 6 Free", "Font Awesome 6 Brands", sans-serif; font-size: 13px; }}
window.lista .ostruvek {{ background: alpha({base}, 0.88); border: 1px solid alpha({surface}, 0.9); border-radius: 14px; padding: 3px; }}
window.lista .modul, window.lista .plocha {{ padding: 0 11px; margin: 0 1px; border-radius: 11px; min-height: 24px;
        color: {text}; transition: background 150ms ease, color 150ms ease; }}
window.lista .klikaci:hover, window.lista .plocha:hover {{ background: {surface}; }}
window.lista #aplikace {{ font-weight: 700; padding: 0 12px 0 13px; }}
window.lista #dlazdice {{ color: {dim}; }}
window.lista #dlazdice.dlazdice {{ background: alpha({accent}, 0.18); color: {accent}; }}
window.lista .plocha {{ padding: 0 9px; min-width: 18px; color: {dim}; }}
window.lista .plocha.prazdna {{ color: alpha({dim}, 0.55); }}
window.lista .plocha.aktivni {{ background: alpha({accent}, 0.18); color: {accent}; font-weight: 700; }}
window.lista .plocha.nalehava {{ background: {red}; color: {base}; }}
window.lista #rezim {{ background: {yellow}; color: {base}; font-weight: 700; }}
window.lista #hodiny {{ font-weight: 700; letter-spacing: 0.3px; }}
window.lista #hudba {{ color: {accent2}; margin: 0; }}
window.lista #hudba.paused {{ color: {dim}; }}
window.lista #predchozi, window.lista #dalsi {{ padding: 0 9px; color: {dim}; font-size: 11px; }}
window.lista #predchozi:hover, window.lista #dalsi:hover {{ color: {accent2}; }}
window.lista #pocasi {{ color: {accent2}; }}
window.lista #pocasi.stale {{ color: {dim}; }}
window.lista #tray {{ padding: 0 8px; }}
window.lista .tray-polozka {{ border-radius: 8px; }}
window.lista .tray-polozka.needs-attention {{ background: alpha({red}, 0.3); }}
window.lista #zvuk.muted, window.lista #sit.disconnected {{ color: {dim}; }}
window.lista #baterie.charging {{ color: {green}; }}
window.lista #baterie.warning {{ color: {yellow}; }}
window.lista #baterie.critical {{ color: {red}; }}
window.lista #notifikace.neprectene {{ color: {accent}; }}
window.lista #notifikace.dnd {{ color: {dim}; }}
window.lista #nahravani {{ color: {red}; font-weight: 700; }}
window.lista #nahravani:hover {{ background: alpha({red}, 0.18); }}
window.lista #mikrofon {{ color: {yellow}; }}
window.lista #aktualizace {{ color: {accent2}; }}
window.lista #pomodoro {{ color: {accent}; font-weight: 700; }}
window.lista #pomodoro.prestavka {{ color: {green}; }}
window.lista #pomodoro.pozastaveno {{ color: {dim}; }}
window.lista #vypnout {{ color: {red}; padding: 0 12px; }}
tooltip {{ background: {base}; border: 1px solid {surface}; border-radius: 12px; }}
tooltip label {{ color: {text}; padding: 2px; }}
/* bubliny notifikací (dřív mako) */
window.bubliny {{ font-family: "Adwaita Sans"; }}
window.bubliny .bublina {{ background: alpha({base}, 0.95); border: 1px solid {surface}; border-radius: 16px; padding: 14px 16px; }}
window.bubliny .bublina.kriticka {{ border: 2px solid {red}; }}
window.bubliny .bublina .aplikace {{ color: {dim}; font-size: 12px; }}
window.bubliny .bublina .nadpis {{ color: {text}; font-weight: 700; font-size: 14px; }}
window.bubliny .bublina .telo {{ color: {dim}; font-size: 13px; }}
window.bubliny levelbar trough {{ background: {mantle}; border-radius: 6px; min-height: 6px; border: none; padding: 0; }}
window.bubliny levelbar block.filled {{ background: {accent}; border-radius: 6px; border: none; }}
window.bubliny levelbar block.empty {{ background: transparent; border: none; }}
window.bubliny button.akce {{ background: {surface}; color: {text}; border: none; border-radius: 10px; padding: 4px 12px; box-shadow: none; }}
window.bubliny button.akce:hover {{ background: {overlay}; }}

/* nabídky aplikací z trayu */
popover.tray-nabidka > contents {{ background: alpha({base}, 0.96); border: 1px solid {surface}; border-radius: 14px; padding: 6px; box-shadow: none; }}
popover.tray-nabidka modelbutton {{ color: {text}; border-radius: 8px; padding: 6px 10px; }}
popover.tray-nabidka modelbutton:hover {{ background: {surface}; }}
popover.tray-nabidka modelbutton:disabled {{ color: alpha({dim}, 0.6); }}
popover.tray-nabidka separator {{ background: {surface}; margin: 4px 0; }}

/* aplikace Settings (nastaveni/): normální okno v barvách tématu */
window.nastaveni {{ background: {base}; color: {text}; }}
window.nastaveni headerbar {{ background: {mantle}; color: {text}; box-shadow: none; border-bottom: 1px solid {surface}; }}
window.nastaveni headerbar button {{ color: {text}; }}
window.nastaveni label {{ color: {text}; font-family: "Adwaita Sans"; font-size: 14px; }}
window.nastaveni .ikona {{ font-family: "Font Awesome 6 Free"; font-size: 15px; color: {accent2}; }}
window.nastaveni .postranni {{ background: {mantle}; padding: 14px 10px; border-right: 1px solid {surface}; }}
window.nastaveni list.stranky {{ background: transparent; }}
window.nastaveni list.stranky row {{ border-radius: 10px; padding: 9px 10px; margin: 1px 0; outline: none; }}
window.nastaveni list.stranky row:hover {{ background: alpha({surface}, 0.6); }}
window.nastaveni list.stranky row:selected {{ background: {surface}; }}
window.nastaveni .stranka {{ margin: 28px 32px 56px 32px; }}
window.nastaveni .nadpis-stranky {{ font-size: 24px; font-weight: 700; }}
window.nastaveni .nadpis-skupiny {{ font-size: 13px; font-weight: 700; color: {dim}; margin-left: 4px; }}
window.nastaveni label.popis {{ color: {dim}; font-size: 12px; }}
window.nastaveni list.karta {{ background: {mantle}; border: 1px solid {surface}; border-radius: 14px; }}
window.nastaveni list.karta > row {{ border-bottom: 1px solid alpha({surface}, 0.7); }}
window.nastaveni list.karta > row:last-child {{ border-bottom: none; }}
window.nastaveni list.karta > row:hover {{ background: transparent; }}
window.nastaveni .radek {{ padding: 12px 16px; min-height: 28px; }}
window.nastaveni button.radek-akce {{ background: transparent; border: none; box-shadow: none; padding: 0; border-radius: 0; outline: none; }}
window.nastaveni button.radek-akce:hover, window.nastaveni button.radek-akce:focus-visible {{ background: alpha({surface}, 0.55); }}
window.nastaveni button.radek-akce label {{ font-weight: 400; }}
window.nastaveni button.hlavni {{ background: {accent}; border: none; box-shadow: none; border-radius: 10px; padding: 5px 14px; }}
window.nastaveni button.hlavni label {{ color: {base}; font-weight: 700; }}
window.nastaveni button.vedlejsi {{ background: {surface}; border: none; box-shadow: none; border-radius: 10px; padding: 5px 14px; }}
window.nastaveni button.vedlejsi:hover {{ background: {overlay}; }}
window.nastaveni switch {{ background: {surface}; border: none; border-radius: 14px; outline: none; }}
window.nastaveni switch:checked {{ background: {accent}; }}
window.nastaveni switch slider {{ background: {text}; border-radius: 14px; border: none; min-width: 22px; min-height: 22px; box-shadow: none; }}
window.nastaveni scale trough {{ background: {surface}; border-radius: 8px; min-height: 6px; border: none; }}
window.nastaveni scale highlight {{ background: {accent}; border-radius: 8px; border: none; }}
window.nastaveni scale slider {{ background: {text}; border-radius: 100%; min-width: 16px; min-height: 16px; margin: -6px; border: none; box-shadow: none; outline: none; }}
window.nastaveni levelbar trough {{ background: {surface}; border-radius: 8px; min-height: 10px; border: none; padding: 0; }}
window.nastaveni levelbar block.filled {{ background: {accent}; border-radius: 8px; border: none; }}
window.nastaveni levelbar block.empty {{ background: transparent; border: none; }}
window.nastaveni entry, window.nastaveni searchentry, window.nastaveni passwordentry {{ background: {base}; color: {text};
    border: 1px solid {surface}; border-radius: 10px; min-height: 34px; box-shadow: none; outline: none; padding: 0 10px; }}
window.nastaveni entry:focus-within, window.nastaveni searchentry:focus-within, window.nastaveni passwordentry:focus-within {{ border-color: {accent}; }}
window.nastaveni dropdown > button {{ background: {surface}; border: none; box-shadow: none; border-radius: 10px; color: {text}; }}
window.nastaveni popover > contents {{ background: {mantle}; border: 1px solid {surface}; border-radius: 12px; box-shadow: none; }}
window.nastaveni popover row {{ border-radius: 8px; }}
window.nastaveni popover row:hover {{ background: {surface}; }}
window.nastaveni button.zpet {{ background: transparent; border: none; box-shadow: none; margin: 16px 0 0 24px; padding: 4px 10px; border-radius: 10px; }}
window.nastaveni button.zpet:hover {{ background: {surface}; }}
window.nastaveni button.zpet label {{ color: {accent}; font-weight: 600; }}
window.nastaveni button.tapeta {{ padding: 0; border-radius: 12px; border: 3px solid transparent; background: {surface}; box-shadow: none; }}
window.nastaveni button.tapeta.aktivni-tapeta {{ border-color: {accent}; }}
window.nastaveni button.tapeta:hover {{ border-color: alpha({accent}, 0.5); }}
window.nastaveni label.skupina-stranek {{ color: {dim}; font-size: 12px; font-weight: 700; margin: 12px 10px 4px 10px; }}
window.nastaveni picture.avatar {{ border-radius: 100%; background: {surface}; }}
window.nastaveni label.klavesy {{ background: {surface}; border-radius: 8px; padding: 3px 9px; font-size: 13px; font-weight: 600; }}
window.nastaveni label.potvrzeni {{ background: alpha({text}, 0.94); color: {base}; padding: 10px 18px; border-radius: 20px; font-weight: 600; }}

/* skoro průhledný proužek, který kurzor zachytí (spouštěcí proužek doku) */
.prouzek {{ background: rgba(0, 0, 0, 0.01); }}

/* přepínač oken Alt+Tab */
#prepinac {{
    background: alpha({base}, 0.95);
    border: 1px solid {surface};
    border-radius: 16px;
    padding: 20px 14px 16px 14px;
}}
#prepinac .nadpis-prepinace {{ color: {accent}; font-family: "Adwaita Sans"; font-size: 22px; margin: 0 10px 14px 10px; }}
#prepinac .radek-okna {{ border-radius: 6px; padding: 0 10px; }}
#prepinac .radek-okna.vybrany {{ background: {surface}; }}
#prepinac .radek-okna label {{ color: {text}; font-family: "Adwaita Sans"; font-size: 22px; }}
"#,
        base = b("base"), mantle = b("mantle"), surface = b("surface"), overlay = b("overlay"),
        text = b("text"), dim = b("dim"), accent = b("accent"), accent2 = b("accent2"), red = b("red"),
        green = b("green"), yellow = b("yellow"),
    )
}

/// Načte CSS pro celou obrazovku (nahradí předchozí, aby se neskládalo)
pub fn pouzit() {
    thread_local! {
        static POSKYTOVATEL: std::cell::RefCell<Option<gtk::CssProvider>> = const { std::cell::RefCell::new(None) };
    }
    let Some(displej) = gtk::gdk::Display::default() else { return };
    let paleta = Paleta::nacist();
    POSKYTOVATEL.with(|p| {
        let mut p = p.borrow_mut();
        if let Some(stary) = p.take() {
            gtk::style_context_remove_provider_for_display(&displej, &stary);
        }
        let novy = gtk::CssProvider::new();
        novy.load_from_string(&css(&paleta));
        gtk::style_context_add_provider_for_display(&displej, &novy, gtk::STYLE_PROVIDER_PRIORITY_USER);
        *p = Some(novy);
    });
}
