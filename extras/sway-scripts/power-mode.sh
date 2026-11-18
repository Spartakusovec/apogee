#!/bin/bash
# Power mode through the power-profiles D-Bus interface (tuned-ppd or power-profiles-daemon):
#   power-mode.sh          switch to the next one (power-saver → balanced → performance)
#   power-mode.sh <mode>   set power-saver / balanced / performance
#   power-mode.sh state    print the current mode
BUS=(net.hadess.PowerProfiles /net/hadess/PowerProfiles net.hadess.PowerProfiles ActiveProfile)
now=$(busctl get-property "${BUS[@]}" | cut -d'"' -f2)
case "${1:-}" in
    state) echo "$now"; exit 0 ;;
    power-saver|balanced|performance) new=$1 ;;
    "") case "$now" in power-saver) new=balanced ;; balanced) new=performance ;; *) new=power-saver ;; esac ;;
    *) echo "Usage: power-mode.sh [power-saver|balanced|performance|state]"; exit 1 ;;
esac
busctl set-property "${BUS[@]}" s "$new"
case "$new" in
    power-saver) text="Power saver – longer battery life" ;;
    balanced)    text="Balanced" ;;
    performance) text="Performance – faster, uses more battery" ;;
esac
notify-send -a osd -i battery -t 2000 "Power mode: $text"
