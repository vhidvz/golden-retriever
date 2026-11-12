#!/usr/bin/env python3
# Throwaway test client: a 400x300 transparent window whose only clickable area is a 100x100 square.
import argparse, gi
gi.require_version('Gtk', '3.0'); gi.require_version('Gdk', '3.0')
from gi.repository import Gtk, Gdk, GLib
import cairo
ap = argparse.ArgumentParser()
ap.add_argument('--title'); ap.add_argument('--log'); ap.add_argument('--popup', action='store_true')
ap.add_argument('--above', action='store_true'); ap.add_argument('--x', type=int, default=300); ap.add_argument('--y', type=int, default=200)
a = ap.parse_args()
W, H = 400, 300; RX, RY, RW, RH = 150, 100, 100, 100
def log(msg):
    with open(a.log, 'a') as f: f.write(msg + '\n')
win = Gtk.Window(type=Gtk.WindowType.POPUP if a.popup else Gtk.WindowType.TOPLEVEL)
win.set_title(a.title); win.set_decorated(False); win.set_default_size(W, H); win.set_app_paintable(True)
win.set_accept_focus(False); win.set_focus_on_map(False); win.set_skip_taskbar_hint(True); win.set_skip_pager_hint(True)
scr = win.get_screen(); vis = scr.get_rgba_visual()
if vis: win.set_visual(vis)
def draw(w, cr):
    cr.set_operator(cairo.OPERATOR_SOURCE); cr.set_source_rgba(0, 0, 0, 0); cr.paint()
    cr.set_source_rgba(0.95, 0.6, 0.1, 1.0); cr.rectangle(RX, RY, RW, RH); cr.fill()      # the "dog": opaque, clickable
    cr.set_source_rgba(0.1, 0.3, 0.9, 0.5); cr.rectangle(0, 0, 60, 60); cr.fill()          # translucent marker, not clickable
    return False
win.connect('draw', draw)
win.add_events(Gdk.EventMask.BUTTON_PRESS_MASK)
win.connect('button-press-event', lambda w, e: (log(f'click {a.title} win={int(e.x)},{int(e.y)}'), True)[1])
win.connect('focus-in-event', lambda w, e: (log(f'focus-in {a.title}'), False)[1])
win.connect('realize', lambda w: w.input_shape_combine_region(cairo.Region(cairo.RectangleInt(RX, RY, RW, RH))))
if a.popup or a.above: win.move(a.x, a.y)
if a.above: win.set_keep_above(True); win.stick()
win.show_all()
if a.popup or a.above: win.move(a.x, a.y)
log(f'mapped {a.title} backend={type(Gdk.Display.get_default()).__name__} rgba={vis is not None} composited={scr.is_composited()}')
GLib.timeout_add_seconds(90, Gtk.main_quit); Gtk.main()
