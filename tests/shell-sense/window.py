#!/usr/bin/env python3
"""Test only: an ordinary window with a given title."""
import sys, gi
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GLib
win = Gtk.Window(); win.set_title(sys.argv[1]); win.set_default_size(700, 500); win.add(Gtk.Label(label=sys.argv[1])); win.show_all()
GLib.timeout_add_seconds(120, Gtk.main_quit); Gtk.main()
