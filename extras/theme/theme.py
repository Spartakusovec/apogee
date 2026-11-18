#!/usr/bin/python3 -I
"""
Colour theme from the wallpaper.

    theme.py <wallpaper> [<lock image>] [--preview]
    theme.py --gtk     only rewrite the GTK app colours (after switching dark / light)

Dark or light palette depending on org.gnome.desktop.interface color-scheme
(prefer-dark = dark, anything else = light; Settings → Appearance switches it).

It takes the dominant colours of the wallpaper, derives a palette from them (background,
surfaces, text, two accents, semantic colours) and fills the templates in
~/.config/theme/templates. TARGETS says where each template is written. --preview only prints
the palette and writes nothing.

Colours that don't come out of a wallpaper as you'd like can be overridden per wallpaper in
~/.config/theme/palettes/<wallpaper name without extension>.json, e.g. {"accent": "#a7c080"}.
Set THEME_MODE=light / dark to try a palette without switching the whole desktop.
"""

import colorsys
import json
import math
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from string import Template

HOME = Path.home()
TEMPLATES = HOME / ".config/theme/templates"
CUSTOM_PALETTES = HOME / ".config/theme/palettes"
TARGETS = {
    "sway-colors": HOME / ".config/sway/colors",
    "gtklock-colors.css": HOME / ".config/gtklock/colors.css",
    "fuzzel-colors.ini": HOME / ".config/fuzzel/colors.ini",
    "gtk4.css": HOME / ".config/gtk-4.0/gtk.css",
    "gtk3.css": HOME / ".config/gtk-3.0/gtk.css",
}
GTK = ("gtk4.css", "gtk3.css")

# GNOME / libadwaita named accent colours by hue (0–1)
GNOME_ACCENTS = [
    (0.03, "red"), (0.10, "orange"), (0.17, "yellow"), (0.42, "green"),
    (0.54, "teal"), (0.68, "blue"), (0.80, "purple"), (0.94, "pink"), (1.01, "red"),
]


def histogram(image):
    """Dominant colours of the image as [(count, (r, g, b))], channels 0–1."""
    out = subprocess.run(
        ["magick", f"{image}[0]", "-resize", "256x256", "-colors", "24",
         "-format", "%c", "histogram:info:-"],
        capture_output=True, text=True, check=True).stdout
    colors = []
    for line in out.splitlines():
        m = re.match(r"\s*(\d+):.*#([0-9A-Fa-f]{6})", line)
        if m:
            h = m.group(2)
            colors.append((int(m.group(1)), tuple(int(h[i:i + 2], 16) / 255 for i in (0, 2, 4))))
    return colors


def hls(h, l, s):
    r, g, b = colorsys.hls_to_rgb(h % 1, min(max(l, 0), 1), min(max(s, 0), 1))
    return "#%02x%02x%02x" % (round(r * 255), round(g * 255), round(b * 255))


def luminance(hexa):
    """Relative luminance (WCAG)."""
    def channel(c):
        c = int(c, 16) / 255
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (channel(hexa[i:i + 2]) for i in (1, 3, 5))
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a, b):
    la, lb = sorted((luminance(a), luminance(b)), reverse=True)
    return (la + 0.05) / (lb + 0.05)


def hue_distance(a, b):
    d = abs(a - b) % 1
    return min(d, 1 - d)


def readable(h, l, s, background, minimum=4.5):
    """Moves the lightness away from the background (lighter on dark, darker on light)
    until the colour has enough contrast."""
    step = 0.02 if luminance(background) < 0.3 else -0.02
    while contrast(hls(h, l, s), background) < minimum and 0.05 < l < 0.95:
        l += step
    return hls(h, l, s)


def light_mode():
    forced = os.environ.get("THEME_MODE")
    if forced in ("light", "dark"):
        return forced == "light"
    try:
        v = subprocess.run(["gsettings", "get", "org.gnome.desktop.interface", "color-scheme"], capture_output=True, text=True).stdout
    except OSError:
        return False
    return "prefer-dark" not in v


