#!/usr/bin/env python3
# Throwaway test client: an ordinary opaque window standing in for "the application underneath".
import argparse, gi
gi.require_version('Gtk', '3.0'); gi.require_version('Gdk', '3.0')
from gi.repository import Gtk, Gdk, GLib
ap = argparse.ArgumentParser(); ap.add_argument('--title'); ap.add_argument('--log'); a = ap.parse_args()
def log(msg):
    with open(a.log, 'a') as f: f.write(msg + '\n')
win = Gtk.Window(); win.set_title(a.title); win.set_default_size(1200, 800)
area = Gtk.DrawingArea(); area.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
def draw(w, cr):
    cr.set_source_rgb(0.85, 0.9, 0.85); cr.paint()
    cr.set_source_rgb(0.2, 0.2, 0.2)
    for i in range(0, 1200, 40): cr.rectangle(i, 0, 2, 800)
    cr.fill(); return False
area.connect('draw', draw)
area.connect('button-press-event', lambda w, e: (log(f'click {a.title} win={int(e.x)},{int(e.y)}'), True)[1])
win.connect('focus-in-event', lambda w, e: (log(f'focus-in {a.title}'), False)[1])
win.connect('focus-out-event', lambda w, e: (log(f'focus-out {a.title}'), False)[1])
win.add(area); win.show_all(); log(f'mapped {a.title} backend={type(Gdk.Display.get_default()).__name__}')
GLib.timeout_add_seconds(180, Gtk.main_quit); Gtk.main()
