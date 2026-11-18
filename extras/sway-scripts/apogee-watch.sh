#!/bin/bash
# Keeps Apogee (bar, dock, popups, notifications, Wi-Fi passwords) running for the Sway session:
# if it crashes, it is started again within a second. It runs inside the Sway session on purpose,
# not as a systemd service: there Apogee would lose its logind session (pkexec and backlight
# control would stop working), and a restart would kill the apps launched from the dock.
# Start it from the Sway config; it ends by itself when Sway ends.
#   pkill -x apogee-daemon   restart Apogee (e.g. after installing a new version)
LOG=~/.cache/apogee/errors.log
mkdir -p "$(dirname "$LOG")"
crashes=()
while swaymsg -t get_version >/dev/null 2>&1; do
    # --wait: if Apogee is already running (started some other way), take over when it ends
    ~/.local/bin/apogee-daemon --wait
    code=$?
    swaymsg -t get_version >/dev/null 2>&1 || break
    echo "$(date '+%F %T') apogee-watch: apogee ended (exit $code), starting it again" >> "$LOG"
    # too many crashes in a row (5 per minute): wait a bit instead of looping
    now=$(date +%s); crashes=("${crashes[@]}" "$now"); crashes=("${crashes[@]: -5}")
    if [ "${#crashes[@]}" -ge 5 ] && [ $((now - crashes[0])) -lt 60 ]; then
        notify-send -a apogee "The panel keeps crashing" "Trying again in 30 seconds. Details: ~/.cache/apogee/errors.log" 2>/dev/null
        sleep 30
    else
        sleep 1
    fi
done
