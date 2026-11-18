//! Colours from the theme (~/.config/theme/palette.json, written by set-wallpaper) and the popups' CSS.
//! The palette is read every time a popup opens, so a wallpaper change shows at once.

use std::collections::HashMap;

pub struct Palette(HashMap<String, String>);

impl Palette {
    pub fn load() -> Palette {
        let default = [
            ("base", "#1e1e2e"), ("mantle", "#181825"), ("surface", "#313244"), ("overlay", "#6c7086"),
            ("text", "#cdd6f4"), ("dim", "#9399b2"), ("accent", "#89b4fa"), ("accent2", "#f5c2e7"),
            ("red", "#f38ba8"), ("green", "#a6e3a1"), ("yellow", "#f9e2af"),
        ];
        let mut colors: HashMap<String, String> =
            default.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let file = crate::common::home().join(".config/theme/palette.json");
        if let Ok(text) = std::fs::read_to_string(file) {
            if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&text) {
                for (k, v) in map {
                    if let Some(s) = v.as_str() {
                        if s.starts_with('#') {
                            colors.insert(k, s.to_string());
                        }
                    }
                }
            }
        }
        Palette(colors)
    }

    pub fn color(&self, name: &str) -> &str {
        self.0.get(name).map(String::as_str).unwrap_or("#888888")
    }

    /// #rrggbb → (r, g, b) 0–1 for drawing
    pub fn rgb(&self, name: &str) -> (f64, f64, f64) {
        let h = self.color(name).trim_start_matches('#');
        let kanal = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("88"), 16).unwrap_or(136) as f64 / 255.0;
        (kanal(0), kanal(2), kanal(4))
    }
}

pub fn css(p: &Palette) -> String {
    let b = |n| p.color(n);
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
#panel .title {{ font-size: 16px; font-weight: 700; }}
#panel .dim {{ color: {dim}; font-size: 12px; }}
#panel .icon {{ font-family: "Font Awesome 6 Free"; font-size: 16px; color: {accent2}; }}
#panel .value {{ min-width: 46px; }}
#panel scale trough {{ background: {mantle}; border-radius: 8px; min-height: 8px; border: none; }}
#panel scale highlight {{ background: {accent}; border-radius: 8px; border: none; }}
#panel scale slider {{ background: {text}; border-radius: 100%; min-width: 16px; min-height: 16px;
                       margin: -5px; border: none; box-shadow: none; outline: none; }}
#panel button {{ background: {surface}; border: none; border-radius: 10px; padding: 6px 12px;
                 box-shadow: none; color: {text}; outline: none; }}
#panel button:hover {{ background: {overlay}; }}
#panel button.enabled {{ background: {accent}; }}
#panel button.enabled label {{ color: {base}; }}
#panel button.row {{ background: transparent; padding: 7px 10px; }}
#panel button.row:hover, #panel button.row:focus-visible {{ background: {surface}; }}
#panel button.link {{ background: transparent; padding: 4px 6px; }}
#panel button.link label {{ color: {accent}; font-size: 13px; }}
#panel switch {{ background: {mantle}; border: none; border-radius: 14px; outline: none; }}
#panel switch:checked {{ background: {accent}; }}
#panel switch slider {{ background: {text}; border-radius: 14px; border: none; min-width: 22px; min-height: 22px; box-shadow: none; }}
#panel separator {{ background: {surface}; margin: 12px 0; min-height: 1px; }}
#panel .right {{ color: {dim}; font-size: 12px; }}
#panel .active label {{ color: {accent}; font-weight: 700; }}
#panel entry {{ background: {mantle}; color: {text}; border: 1px solid {surface};
                border-radius: 10px; padding: 6px 10px; caret-color: {accent}; box-shadow: none; outline: none; }}
