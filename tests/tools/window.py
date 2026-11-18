# A small GTK 3 window without its own title bar, so Sway draws the title bar (with the patch: buttons).
import sys, gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, GLib
GLib.set_prgname("testwindow")
w = Gtk.Window(title=sys.argv[1] if len(sys.argv) > 1 else "Test window")
w.set_default_size(600, 400)
w.add(Gtk.Label(label="Apogee smoke test"))
w.connect("destroy", Gtk.main_quit)
w.show_all()
Gtk.main()
