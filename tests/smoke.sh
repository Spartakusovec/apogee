#!/bin/bash
# Smoke test of Apogee in a fully separate environment: its own headless Sway, its own
# XDG_RUNTIME_DIR (the Sway, Wayland and Apogee sockets live only there), its own session bus
# and a copy of the config in a temporary HOME. The real session is never touched.
#
#   bash tests/smoke.sh            – sestavit (cargo build --release) a otestovat
#   bash tests/smoke.sh --no-build – test the existing build in target/release
#   SWAY=/path/to/sway bash tests/smoke.sh – with another Sway (e.g. plain Fedora Sway, without the patch)
#   MEASURE_RUNS=20 bash tests/smoke.sh --no-build – also measure how fast popups open (20 runs each)
#
# What it checks: the daemon starts and answers; every Settings page opens; the bar popups
# open (also by clicking under a module); the Wi-Fi password dialog returns the password to
# NetworkManager; no crash in the error log. Screenshots go to tests/out/ (not in git).
set -uo pipefail
SRC=$(cd "$(dirname "$0")/.." && pwd)
OUT=$SRC/tests/out
DAEMON=$SRC/target/release/apogee-daemon
SWAY=${SWAY:-sway}
failures=0
ok() { echo "  ok    $1"; }
fail() { echo "  FAIL $1"; failures=$((chyb + 1)); }

if [ "${1:-}" != "--no-build" ]; then
    echo "== Build"
    (cd "$SRC" && cargo build --release 2>&1 | grep -E "^(warning|error)" -A5) && true
fi
[ -x "$DAEMON" ] || { echo "Missing $DAEMON"; exit 1; }

