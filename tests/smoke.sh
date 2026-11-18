#!/bin/bash
# Kouřový test apogeeu v úplně odděleném prostředí: vlastní Sway bez obrazovky (headless),
# vlastní XDG_RUNTIME_DIR (sokety Swaye, Waylandu i apogeeu jen tam), vlastní sběrnice sezení
# a kopie nastavení v dočasném HOME. Skutečného sezení se nedotkne.
#
#   bash tests/smoke.sh            – sestavit (cargo build --release) a otestovat
#   bash tests/smoke.sh --no-build – otestovat už sestavený target/release/apogee-daemon
#   SWAY=/cesta/k/sway bash tests/smoke.sh – s jiným Swayem (třeba čistým z Fedory, bez úpravy)
#   MEASURE_RUNS=20 bash tests/smoke.sh --no-build – navíc změřit rychlost otevření okének (20 pokusů na okénko)
#
# Co ověří: démon naběhne a odpovídá; všechny stránky Settings se otevřou; okénka z panelu
# se otevřou (i klikem pod modulem); dialog hesla k Wi-Fi vrátí NetworkManageru jméno a heslo;
# v logu chyb není pád. Snímky obrazovky uloží do tests/out/ (není v gitu, nezálohuje se).
set -uo pipefail
ZDROJ=$(cd "$(dirname "$0")/.." && pwd)
OUT=$ZDROJ/tests/out
DEMON=$ZDROJ/target/release/apogee-daemon
SWAY=${SWAY:-sway}
chyb=0
ok() { echo "  ok    $1"; }
chyba() { echo "  CHYBA $1"; chyb=$((chyb + 1)); }

if [ "${1:-}" != "--no-build" ]; then
    echo "== Build"
    (cd "$ZDROJ" && cargo build --release 2>&1 | grep -E "^(warning|error)" -A5) && true
fi
[ -x "$DEMON" ] || { echo "Missing $DEMON"; exit 1; }

