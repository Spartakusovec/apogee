# window-actions: a Sway patch for title bar buttons, maximize, snapping and minimize

Plain Sway has no buttons in window title bars, no maximize and no minimize. This patch
(`sway-window-actions.patch`, ~1,150 added lines of C, against **Sway 1.11**) adds them, so
floating windows behave like on Windows, macOS or GNOME:

![Title bar with minimize, maximize and close buttons](../docs/screenshots/titlebar.png)

- **minimize, maximize / restore and close buttons** in Sway's title bars, drawn by Sway itself
  (circles in the title text colour, highlighted under the pointer);
- **double-click** a title bar to maximize / restore;
- **edge snapping**: drag a window to the left or right edge for half the screen, to the top edge
  for the whole screen, with a translucent preview; drag it away to restore;
- **commands** for key bindings: `maximize [enable|disable|toggle]`, `minimize`, `snap left|right`;
- **minimize** sends a window to the scratchpad but remembers its workspace (and whether it was
  tiled), so the dock or Alt+Tab bring it back where it was;
- **app title bars work too**: GTK and other apps with their own title bar get working minimize
  and maximize buttons (Sway now answers `set_maximized` / `set_minimized`), and they don't get
  a second title bar from Sway when tiled;
- **new floating windows fit the workspace** (e.g. Firefox no longer opens under the bar), and
  windows that ask to open maximized are maximized;
- two **IPC tick events** for a shell such as Apogee: `window-menu` (right click on an app's
  title bar) and `window-drag` (a window is being dragged, at most 10× a second).

Tiling is unchanged: tiled windows show only minimize and close, and the other commands do
nothing there or fall back to Sway's normal behaviour (`snap left` = `focus left`).

## Status

Used daily on Fedora 44 with Sway 1.11. Upstream Sway doesn't want title bar buttons, so this
stays a downstream patch that has to follow Sway releases. If it stops applying to a new
version, Sway without the patch keeps working; only these features are missing until the patch
is updated. The Apogee smoke test checks both cases (`SWAY=… bash tests/smoke.sh`) and clicks
the buttons with a virtual pointer.

## Fedora: automatic

```bash
sudo bash sway-patch/system/install.sh
```

This installs a small root script, a dnf hook and a boot-time timer. Whenever Fedora updates
Sway, Fedora's own source package is rebuilt with the patch (a few minutes, with a notification
at the start and the end); log out and back in afterwards. The rebuilt package has a higher
release number (e.g. `1.11-3.1.fc44`), so it doesn't get replaced until Fedora ships a newer Sway.
`install.sh` also says how to turn it off and go back to Fedora's Sway.

By hand instead: `bash sway-patch/build.sh` builds the RPMs and prints the install command.

## Other distributions

Apply the patch to the Sway 1.11 source and build it as usual:

```bash
curl -LO https://github.com/swaywm/sway/releases/download/1.11/sway-1.11.tar.gz
tar xzf sway-1.11.tar.gz && cd sway-1.11
patch -p1 < /path/to/sway-window-actions.patch
meson setup build && ninja -C build && sudo ninja -C build install
```

## Example key bindings

```
bindsym $mod+Up    maximize enable
bindsym $mod+Down  maximize disable
bindsym $mod+Left  snap left
bindsym $mod+Right snap right
bindsym $mod+h     minimize
```

Licence: MIT, like Sway.
