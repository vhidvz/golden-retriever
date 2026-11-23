#!/usr/bin/env python3
"""A tiny GTK4 app with an accessible button, label and text field.

Run it in the graphical session, then use the other probes in this directory to
inspect its AT-SPI tree, invoke the button and watch semantic events. It exists only
for the workflow-accessibility spike; Buddy never starts it.
"""
import gi

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk


class App(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="local.buddy.A11yProbe")

    def do_activate(self):
        window = Gtk.ApplicationWindow(application=self, title="Buddy A11y Probe")
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.label = Gtk.Label(label="ready")
        self.button = Gtk.Button(label="Press me")
        self.button.connect("clicked", self.on_press)
        self.entry = Gtk.Entry()
        box.append(self.label)
        box.append(self.button)
        box.append(self.entry)
        window.set_child(box)
        window.present()

    def on_press(self, _button):
        self.label.set_label("pressed")


App().run([])
