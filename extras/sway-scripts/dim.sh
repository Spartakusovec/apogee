#!/bin/bash
# Gradually dims the screen before idle locking (called by swayidle, see idle.sh).
#   dim.sh start     dim to a tenth over 20 s (only when no sound is playing)
#   dim.sh restore   mouse or key: stop dimming and bring the brightness back
SAVED="$XDG_RUNTIME_DIR/brightness-before-dim"
PID="$XDG_RUNTIME_DIR/dimming.pid"

case "${1:-}" in
    start)
        # not during a video (same as unless-playing.sh for locking)
        pactl list short sinks 2>/dev/null | grep -q RUNNING && exit 0
        [ -f "$SAVED" ] && exit 0
        now=$(brightnessctl get)
        echo "$now" > "$SAVED"
        target=$(( now / 10 )); [ "$target" -lt 1 ] && target=1
        (
            steps=40   # 40 × 0.5 s = 20 s
            for i in $(seq 1 $steps); do
                brightnessctl -q set $(( now - (now - target) * i / steps ))
                sleep 0.5
            done
        ) &
        echo $! > "$PID"
        ;;
    restore|obnovit)
        [ -f "$PID" ] && kill "$(cat "$PID")" 2>/dev/null
        rm -f "$PID"
        if [ -f "$SAVED" ]; then
            brightnessctl -q set "$(cat "$SAVED")"
            rm -f "$SAVED"
        fi
        ;;
esac
