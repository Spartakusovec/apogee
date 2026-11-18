# Settings app: design notes

One app for everything that would otherwise need a terminal. Everything on screen is in English.

## How it behaves

- **Part of Apogee** (same daemon, same colours from the wallpaper): `apogee settings [page]`.
  The window is built on first use; each page is built when you switch to it, so it always
  shows the current state. Data is loaded in the background.
- **A normal window**, not a popup: it can be moved, maximized and minimized.
- **Where it opens from:** the power menu (Settings), Spotlight (“wifi”, “bluetooth”, “sound”…
  open the right page), and every bar popup has a “… settings” link to its page. The popups stay
  for quick things; the app is for everything else.
- **Changes apply immediately**, no Apply button. Exception: resolution and scale ask “Keep these
  settings?” for 15 seconds and go back otherwise (like GNOME).
- **Almost never a terminal:** things that need root (battery conservation mode, time zone,
  password, firmware) ask for the password through polkit. System updates still open a terminal
  to show progress.
- **Search** in the sidebar matches page names and keywords (English, plus Czech synonyms so
  Czech words find pages too). Enter jumps to the first match. Ctrl+F focuses the search.
- **Where settings are stored:** where they belong – NetworkManager, BlueZ, gsettings,
  `~/.config/apogee/settings.json`, and for Sway a generated `~/.config/sway/settings.conf`
  that the main Sway config includes last.
- **Feedback:** a short toast at the bottom of the window (“Connected to Home Wi-Fi”, “Wrong password”).

## How it looks

- About 920 × 640, resizable. Sidebar (~230 px) with search and pages in four groups; the page on
  the right.
- GNOME-like: rounded cards with rows “name and description on the left, control on the right”
  (switch, slider, drop-down, arrow to a subpage). Page title at the top, space between cards.

## Pages

**Connections** – Wi-Fi (incl. eduroam / WPA-Enterprise, details with IP, DNS, password, metered),
Network (wired, VPN import, hotspot, airplane mode), Bluetooth.

**Device** – Sound (devices, volume, per-app volume), Display (brightness, night light, resolution,
scale, external monitor), Keyboard & Touchpad (layouts, repeat, touchpad, mouse, gestures),
Power & Battery (power mode, battery health, conservation mode, idle and lock times).

**Personalization** – Appearance (wallpaper, dark / light, text size, window buttons, pointer size),
Panel & Dock (bar modules, clock format, dock hiding, pinned apps), Windows & Workspaces (tiling or
floating by default, gaps, focus follows mouse), Notifications (Do Not Disturb with a schedule,
muted apps), Shortcuts (searchable list).

**System** – Apps (default apps, apps after login), Date & Time (time zone, automatic time,
location, regional formats), Users (picture, name, password), Storage (usage, clean-up),
Updates (system, Flatpak, firmware, the patched Sway), About (hardware, software, backup).

## Code

`src/settings/`: `mod.rs` (window, sidebar, search, toast), `ui.rs` (page, group, row helpers),
`sway_conf.rs` (the generated Sway settings file), one file per page.

## Ideas for later

Editing keyboard shortcuts, arranging monitors by dragging, printers, accessibility (high
contrast, zoom), screen sharing settings, installing updates in the window with progress.
