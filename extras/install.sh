#!/bin/bash
# Installs the companion pieces Apogee expects: the wallpaper → colour theme generator,
# set-wallpaper and the small Sway scripts (night light, power modes, idle, apogee-watch).
#   bash extras/install.sh
# Existing files are kept as <name>.before-apogee. Needs: python3, ImageMagick (magick), jq,
# wlsunset, swayidle, gtklock, brightnessctl, Papirus icons (optional, for folder colours).
set -euo pipefail
E=$(cd "$(dirname "$0")" && pwd)
put() { # source target mode
    mkdir -p "$(dirname "$2")"
    if [ -e "$2" ] && ! cmp -s "$1" "$2"; then cp "$2" "$2.before-apogee"; fi
    install -m "$3" "$1" "$2"
}
for f in "$E"/sway-scripts/*.sh; do put "$f" ~/.config/sway/scripts/"$(basename "$f")" 755; done
put "$E/theme/theme.py" ~/.config/theme/theme.py 755
for f in "$E"/theme/templates/*; do put "$f" ~/.config/theme/templates/"$(basename "$f")" 644; done
put "$E/bin/set-wallpaper" ~/.local/bin/set-wallpaper 755
mkdir -p ~/.config/theme/palettes

cat <<'MSG'
Installed. Add this to your Sway config (~/.config/sway/config) if it isn't there yet:

    set $scripts ~/.config/sway/scripts
    include ~/.config/sway/colors                 # colours from the wallpaper (set-wallpaper writes it)
    exec $scripts/apogee-watch.sh                 # Apogee, restarted if it ever crashes
    exec $scripts/night-light.sh
    exec $scripts/idle.sh
    exec $scripts/icons.sh
    include ~/.config/sway/settings.conf          # last: what the Settings app changes

Then pick a wallpaper:  set-wallpaper ~/Pictures/Wallpapers/something.jpg
MSG