# --- separate environment ---
T=$(mktemp -d /tmp/apogee-test.XXXXXX)
RT=$T/run; H=$T/home
mkdir -p "$RT" "$H/.config" "$H/.cache" "$OUT"; chmod 700 "$RT"
rm -f "$OUT"/*.png
for d in apogee sway theme gtklock fuzzel dconf; do
    [ -e ~/.config/$d ] && cp -r ~/.config/$d "$H/.config/"
done
mkdir -p "$H/Pictures"; cp -r ~/Pictures/Wallpapers "$H/Pictures/" 2>/dev/null
ln -s ~/.local "$H/.local"
# no apps at login (the “already done” marker)
touch "$RT/apogee-autostart-done"

cat > "$T/sway.conf" <<EOF
include $H/.config/sway/colors
font pango:Adwaita Sans Semi-Bold 10
default_border normal 2
titlebar_padding 10 8
gaps inner 8
for_window [app_id=".*"] floating enable
output * mode --custom 1920x1080
EOF

# tools: virtual pointer and keyboard (wlroots protocols)
for n in pointer keyboard; do
    wayland-scanner client-header "$SRC/tests/tools/virtual-$n.xml" "$T/virtual-$n.h"
    wayland-scanner private-code "$SRC/tests/tools/virtual-$n.xml" "$T/virtual-$n.c"
done
gcc -I"$T" -o "$T/vpointer" "$SRC/tests/tools/vpointer.c" "$T/virtual-pointer.c" -lwayland-client || exit 1
gcc -I"$T" -o "$T/vkeyboard" "$SRC/tests/tools/vkeyboard.c" "$T/virtual-keyboard.c" -lwayland-client -lxkbcommon || exit 1

# everything else runs inside its own session bus (dbus-run-session) with its own XDG_RUNTIME_DIR
cat > "$T/inside.sh" <<'UVNITR'
set -uo pipefail
cd "$T"
env -u SWAYSOCK -u WAYLAND_DISPLAY -u DISPLAY XDG_RUNTIME_DIR="$RT" HOME="$H" \
    WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=pixman WLR_HEADLESS_OUTPUTS=1 \
    "$SWAY" -c "$T/sway.conf" > "$T/sway.log" 2>&1 &
SWAYPID=$!
for _ in $(seq 50); do ls "$RT"/sway-ipc.*.sock >/dev/null 2>&1 && break; sleep 0.1; done
export XDG_RUNTIME_DIR=$RT HOME=$H
export SWAYSOCK=$(ls "$RT"/sway-ipc.*.sock 2>/dev/null | head -1)
export WAYLAND_DISPLAY=$(ls "$RT" | grep -E '^wayland-[0-9]+$' | head -1)
[ -n "$SWAYSOCK" ] && [ -n "$WAYLAND_DISPLAY" ] || { echo "  FAIL test Sway did not start"; tail -5 "$T/sway.log"; exit 1; }
unset DISPLAY

APOGEE_AGENT_TEST=1 "$DAEMON" > "$T/apogee.log" 2>&1 &
HPID=$!
for _ in $(seq 100); do [ -S "$RT/apogee.sock" ] && break; sleep 0.1; done
send() { python3 -I -c "import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); s.send((' '.join(sys.argv[2:])+'\n').encode())" "$RT/apogee.sock" "$@"; }
shot() { timeout 10 grim ${2:+-g "$2"} "$OUT/$1.png" 2>/dev/null; }
has_window() { swaymsg -t get_tree | grep -q "\"app_id\": \"$1\""; }
if [ -S "$RT/apogee.sock" ]; then echo "  ok    daemon started"; else echo "  FAIL daemon did not start"; exit 1; fi
sleep 4
shot panel "0,0 1920x50"
python3 -I -c "import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d.get('format')==2 and not set(d)&{'dok','hodiny','poloha','panel_skryte','rozlozeni'} else 1)" "$H/.config/apogee/settings.json" \
    && echo "  ok    settings.json in the current format" || echo "  FAIL settings.json was not migrated"
cp "$H/.config/apogee/settings.json" "$OUT/settings-after-migration.json" 2>/dev/null

echo "== Settings pages"
for s in wifi network bluetooth sound display keyboard power appearance panel windows notifications shortcuts apps time users storage updates about; do
    send settings "$s"; sleep 1.5
    if has_window apogee-settings; then echo "  ok    settings $s"; else echo "  FAIL settings $s: no window"; fi
    shot "settings-$s"
done
swaymsg '[app_id=apogee-settings] kill' >/dev/null; sleep 1

echo "== Popups"
for p in calendar weather sound brightness wifi bluetooth battery notifications updates power wallpapers; do
    send "$p"; sleep 1.2; shot "popup-$p"; send "$p"; sleep 0.4
done
echo "  ok    popups opened (see tests/out/popup-*.png)"
# click on the clock: the calendar under it
"$T/vpointer" 960 24 click; sleep 1.5; shot popup-click-clock "460,0 1000x600"; "$T/vpointer" 960 24 click; sleep 0.5
echo "  ok    click on the clock (tests/out/popup-click-clock.png)"

echo "== Wi-Fi password dialog"
AGENT=$(gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.ListNames \
    | grep -o "':1\.[0-9]*'" | tr -d "'" | while read -r n; do
        gdbus introspect --session --dest "$n" --object-path /org/freedesktop/NetworkManager/SecretAgent 2>/dev/null | grep -q GetSecrets && echo "$n"
    done | head -1)
shot no-dialog
gdbus call --session --dest "$AGENT" --object-path /org/freedesktop/NetworkManager/SecretAgent \
    --method org.freedesktop.NetworkManager.SecretAgent.GetSecrets \
    "{'connection': {'id': <'Test network'>, 'type': <'802-11-wireless'>}}" /org/freedesktop/NetworkManager/Settings/1 \
    802-11-wireless-security "[]" 1 --timeout 20 > "$T/reply.txt" 2>&1 &
sleep 2; shot wifi-password
"$T/vkeyboard" testpassword enter; sleep 1.5
if grep -q "'psk': <'testpassword'>" "$T/reply.txt"; then echo "  ok    password returned to NetworkManager"; else echo "  FAIL password dialog: $(cat "$T/reply.txt")"; fi
# the dialog must be in the middle of the screen (not under the last clicked bar module)
python3 -I - "$OUT/no-dialog.png" "$OUT/wifi-password.png" <<'PY' && echo "  ok    dialog in the middle of the screen" || echo "  FAIL dialog is not in the middle (tests/out/wifi-password.png)"
import sys
from PIL import Image, ImageChops
a, b = (Image.open(f).convert("L") for f in sys.argv[1:3])
# where the picture changed (without the top bar and its clock)
box = ImageChops.difference(a, b).point(lambda v: 255 if v > 40 else 0).crop((0, 60, 1920, 1080)).getbbox()
if not box:
    sys.exit(1)
x, y = (box[0] + box[2]) / 2, (box[1] + box[3]) / 2 + 60
sys.exit(0 if abs(x - 960) < 120 and 380 < y < 700 else 1)
PY

if [ -n "${MEASURE_RUNS:-}" ]; then
    echo "== Speed: command → first screenshot with the popup (median of $MEASURE_RUNS runs)"
    # grim captures only the strip under the bar in a loop; time = from sending the command to the
    # first capture that differs from the starting one (the popup is visible). Then close and wait.
    for p in calendar sound wifi battery notifications; do
        python3 -I - "$RT/apogee.sock" "$p" "$MEASURE_RUNS" <<'PY'
import os, socket, statistics, subprocess, sys, time
sock, cmd, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
def send(t):
    s = socket.socket(socket.AF_UNIX); s.connect(sock); s.send((t + "\n").encode()); s.close()
def shot():
    return subprocess.run(["grim", "-g", "0,46 1920x60", "-t", "ppm", "-"], capture_output=True).stdout
times = []
for _ in range(n):
    before = shot()
    t0 = time.perf_counter(); send(cmd)
    while time.perf_counter() - t0 < 3:
        if shot() != before:
            times.append((time.perf_counter() - t0) * 1000); break
    send(cmd); time.sleep(0.6)
print(f"  info  {cmd:<9} median {statistics.median(times):5.0f} ms  (min {min(times):.0f}, max {max(times):.0f}, {len(times)} runs)" if times else f"  FAIL {cmd}: popup never appeared")
PY
    done
    t0=$(date +%s%N); timeout 5 grim -g "0,46 1920x60" -t ppm - >/dev/null; t1=$(date +%s%N)
    echo "  info  one screenshot takes $(( (t1 - t0) / 1000000 )) ms (the resolution of these numbers)"
fi

echo "== Health"
if kill -0 "$HPID" 2>/dev/null; then echo "  ok    daemon still running"; else echo "  FAIL daemon is gone"; fi
if grep -q panicked "$H/.cache/apogee/errors.log" 2>/dev/null; then echo "  FAIL panic in errors.log:"; tail -4 "$H/.cache/apogee/errors.log"; else echo "  ok    no panic"; fi
pss=$(awk '/^Pss:/{print int($2/1024)}' /proc/$HPID/smaps_rollup 2>/dev/null)
echo "  info  daemon memory ${pss:-?} MB (PSS, software rendering)"
echo "== Sway without the patch? (commands only the patched Sway knows)"
if grep -qaF "close button: cairo" "$(command -v "$SWAY")"; then echo "  info  patched Sway"; else echo "  info  plain Sway"; fi
for c in "maximize toggle" "snap left" "minimize"; do
    out=$(swaymsg "$c" 2>&1)
    if [[ "$out" == *nknown* ]]; then echo "  info  '$c' is unknown to this Sway"; else echo "  info  '$c' is known"; fi
done
kill "$HPID" 2>/dev/null; kill "$SWAYPID" 2>/dev/null; wait 2>/dev/null
UVNITR
# MERIT= is the older name of MEASURE_RUNS
export T RT H OUT DAEMON SWAY MEASURE_RUNS=${MEASURE_RUNS:-${MERIT:-}}
# services the bus starts by itself (portals, gvfs, dconf) also get only the test folders
env -u WAYLAND_DISPLAY -u DISPLAY -u SWAYSOCK -u I3SOCK XDG_RUNTIME_DIR="$RT" HOME="$H" GTK_A11Y=none \
    dbus-run-session -- bash "$T/inside.sh" 2>/dev/null | tee "$T/result.txt"
failures=$(grep -c "FAIL" "$T/result.txt")

# clean-up: only processes of this test (by their XDG_RUNTIME_DIR)
for p in $(pgrep -u "$(id -u)" -f "apogee-daemon|sway -c $T|xdg-desktop-portal|gvfsd|dbus-daemon"); do
    tr '\0' '\n' < /proc/$p/environ 2>/dev/null | grep -qx "XDG_RUNTIME_DIR=$RT" && kill "$p" 2>/dev/null
done
rm -rf "$T"
echo
[ "$failures" = 0 ] && { echo "All good. Screenshots: $OUT"; exit 0; } || { echo "Failures: $failures (screenshots: $OUT)"; exit 1; }