def palette(image):
    colors = histogram(image)
    total = sum(n for n, _ in colors)
    hls_colors = [(n / total, *colorsys.rgb_to_hls(*rgb)) for n, rgb in colors]   # (weight, h, l, s)

    # tint of the neutral colours: average hue weighted by amount and saturation
    x = sum(w * s * math.cos(2 * math.pi * h) for w, h, l, s in hls_colors)
    y = sum(w * s * math.sin(2 * math.pi * h) for w, h, l, s in hls_colors)
    hue = (math.atan2(y, x) / (2 * math.pi)) % 1
    tint = min(max(sum(w * s for w, h, l, s in hls_colors), 0.06), 0.22)

    light = light_mode()
    if light:
        p = {
            "mantle": hls(hue, 0.90, tint * 0.8),
            "base": hls(hue, 0.955, tint * 0.7),
            "surface": hls(hue, 0.85, tint * 0.8),
            "overlay": hls(hue, 0.62, tint * 0.7),
        }
        p["text"] = readable(hue, 0.18, min(tint, 0.25), p["base"], 10)
        p["dim"] = readable(hue, 0.42, tint * 0.6, p["base"], 4.5)
    else:
        p = {
            "mantle": hls(hue, 0.09, tint),
            "base": hls(hue, 0.125, tint),
            "surface": hls(hue, 0.19, tint * 0.9),
            "overlay": hls(hue, 0.42, tint * 0.7),
        }
        p["text"] = readable(hue, 0.86, min(tint, 0.25), p["base"], 10)
        p["dim"] = readable(hue, 0.62, tint * 0.6, p["base"], 4.5)
    # lightness of accents and semantic colours: pastel on dark, saturated and darker on light
    L = (lambda dark: 1.12 - dark) if light else (lambda dark: dark)

    # accent: the most striking colour of the wallpaper (saturated, not too dark or light, plentiful)
    candidates = sorted(
        ((math.sqrt(w) * s * (1 - abs(l - 0.55)), h, s) for w, h, l, s in hls_colors if s > 0.15 and 0.12 < l < 0.9),
        reverse=True)
    if candidates:
        _, h1, s1 = candidates[0]
        others = [c for c in candidates if hue_distance(c[1], h1) > 0.12]
        h2, s2 = (others[0][1], others[0][2]) if others else ((h1 + 0.11) % 1, s1)
    else:   # black-and-white wallpaper
        h1, s1, h2, s2 = hue, 0.3, (hue + 0.11) % 1, 0.3
    p["accent"] = readable(h1, L(0.70), min(max(s1, 0.40), 0.70), p["base"], 4.5 if light else 6)
    p["accent2"] = readable(h2, L(0.74), min(max(s2, 0.40), 0.70), p["base"], 4.5 if light else 6)

    # semantic colours, slightly towards the wallpaper's tint
    p["red"] = readable(0.985, L(0.70), 0.62, p["base"], 4.5 if light else 5)
    p["green"] = readable(0.25, L(0.68), 0.38, p["base"], 4.5 if light else 5)
    p["yellow"] = readable(0.11, L(0.74), 0.62, p["base"], 3.5 if light else 5)
    p["light"] = "true" if light else "false"

    custom = CUSTOM_PALETTES / (Path(image).stem + ".json")
    if custom.exists() and not light:   # hand-picked colours are tuned for the dark palette
        p.update(json.loads(custom.read_text()))

    accent_hls = colorsys.rgb_to_hls(*(int(p["accent"][i:i + 2], 16) / 255 for i in (1, 3, 5)))
    p["gnome_accent"] = "slate" if accent_hls[2] < 0.2 else next(n for limit, n in GNOME_ACCENTS if accent_hls[0] < limit)
    p.update(gtk_accent(accent_hls))
    p["papirus_color"] = folder_color(p["accent"])
    return p


def gtk_accent(accent_hls):
    """Accent for GTK apps: button background (a saturated shade of the wallpaper accent on which
    white text is readable) and the accent text colour for dark and light apps."""
    h, l, s = accent_hls
    s = min(max(s, 0.45), 0.75)
    white, black = "#ffffff", "#1e1e1e"
    background = hls(h, 0.50, s)
    while contrast(background, white) < 4.5 and contrast(background, black) < 7:
        background = hls(h, colorsys.rgb_to_hls(*(int(background[i:i + 2], 16) / 255 for i in (1, 3, 5)))[1] - 0.02, s)
    return {
        "gtk_accent": background,
        "gtk_accent_fg": white if contrast(background, white) >= 4.5 else black,
        "gtk_accent_on_dark": readable(h, 0.70, s, "#242424", 4.5),
        "gtk_accent_on_light": readable(h, 0.40, s, "#fafafa", 4.5),
    }


def write(p, wallpaper, lock_image):
    values = dict(p)
    values.update({k + "_hex": v.lstrip("#") for k, v in p.items() if v.startswith("#")})
    values["wallpaper"] = wallpaper
    values["lock_image"] = lock_image
    for name, target in TARGETS.items():
        if name in GTK or not (TEMPLATES / name).exists():
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(Template((TEMPLATES / name).read_text()).substitute(values))
    (HOME / ".config/theme/palette.json").write_text(json.dumps(p, indent=2) + "\n")
    write_gtk()
    write_firefox(values)
    folder_icons(p["papirus_color"])


# Papirus folder colours (papirus-folders) and their approximate shade
PAPIRUS_COLORS = {
    "blue": "#5294e2", "bluegrey": "#607d8b", "brown": "#ae8e6c", "carmine": "#a30002",
    "cyan": "#00bcd4", "darkcyan": "#45abb7", "deeporange": "#eb6637", "green": "#87b158",
    "grey": "#8e8e8e", "indigo": "#5c6bc0", "magenta": "#ca71df", "nordic": "#81a1c1",
    "orange": "#ee923a", "palebrown": "#d1bfae", "paleorange": "#eeca8f", "pink": "#f06292",
    "red": "#e57373", "teal": "#16a085", "violet": "#7e57c2", "yellow": "#f9bd30",
}
PAPIRUS = Path("/usr/share/icons/Papirus")
FOLDER_SIZES = {"22x22": "22x22", "24x24": "24x24", "32x32": "32x32", "48x48": "48x48",
                "64x64": "64x64", "96x96": "48x48", "128x128": "64x64"}