#panel entry:focus-within {{ border-color: {accent}; }}
#panel .large {{ font-size: 30px; font-weight: 700; }}
#panel .giant {{ font-size: 44px; font-weight: 300; }}
#panel .weather-icon {{ font-family: "Font Awesome 6 Free"; font-size: 34px; color: {accent2}; }}
#panel .small-icon {{ font-family: "Font Awesome 6 Free"; font-size: 15px; color: {accent2}; }}
#panel .dest {{ color: {accent}; font-size: 11px; }}
#panel levelbar trough {{ background: {mantle}; border-radius: 8px; min-height: 10px; border: none; padding: 0; }}
#panel levelbar block.filled {{ background: {accent}; border-radius: 8px; border: none; }}
#panel levelbar block.empty {{ background: transparent; border: none; }}
#panel levelbar.low block.filled {{ background: {red}; }}
#panel .day {{ min-width: 38px; min-height: 34px; border-radius: 10px; }}
#panel .day.other-month {{ color: alpha({dim}, 0.45); }}
#panel .day.weekend {{ color: {accent2}; }}
#panel .day.today {{ background: {accent}; color: {base}; font-weight: 700; }}
#panel .day-header {{ color: {dim}; font-size: 12px; }}
#panel .week {{ color: alpha({dim}, 0.6); font-size: 11px; min-width: 26px; }}
#panel button.arrow {{ background: transparent; padding: 2px 10px; }}
#panel button.arrow:hover {{ background: {surface}; }}
#panel button.quick {{ min-height: 42px; padding: 0; }}
#panel button.quick label {{ font-family: "Font Awesome 6 Free"; font-size: 16px; }}
#panel button.quick-icon label {{ font-family: "Font Awesome 6 Free"; font-size: 15px; }}
#panel .notifications {{ background: alpha({surface}, 0.55); border-radius: 12px; padding: 10px 12px; }}
#panel .notifications .title-notifications {{ font-weight: 700; font-size: 13px; }}
#panel .notifications .body {{ color: {dim}; font-size: 13px; }}
#panel button.close {{ background: transparent; padding: 0 4px; min-height: 0; }}
#panel button.close label {{ color: {dim}; font-size: 12px; font-family: "Font Awesome 6 Free"; }}
#panel button.close:hover label {{ color: {red}; }}
#panel .wrapper {{ background: {surface}; border-radius: 12px; }}
#panel .wrapper-fallback {{ font-family: "Font Awesome 6 Free"; font-size: 34px; color: {dim}; }}
#panel .title-track {{ font-size: 16px; font-weight: 700; }}
#panel .small-time {{ color: {dim}; font-size: 11px; }}
#panel button.controls {{ background: transparent; min-width: 44px; min-height: 44px; padding: 0; border-radius: 22px; }}
#panel button.controls:hover {{ background: {surface}; }}
#panel button.controls label {{ font-family: "Font Awesome 6 Free"; font-size: 17px; }}
#panel button.controls:disabled label {{ color: alpha({dim}, 0.5); }}
#panel button.controls.main {{ background: {accent}; min-width: 54px; min-height: 54px; border-radius: 27px; }}
#panel button.controls.main:hover {{ background: alpha({accent}, 0.85); }}
#panel button.controls.main label {{ color: {base}; font-size: 20px; }}
#panel button.main-action {{ background: {accent}; min-height: 38px; border-radius: 12px; }}
#panel button.main-action:hover {{ background: alpha({accent}, 0.85); }}
#panel button.main-action label {{ color: {base}; font-weight: 700; }}
#panel button.wallpaper {{ padding: 0; border: 2px solid transparent; border-radius: 12px; background: {mantle}; }}
#panel button.wallpaper:hover {{ border-color: {overlay}; }}
#panel button.wallpaper.active-wallpaper {{ border-color: {accent}; }}
#panel entry {{ min-height: 0; }}

#panel .osd {{ padding: 12px 20px; }}
#panel list.clipboard {{ background: transparent; }}
#panel list.clipboard row {{ border-radius: 10px; padding: 7px 10px; outline: none; }}
#panel list.clipboard row:hover {{ background: alpha({surface}, 0.6); }}
#panel list.clipboard row:selected {{ background: {surface}; }}

/* dok */
#dock {{ background: alpha({base}, 0.88); border: 1px solid alpha({surface}, 0.9); border-radius: 20px;
        padding: 5px 8px 3px 8px; }}
#dock button.dock-item {{ background: none; border: none; box-shadow: none; outline: none;
        padding: 4px 4px 2px 4px; margin: 0 2px; border-radius: 14px; transition: background 150ms ease; }}
#dock button.dock-item:hover {{ background: {surface}; }}
#dock .dots {{ min-height: 6px; margin-top: 2px; }}
#dock .dot {{ min-width: 5px; min-height: 5px; border-radius: 3px; margin: 0 1px; background: {dim}; }}
#dock button.active .dot {{ background: {accent}; }}
/* minimized window: just a ring */
#dock .dot.hidden {{ background: transparent; box-shadow: inset 0 0 0 1px {dim}; }}
#dock .dock-grid .square {{ min-width: 7px; min-height: 7px; border-radius: 3px; background: {text}; }}
#dock .dock-grid .square.center {{ background: {accent}; }}
popover.dock-menu > contents {{ background: transparent; border: none; box-shadow: none; padding: 0; }}
popover.dock-menu #panel {{ padding: 8px; }}

