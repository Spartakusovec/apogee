#!/bin/bash
# Idle handling (swayidle), configured in ~/.config/sway/idle.conf (the Settings app edits it):
#   lock=600         lock after this many seconds (0 = never); the screen dims 20 s before
#   screen_off=900   turn the screen off after this many seconds (0 = never)
# Nothing happens while sound or video is playing (unless-playing.sh, dim.sh).
# Started from the Sway config; Settings starts it again after a change.
S=~/.config/sway/scripts
CONF=~/.config/sway/idle.conf
get() { sed -n "s/^$1=//p" "$CONF" 2>/dev/null; }
lock=$(get lock); screen_off=$(get screen_off)
lock=${lock:-600}; screen_off=${screen_off:-900}
pkill -x swayidle
args=(-w)
if [ "$lock" -gt 20 ]; then
    args+=(timeout $((lock - 20)) "$S/dim.sh start" resume "$S/dim.sh restore")
    args+=(timeout "$lock" "$S/unless-playing.sh gtklock -d")
fi
if [ "$screen_off" -gt 0 ]; then
    args+=(timeout "$screen_off" "$S/unless-playing.sh swaymsg 'output * power off'" resume "swaymsg 'output * power on'")
fi
args+=(before-sleep "gtklock -d")
exec swayidle "${args[@]}"
