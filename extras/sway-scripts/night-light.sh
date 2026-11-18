#!/bin/bash
# Night light (wlsunset), configured in ~/.config/sway/night-light.conf (the Settings app edits it):
#   mode=sunset          warm light from sunset to sunrise
#   mode=always          warm light all the time
#   mode=off
#   temperature=4000     how warm (2500 = very orange, 6000 = almost neutral)
#   location=LAT,LON     for sunset and sunrise (Settings → Date & Time → Location)
#
#   night-light.sh          start / restart with the current settings (at login, after a change)
#   night-light.sh toggle   on / off (the button in the notification centre)
#   night-light.sh state    print true / false
CONF=~/.config/sway/night-light.conf
[ -f "$CONF" ] || printf 'mode=sunset\ntemperature=4000\n' > "$CONF"
get() { sed -n "s/^$1=//p" "$CONF"; }
set_key() { grep -q "^$1=" "$CONF" && sed -i "s/^$1=.*/$1=$2/" "$CONF" || echo "$1=$2" >> "$CONF"; }
mode=$(get mode); temperature=$(get temperature); location=$(get location)
mode=${mode:-sunset}; temperature=${temperature:-4000}

case "${1:-}" in
    state) pgrep -x wlsunset >/dev/null && echo true || echo false; exit 0 ;;
    toggle)
        if [ "$mode" = off ]; then
            last=$(get last); set_key mode "${last:-sunset}"
        else
            set_key last "$mode"; set_key mode off
        fi
        exec "$0" ;;
esac

pkill -x wlsunset
for _ in $(seq 20); do pgrep -x wlsunset >/dev/null || break; sleep 0.05; done
if [ -z "$location" ] && [ "$mode" = sunset ]; then
    # without a location wlsunset can't know when the sun sets: warm light from 20:00 to 7:00
    setsid wlsunset -S 07:00 -s 20:00 -t "$temperature" -T 6500 >/dev/null 2>&1 &
    exit 0
fi
lat=${location%,*}; lon=${location#*,}
case "$mode" in
    sunset) setsid wlsunset -l "$lat" -L "$lon" -t "$temperature" -T 6500 >/dev/null 2>&1 & ;;
    # wlsunset needs a day temperature above the night one; 1 K above = warm all the time
    always) setsid wlsunset -l "${lat:-0}" -L "${lon:-0}" -t "$temperature" -T $((temperature + 1)) >/dev/null 2>&1 & ;;
esac
exit 0