/* app grid */
window.launcher .dim {{ background: rgba(0, 0, 0, 0.45); }}
window.launcher #panel {{ padding: 20px 22px; }}
window.launcher flowbox.apps {{ background: transparent; }}
window.launcher flowbox.apps flowboxchild {{ border-radius: 14px; padding: 12px 4px; outline: none; }}
window.launcher flowbox.apps flowboxchild:hover {{ background: alpha({surface}, 0.6); }}
window.launcher flowbox.apps flowboxchild:selected {{ background: {surface}; }}
window.launcher .apps-tiling label {{ color: {text}; font-family: "Adwaita Sans"; font-size: 13px; }}

/* spotlight */
window.spotlight #panel {{ padding: 12px 14px; border-radius: 22px; }}
window.spotlight .spotlight-field {{ padding: 6px 10px; }}
window.spotlight .spotlight-zoom {{ font-family: "Font Awesome 6 Free"; font-size: 22px; color: {dim}; }}
window.spotlight entry {{ background: transparent; border: none; box-shadow: none; outline: none;
        font-size: 24px; min-height: 48px; padding: 0; color: {text}; }}
window.spotlight list.spotlight-results {{ background: transparent; margin-top: 6px; }}
window.spotlight list.spotlight-results row {{ border-radius: 12px; padding: 8px 12px; outline: none; }}
window.spotlight list.spotlight-results row.group {{ padding: 8px 10px 2px 10px; }}
window.spotlight list.spotlight-results row:hover {{ background: alpha({surface}, 0.6); }}
window.spotlight list.spotlight-results row:selected {{ background: {surface}; }}
window.spotlight .result {{ font-size: 17px; }}
window.spotlight list.spotlight-results .dim {{ font-size: 13px; }}
window.spotlight .spotlight-icon {{ font-family: "Font Awesome 6 Free"; font-size: 24px; color: {accent2}; }}

/* top bar (was waybar) */
window.bar {{ background: transparent; color: {text};
        font-family: "Adwaita Sans", "Font Awesome 6 Free", "Font Awesome 6 Brands", sans-serif; font-size: 13px; }}
window.bar .island {{ background: alpha({base}, 0.88); border: 1px solid alpha({surface}, 0.9); border-radius: 14px; padding: 3px; }}
window.bar .module, window.bar .workspace {{ padding: 0 11px; margin: 0 1px; border-radius: 11px; min-height: 24px;
        color: {text}; transition: background 150ms ease, color 150ms ease; }}
window.bar .clickable:hover, window.bar .workspace:hover {{ background: {surface}; }}
window.bar #apps {{ font-weight: 700; padding: 0 12px 0 13px; }}
window.bar #tiling {{ color: {dim}; }}
window.bar #tiling.tiling {{ background: alpha({accent}, 0.18); color: {accent}; }}
window.bar .workspace {{ padding: 0 9px; min-width: 18px; color: {dim}; }}
window.bar .workspace.empty {{ color: alpha({dim}, 0.55); }}
window.bar .workspace.active {{ background: alpha({accent}, 0.18); color: {accent}; font-weight: 700; }}
window.bar .workspace.urgent {{ background: {red}; color: {base}; }}
window.bar #mode {{ background: {yellow}; color: {base}; font-weight: 700; }}
window.bar #clock {{ font-weight: 700; letter-spacing: 0.3px; }}
window.bar #music {{ color: {accent2}; margin: 0; }}
window.bar #music.paused {{ color: {dim}; }}
window.bar #previous, window.bar #next {{ padding: 0 9px; color: {dim}; font-size: 11px; }}
window.bar #previous:hover, window.bar #next:hover {{ color: {accent2}; }}
window.bar #weather {{ color: {accent2}; }}
window.bar #weather.stale {{ color: {dim}; }}
window.bar #tray {{ padding: 0 8px; }}
window.bar .tray-item {{ border-radius: 8px; }}
window.bar .tray-item.needs-attention {{ background: alpha({red}, 0.3); }}
window.bar #sound.muted, window.bar #network.disconnected {{ color: {dim}; }}
window.bar #battery.charging {{ color: {green}; }}
window.bar #battery.warning {{ color: {yellow}; }}
window.bar #battery.critical {{ color: {red}; }}
window.bar #notifications.unread {{ color: {accent}; }}
window.bar #notifications.dnd {{ color: {dim}; }}
window.bar #recording {{ color: {red}; font-weight: 700; }}
window.bar #recording:hover {{ background: alpha({red}, 0.18); }}
window.bar #microphone {{ color: {yellow}; }}
window.bar #updates {{ color: {accent2}; }}
window.bar #pomodoro {{ color: {accent}; font-weight: 700; }}
window.bar #pomodoro.break_time {{ color: {green}; }}
window.bar #pomodoro.paused {{ color: {dim}; }}
window.bar #power {{ color: {red}; padding: 0 12px; }}
tooltip {{ background: {base}; border: 1px solid {surface}; border-radius: 12px; }}
tooltip label {{ color: {text}; padding: 2px; }}
/* notification bubbles (were mako) */
window.bubbles {{ font-family: "Adwaita Sans"; }}
window.bubbles .bubble {{ background: alpha({base}, 0.95); border: 1px solid {surface}; border-radius: 16px; padding: 14px 16px; }}
window.bubbles .bubble.critical {{ border: 2px solid {red}; }}
window.bubbles .bubble .apps {{ color: {dim}; font-size: 12px; }}
window.bubbles .bubble .title {{ color: {text}; font-weight: 700; font-size: 14px; }}
window.bubbles .bubble .body {{ color: {dim}; font-size: 13px; }}
window.bubbles levelbar trough {{ background: {mantle}; border-radius: 6px; min-height: 6px; border: none; padding: 0; }}
window.bubbles levelbar block.filled {{ background: {accent}; border-radius: 6px; border: none; }}
window.bubbles levelbar block.empty {{ background: transparent; border: none; }}
window.bubbles button.action {{ background: {surface}; color: {text}; border: none; border-radius: 10px; padding: 4px 12px; box-shadow: none; }}
window.bubbles button.action:hover {{ background: {overlay}; }}