def folder_color(accent):
    def rgb(x):
        return [int(x[i:i + 2], 16) for i in (1, 3, 5)]
    a = rgb(accent)
    return min(PAPIRUS_COLORS, key=lambda n: sum((u - v) ** 2 for u, v in zip(a, rgb(PAPIRUS_COLORS[n]))))


def folder_icons(color):
    """Icon themes Papirus-Wallpaper-Dark and Papirus-Wallpaper: Papirus(-Dark) with folders in the
    wallpaper colour (copies of the folder icons from /usr/share/icons/Papirus; everything else
    is inherited). One set for all wallpapers (~2 MB), rewritten only when the colour changes."""
    if not PAPIRUS.is_dir():
        return
    icons = HOME / ".local/share/icons"
    marker = icons / "Papirus-Wallpaper-Dark/.color"
    if marker.exists() and marker.read_text() == color and (icons / "Papirus-Wallpaper").exists():
        return
    for name, base in (("Papirus-Wallpaper-Dark", "Papirus-Dark"), ("Papirus-Wallpaper", "Papirus")):
        target = icons / name
        if target.exists():
            shutil.rmtree(target)
        target.mkdir(parents=True)
        index = (Path("/usr/share/icons") / base / "index.theme").read_text()
        index = re.sub(r"(?m)^Name=.*$", "Name=" + name, index, count=1)
        index = re.sub(r"(?m)^Inherits=.*$", "Inherits=" + base + ",hicolor", index, count=1)
        (target / "index.theme").write_text(index)
        for size, source in FOLDER_SIZES.items():
            if name != "Papirus-Wallpaper-Dark":
                # the light set: same folders, linked to the dark one (relative, works in Flatpak too)
                for s in (size, size + "@2x"):
                    (target / s).mkdir()
                    (target / s / "places").symlink_to(Path("../../Papirus-Wallpaper-Dark") / size / "places")
                continue
            places = target / size / "places"
            places.mkdir(parents=True)
            (target / (size + "@2x")).symlink_to(size)
            prefix = f"folder-{color}"
            for f in (PAPIRUS / source / "places").iterdir():
                n = f.name
                if n == prefix + ".svg" or n.startswith(prefix + "-"):
                    new = "folder" + n[len(prefix):]
                elif n.startswith(f"user-{color}-"):
                    new = "user-" + n[len(f"user-{color}-"):]
                else:
                    continue
                shutil.copyfile(f.resolve(), places / new)
    marker.write_text(color)


def firefox_profiles():
    """Firefox profiles (Flatpak and RPM) that have a prefs.js, i.e. are really used."""
    for root in (HOME / ".var/app/org.mozilla.firefox/.mozilla/firefox", HOME / ".mozilla/firefox"):
        if root.is_dir():
            for d in root.iterdir():
                if (d / "prefs.js").exists():
                    yield d


def write_firefox(values):
    """userChrome.css with the palette; user.js turns on loading it (after a Firefox restart)."""
    if not (TEMPLATES / "firefox-userChrome.css").exists():
        return
    css = Template((TEMPLATES / "firefox-userChrome.css").read_text()).substitute(values)
    pref = 'user_pref("toolkit.legacyUserProfileCustomizations.stylesheets", true);'
    for profile in firefox_profiles():
        (profile / "chrome").mkdir(exist_ok=True)
        (profile / "chrome/userChrome.css").write_text(css)
        user_js = profile / "user.js"
        content = user_js.read_text() if user_js.exists() else ""
        if pref not in content:
            user_js.write_text(content + ("" if content.endswith("\n") or not content else "\n")
                               + "// colours from the wallpaper (~/.config/theme)\n" + pref + "\n")


def write_gtk():
    """Only the GTK app colours from the saved palette (after switching dark / light)."""
    p = json.loads((HOME / ".config/theme/palette.json").read_text())
    values = dict(p)
    values["gtk_accent_text"] = p.get("gtk_accent_on_light" if light_mode() else "gtk_accent_on_dark", p["accent"])
    for name in GTK:
        if not (TEMPLATES / name).exists():
            continue
        target = TARGETS[name]
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(Template((TEMPLATES / name).read_text()).substitute(values))


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    if sys.argv[1] == "--gtk":
        write_gtk()
        sys.exit(0)
    p = palette(sys.argv[1])
    if "--preview" in sys.argv:
        print(json.dumps(p, indent=2))
    else:
        lock_image = sys.argv[2] if len(sys.argv) > 2 and not sys.argv[2].startswith("--") else sys.argv[1]
        write(p, sys.argv[1], lock_image)
