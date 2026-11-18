#!/bin/bash
# Builds Fedora's Sway with the window-actions patch as RPMs, by hand (the automatic way is
# system/install.sh). Downloads the Sway source package Fedora has now, adds the patch, bumps the
# release (e.g. 4 → 4.1, so dnf sees it as newer) and builds. Prints the install command at the end.
# If the patch doesn't apply to a new Sway version, it stops with an error: then the patch has to
# be ported by hand.
#   bash build.sh
# Needs: sudo dnf install rpm-build && sudo dnf builddep sway
set -euo pipefail
cd "$(dirname "$0")"
HERE=$PWD
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
(cd "$WORK" && dnf download --source sway >/dev/null)
SRPM=$(ls "$WORK"/sway-*.src.rpm)
echo "Fedora: $(basename "$SRPM")"
mkdir -p "$WORK/srpm"
(cd "$WORK/srpm" && rpm2cpio "$SRPM" | cpio -idm --quiet)
cp "$HERE/sway-window-actions.patch" "$WORK/srpm/"
python3 - "$WORK/srpm/sway.spec" <<'PY'
import re, sys, time
p = sys.argv[1]
s = open(p).read()
s = re.sub(r"^(Release:\s*)(\d+)(%\{\?dist\})", lambda m: f"{m.group(1)}{m.group(2)}.1{m.group(3)}", s, count=1, flags=re.M)
s = re.sub(r"^(Source\d+:.*\n)(?!Source)", r"\1# title bar buttons, maximize, snapping, minimize (Apogee)\nPatch99:        sway-window-actions.patch\n", s, count=1, flags=re.M)
s = s.replace("#autopatch -p1 -M99", "%autopatch -p1 -M99", 1)
ver = re.search(r"^Version:\s*(\S+)", s, re.M).group(1)
rel = re.search(r"^Release:\s*(\S+?)%", s, re.M).group(1)
s = s.replace("%changelog\n", f"%changelog\n* {time.strftime('%a %b %d %Y')} Apogee <apogee@localhost> - {ver}-{rel}\n- Title bar buttons, maximize, snapping, minimize\n\n", 1)
open(p, "w").write(s)
PY
rm -rf "$HERE/rpmbuild"
rpmbuild -bb --define "_sourcedir $WORK/srpm" --define "_topdir $HERE/rpmbuild" "$WORK/srpm/sway.spec" > "$HERE/build.log" 2>&1 || {
    echo "Build failed, see $HERE/build.log (end):"
    tail -20 "$HERE/build.log"
    exit 1
}
echo
echo "Done. Install (then log out and back in):"
pkgs=()
for b in sway sway-config-upstream sway-wallpapers; do
    rpm -q "$b" >/dev/null 2>&1 || continue
    pkgs+=("$(ls "$HERE"/rpmbuild/RPMS/*/"$b"-[0-9]*.rpm)")
done
echo "  sudo dnf install ${pkgs[*]}"