/* app menus from the tray */
popover.tray-menu > contents {{ background: alpha({base}, 0.96); border: 1px solid {surface}; border-radius: 14px; padding: 6px; box-shadow: none; }}
popover.tray-menu modelbutton {{ color: {text}; border-radius: 8px; padding: 6px 10px; }}
popover.tray-menu modelbutton:hover {{ background: {surface}; }}
popover.tray-menu modelbutton:disabled {{ color: alpha({dim}, 0.6); }}
popover.tray-menu separator {{ background: {surface}; margin: 4px 0; }}

/* the Settings app (settings/): a normal window in the theme colours */
window.settings {{ background: {base}; color: {text}; }}
window.settings headerbar {{ background: {mantle}; color: {text}; box-shadow: none; border-bottom: 1px solid {surface}; }}
window.settings headerbar button {{ color: {text}; }}
window.settings label {{ color: {text}; font-family: "Adwaita Sans"; font-size: 14px; }}
window.settings .icon {{ font-family: "Font Awesome 6 Free"; font-size: 15px; color: {accent2}; }}
window.settings .sidebar {{ background: {mantle}; padding: 14px 10px; border-right: 1px solid {surface}; }}
window.settings list.pages {{ background: transparent; }}
window.settings list.pages row {{ border-radius: 10px; padding: 9px 10px; margin: 1px 0; outline: none; }}
window.settings list.pages row:hover {{ background: alpha({surface}, 0.6); }}
window.settings list.pages row:selected {{ background: {surface}; }}
window.settings .page {{ margin: 28px 32px 56px 32px; }}
window.settings .title-pages {{ font-size: 24px; font-weight: 700; }}
window.settings .title-groups {{ font-size: 13px; font-weight: 700; color: {dim}; margin-left: 4px; }}
window.settings label.description {{ color: {dim}; font-size: 12px; }}
window.settings list.card {{ background: {mantle}; border: 1px solid {surface}; border-radius: 14px; }}
window.settings list.card > row {{ border-bottom: 1px solid alpha({surface}, 0.7); }}
window.settings list.card > row:last-child {{ border-bottom: none; }}
window.settings list.card > row:hover {{ background: transparent; }}
window.settings .row {{ padding: 12px 16px; min-height: 28px; }}
window.settings button.row-action {{ background: transparent; border: none; box-shadow: none; padding: 0; border-radius: 0; outline: none; }}
window.settings button.row-action:hover, window.settings button.row-action:focus-visible {{ background: alpha({surface}, 0.55); }}
window.settings button.row-action label {{ font-weight: 400; }}
window.settings button.main {{ background: {accent}; border: none; box-shadow: none; border-radius: 10px; padding: 5px 14px; }}
window.settings button.main label {{ color: {base}; font-weight: 700; }}
window.settings button.secondary {{ background: {surface}; border: none; box-shadow: none; border-radius: 10px; padding: 5px 14px; }}
window.settings button.secondary:hover {{ background: {overlay}; }}
window.settings switch {{ background: {surface}; border: none; border-radius: 14px; outline: none; }}
window.settings switch:checked {{ background: {accent}; }}
window.settings switch slider {{ background: {text}; border-radius: 14px; border: none; min-width: 22px; min-height: 22px; box-shadow: none; }}
window.settings scale trough {{ background: {surface}; border-radius: 8px; min-height: 6px; border: none; }}
window.settings scale highlight {{ background: {accent}; border-radius: 8px; border: none; }}
window.settings scale slider {{ background: {text}; border-radius: 100%; min-width: 16px; min-height: 16px; margin: -6px; border: none; box-shadow: none; outline: none; }}
window.settings levelbar trough {{ background: {surface}; border-radius: 8px; min-height: 10px; border: none; padding: 0; }}
window.settings levelbar block.filled {{ background: {accent}; border-radius: 8px; border: none; }}
window.settings levelbar block.empty {{ background: transparent; border: none; }}
window.settings entry, window.settings searchentry, window.settings passwordentry {{ background: {base}; color: {text};
    border: 1px solid {surface}; border-radius: 10px; min-height: 34px; box-shadow: none; outline: none; padding: 0 10px; }}
