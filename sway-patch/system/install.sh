#!/bin/bash
# Fedora: turns on the automatic rebuild of Sway with the window-actions patch and runs it now.
#   sudo bash system/install.sh
# Turn off:  sudo rm /etc/dnf/libdnf5-plugins/actions.d/sway-patch.actions
#            sudo systemctl disable --now sway-patch-check.timer
# Then `sudo dnf distro-sync sway` brings back Fedora's plain Sway.
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Run with sudo." >&2; exit 1; }
Z=$(dirname "$(readlink -f "$0")")
dnf -y install libdnf5-plugin-actions rpm-build

# the user who gets the "rebuilding Sway" notifications
echo "USER=${SUDO_USER:-$(logname 2>/dev/null || true)}" > /etc/sway-patch.conf

# older versions were called sway-krizek: take over their state and remove the old parts
if [ -e /usr/local/libexec/sway-krizek ] || [ -e /var/lib/sway-krizek ]; then
    echo "Replacing the old sway-krizek parts…"
    if [ -f /var/lib/sway-krizek/stav ] && [ ! -f /var/lib/sway-patch/state ]; then
        install -Dm644 /var/lib/sway-krizek/stav /var/lib/sway-patch/state
    fi
    systemctl disable --now sway-krizek-kontrola.timer 2>/dev/null || true
    rm -f /etc/systemd/system/sway-krizek-kontrola.service /etc/systemd/system/sway-krizek-kontrola.timer \
          /etc/dnf/libdnf5-plugins/actions.d/sway-krizek.actions /usr/local/libexec/sway-krizek
    rm -rf /usr/local/share/sway-krizek /var/lib/sway-krizek /var/cache/sway-krizek
fi
install -Dm755 "$Z/sway-patch" /usr/local/libexec/sway-patch
rm -f /usr/local/share/sway-patch/sway-buttons.patch
install -Dm644 "$Z/../sway-window-actions.patch" /usr/local/share/sway-patch/sway-window-actions.patch
install -Dm644 "$Z/sway-patch.actions" /etc/dnf/libdnf5-plugins/actions.d/sway-patch.actions
install -Dm644 "$Z/sway-patch-check.service" /etc/systemd/system/sway-patch-check.service
install -Dm644 "$Z/sway-patch-check.timer" /etc/systemd/system/sway-patch-check.timer
systemctl daemon-reload
systemctl enable --now sway-patch-check.timer
echo
echo "Automatic rebuild is on. Checking Sway now (it's rebuilt only when the patch is missing or changed; that takes a few minutes)…"
/usr/local/libexec/sway-patch
echo
rpm -q sway
grep -qaF "close button: cairo" /usr/bin/sway && echo "Sway has the title bar buttons. Log out and back in." || echo "Something went wrong, see /var/cache/sway-patch/build.log"
