#!/bin/bash
# Papirus icons with folders in the wallpaper colour (Papirus-Wallpaper[-Dark], made by
# ~/.config/theme/theme.py), dark or light to match the style (Settings → Appearance → Style).
# Called at Sway login and after the wallpaper or style changes.
dark=$([ "$(gsettings get org.gnome.desktop.interface color-scheme)" = "'prefer-dark'" ] && echo 1)
if [ -d ~/.local/share/icons/Papirus-Wallpaper-Dark ]; then
    gsettings set org.gnome.desktop.interface icon-theme "Papirus-Wallpaper${dark:+-Dark}"
else
    gsettings set org.gnome.desktop.interface icon-theme "Papirus${dark:+-Dark}"
fi