window.settings entry:focus-within, window.settings searchentry:focus-within, window.settings passwordentry:focus-within {{ border-color: {accent}; }}
window.settings dropdown > button {{ background: {surface}; border: none; box-shadow: none; border-radius: 10px; color: {text}; }}
window.settings popover > contents {{ background: {mantle}; border: 1px solid {surface}; border-radius: 12px; box-shadow: none; }}
window.settings popover row {{ border-radius: 8px; }}
window.settings popover row:hover {{ background: {surface}; }}
window.settings button.back {{ background: transparent; border: none; box-shadow: none; margin: 16px 0 0 24px; padding: 4px 10px; border-radius: 10px; }}
window.settings button.back:hover {{ background: {surface}; }}
window.settings button.back label {{ color: {accent}; font-weight: 600; }}
window.settings button.wallpaper {{ padding: 0; border-radius: 12px; border: 3px solid transparent; background: {surface}; box-shadow: none; }}
window.settings button.wallpaper.active-wallpaper {{ border-color: {accent}; }}
window.settings button.wallpaper:hover {{ border-color: alpha({accent}, 0.5); }}
window.settings label.group-pages {{ color: {dim}; font-size: 12px; font-weight: 700; margin: 12px 10px 4px 10px; }}
window.settings picture.avatar {{ border-radius: 100%; background: {surface}; }}
window.settings label.keys {{ background: {surface}; border-radius: 8px; padding: 3px 9px; font-size: 13px; font-weight: 600; }}
window.settings label.toast {{ background: alpha({text}, 0.94); color: {base}; padding: 10px 18px; border-radius: 20px; font-weight: 600; }}

/* almost transparent strip the cursor can hit (the dock's trigger strip) */
.strip {{ background: rgba(0, 0, 0, 0.01); }}

/* Alt+Tab window switcher */
#switcher {{
    background: alpha({base}, 0.95);
    border: 1px solid {surface};
    border-radius: 16px;
    padding: 20px 14px 16px 14px;
}}
#switcher .title-switcher {{ color: {accent}; font-family: "Adwaita Sans"; font-size: 22px; margin: 0 10px 14px 10px; }}
#switcher .row-windows {{ border-radius: 6px; padding: 0 10px; }}
#switcher .row-windows.selected {{ background: {surface}; }}
#switcher .row-windows label {{ color: {text}; font-family: "Adwaita Sans"; font-size: 22px; }}
"#,
        base = b("base"), mantle = b("mantle"), surface = b("surface"), overlay = b("overlay"),
        text = b("text"), dim = b("dim"), accent = b("accent"), accent2 = b("accent2"), red = b("red"),
        green = b("green"), yellow = b("yellow"),
    )
}

/// Loads the CSS for the whole screen (replaces the previous one so they don't stack)
pub fn apply() {
    thread_local! {
        static PROVIDER: std::cell::RefCell<Option<gtk::CssProvider>> = const { std::cell::RefCell::new(None) };
    }
    let Some(display) = gtk::gdk::Display::default() else { return };
    let palette = Palette::load();
    PROVIDER.with(|p| {
        let mut p = p.borrow_mut();
        if let Some(stary) = p.take() {
            gtk::style_context_remove_provider_for_display(&display, &stary);
        }
        let new = gtk::CssProvider::new();
        new.load_from_string(&css(&palette));
        gtk::style_context_add_provider_for_display(&display, &new, gtk::STYLE_PROVIDER_PRIORITY_USER);
        *p = Some(new);
    });
}