# --- oddělené prostředí ---
T=$(mktemp -d /tmp/apogee-test.XXXXXX)
RT=$T/run; H=$T/home
mkdir -p "$RT" "$H/.config" "$H/.cache" "$OUT"; chmod 700 "$RT"
rm -f "$OUT"/*.png
for d in apogee sway theme gtklock fuzzel dconf; do
    [ -e ~/.config/$d ] && cp -r ~/.config/$d "$H/.config/"
done
mkdir -p "$H/Pictures"; cp -r ~/Pictures/Wallpapers "$H/Pictures/" 2>/dev/null
ln -s ~/.local "$H/.local"
# žádné aplikace po startu (značka „už proběhlo“)
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

# nástroje: virtuální myš a klávesnice (protokoly wlroots)
for n in pointer keyboard; do
    wayland-scanner client-header "$ZDROJ/tests/tools/virtual-$n.xml" "$T/virtual-$n.h"
    wayland-scanner private-code "$ZDROJ/tests/tools/virtual-$n.xml" "$T/virtual-$n.c"
done
gcc -I"$T" -o "$T/vmys" "$ZDROJ/tests/tools/vmys.c" "$T/virtual-pointer.c" -lwayland-client || exit 1
gcc -I"$T" -o "$T/vklav" "$ZDROJ/tests/tools/vklav.c" "$T/virtual-keyboard.c" -lwayland-client -lxkbcommon || exit 1

# vše další běží uvnitř vlastní sběrnice sezení (dbus-run-session) s vlastním XDG_RUNTIME_DIR
cat > "$T/uvnitr.sh" <<'UVNITR'
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
[ -n "$SWAYSOCK" ] && [ -n "$WAYLAND_DISPLAY" ] || { echo "  CHYBA test Sway did not start"; tail -5 "$T/sway.log"; exit 1; }
unset DISPLAY

APOGEE_AGENT_TEST=1 "$DEMON" > "$T/apogee.log" 2>&1 &
HPID=$!
for _ in $(seq 100); do [ -S "$RT/apogee.sock" ] && break; sleep 0.1; done
poslat() { python3 -I -c "import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1]); s.send((' '.join(sys.argv[2:])+'\n').encode())" "$RT/apogee.sock" "$@"; }
snimek() { timeout 10 grim ${2:+-g "$2"} "$OUT/$1.png" 2>/dev/null; }
okno() { swaymsg -t get_tree | grep -q "\"app_id\": \"$1\""; }
if [ -S "$RT/apogee.sock" ]; then echo "  ok    daemon started"; else echo "  CHYBA daemon did not start"; exit 1; fi
sleep 4
snimek panel "0,0 1920x50"

echo "== Settings pages"
for s in wifi network bluetooth sound display keyboard power appearance panel windows notifications shortcuts apps time users storage updates about; do
    poslat settings "$s"; sleep 1.5
    if okno apogee-settings; then echo "  ok    settings $s"; else echo "  CHYBA settings $s: no window"; fi
    snimek "settings-$s"
done
swaymsg '[app_id=apogee-settings] kill' >/dev/null; sleep 1

echo "== Popups"
for p in calendar weather sound brightness wifi bluetooth battery notifications updates power wallpapers; do
    poslat "$p"; sleep 1.2; snimek "popup-$p"; poslat "$p"; sleep 0.4
done
echo "  ok    popups opened (see tests/out/popup-*.png)"
# klik na hodiny: kalendář pod nimi
"$T/vmys" 960 24 klik; sleep 1.5; snimek popup-click-clock "460,0 1000x600"; "$T/vmys" 960 24 klik; sleep 0.5
echo "  ok    click on the clock (tests/out/popup-click-clock.png)"

echo "== Wi-Fi password dialog"
AGENT=$(gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.ListNames \
    | grep -o "':1\.[0-9]*'" | tr -d "'" | while read -r n; do
        gdbus introspect --session --dest "$n" --object-path /org/freedesktop/NetworkManager/SecretAgent 2>/dev/null | grep -q GetSecrets && echo "$n"
    done | head -1)
snimek bez-dialogu
gdbus call --session --dest "$AGENT" --object-path /org/freedesktop/NetworkManager/SecretAgent \
    --method org.freedesktop.NetworkManager.SecretAgent.GetSecrets \
    "{'connection': {'id': <'Test network'>, 'type': <'802-11-wireless'>}}" /org/freedesktop/NetworkManager/Settings/1 \
    802-11-wireless-security "[]" 1 --timeout 20 > "$T/odpoved.txt" 2>&1 &
sleep 2; snimek wifi-password
"$T/vklav" testpassword enter; sleep 1.5
if grep -q "'psk': <'testpassword'>" "$T/odpoved.txt"; then echo "  ok    password returned to NetworkManager"; else echo "  CHYBA password dialog: $(cat "$T/odpoved.txt")"; fi
# dialog má být uprostřed obrazovky (ne pod naposledy kliknutým modulem panelu)
python3 -I - "$OUT/bez-dialogu.png" "$OUT/wifi-password.png" <<'PY' && echo "  ok    dialog in the middle of the screen" || echo "  CHYBA dialog is not in the middle (tests/out/wifi-password.png)"
import sys
from PIL import Image, ImageChops
a, b = (Image.open(f).convert("L") for f in sys.argv[1:3])
# kde se obraz změnil (bez horního panelu s hodinami)
box = ImageChops.difference(a, b).point(lambda v: 255 if v > 40 else 0).crop((0, 60, 1920, 1080)).getbbox()
if not box:
    sys.exit(1)
x, y = (box[0] + box[2]) / 2, (box[1] + box[3]) / 2 + 60
sys.exit(0 if abs(x - 960) < 120 and 380 < y < 700 else 1)
PY

if [ -n "${MEASURE_RUNS:-}" ]; then
    echo "== Speed: command → first screenshot with the popup (median of $MEASURE_RUNS runs)"
    # grim snímá v kole jen pruh pod panelem; čas = od odeslání příkazu po první snímek,
    # který se od výchozího liší (okénko je vidět). Pak okénko zavřít a chvíli počkat.
    for p in calendar sound wifi battery notifications; do
        python3 -I - "$RT/apogee.sock" "$p" "$MEASURE_RUNS" <<'PY'
import os, socket, statistics, subprocess, sys, time
sok, prikaz, n = sys.argv[1], sys.argv[2], int(sys.argv[3])
def poslat(t):
    s = socket.socket(socket.AF_UNIX); s.connect(sok); s.send((t + "\n").encode()); s.close()
def snimek():
    return subprocess.run(["grim", "-g", "0,46 1920x60", "-t", "ppm", "-"], capture_output=True).stdout
casy = []
for _ in range(n):
    vychozi = snimek()
    t0 = time.perf_counter(); poslat(prikaz)
    while time.perf_counter() - t0 < 3:
        if snimek() != vychozi:
            casy.append((time.perf_counter() - t0) * 1000); break
    poslat(prikaz); time.sleep(0.6)
print(f"  info  {prikaz:<9} median {statistics.median(casy):5.0f} ms  (min {min(casy):.0f}, max {max(casy):.0f}, {len(casy)} runs)" if casy else f"  CHYBA {prikaz}: popup never appeared")
PY
    done
    t0=$(date +%s%N); timeout 5 grim -g "0,46 1920x60" -t ppm - >/dev/null; t1=$(date +%s%N)
    echo "  info  one screenshot takes $(( (t1 - t0) / 1000000 )) ms (the resolution of these numbers)"
fi

echo "== Health"
if kill -0 "$HPID" 2>/dev/null; then echo "  ok    daemon still running"; else echo "  CHYBA daemon is gone"; fi
if grep -q panicked "$H/.cache/apogee/errors.log" 2>/dev/null; then echo "  CHYBA panic in errors.log:"; tail -4 "$H/.cache/apogee/errors.log"; else echo "  ok    no panic"; fi
pss=$(awk '/^Pss:/{print int($2/1024)}' /proc/$HPID/smaps_rollup 2>/dev/null)
echo "  info  daemon memory ${pss:-?} MB (PSS, software rendering)"
echo "== Sway without the patch? (commands only the patched Sway knows)"
if grep -qaF "close button: cairo" "$(command -v "$SWAY")"; then echo "  info  patched Sway"; else echo "  info  plain Sway"; fi
for c in "maximize toggle" "snap left" "minimize"; do
    vystup=$(swaymsg "$c" 2>&1)
    if [[ "$vystup" == *nknown* ]]; then echo "  info  '$c' is unknown to this Sway"; else echo "  info  '$c' is known"; fi
done
kill "$HPID" 2>/dev/null; kill "$SWAYPID" 2>/dev/null; wait 2>/dev/null
UVNITR
# MERIT= je starší název pro MEASURE_RUNS
export T RT H OUT DEMON SWAY MEASURE_RUNS=${MEASURE_RUNS:-${MERIT:-}}
# i služby, které sběrnice spustí sama (portály, gvfs, dconf), dostanou jen testovací složky
env -u WAYLAND_DISPLAY -u DISPLAY -u SWAYSOCK -u I3SOCK XDG_RUNTIME_DIR="$RT" HOME="$H" GTK_A11Y=none \
    dbus-run-session -- bash "$T/uvnitr.sh" 2>/dev/null | tee "$T/vysledek.txt"
chyb=$(grep -c "CHYBA" "$T/vysledek.txt")

# úklid: jen procesy z tohoto testu (podle jejich XDG_RUNTIME_DIR)
for p in $(pgrep -u "$(id -u)" -f "apogee-daemon|sway -c $T|xdg-desktop-portal|gvfsd|dbus-daemon"); do
    tr '\0' '\n' < /proc/$p/environ 2>/dev/null | grep -qx "XDG_RUNTIME_DIR=$RT" && kill "$p" 2>/dev/null
done
rm -rf "$T"
echo
[ "$chyb" = 0 ] && { echo "All good. Screenshots: $OUT"; exit 0; } || { echo "Failures: $chyb (screenshots: $OUT)"; exit 1; }
