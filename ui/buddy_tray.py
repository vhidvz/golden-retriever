#!/usr/bin/env python3
"""Buddy's OS tray icon and custom notification panel.

The icon is a standard StatusNotifierItem (SNI), so GNOME (AppIndicator
extension), KDE and other trays put it where their own tray lives. The panel
that opens from it is Buddy's own GTK UI: no desktop notification service, no
toast daemon, no OS-specific notification API. It reads the same daemon state
as the CLI and sends every action back as a normal daemon command.
"""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

try:
    import cairo
except ImportError:  # the panel already moves off-screen; this is an extra guard
    cairo = None
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("GdkX11", "4.0")
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import Gdk, GdkPixbuf, GdkX11, Gio, GLib, Gtk  # noqa: E402

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from buddy_ui import Api, ApiError  # noqa: E402

ICON = HERE / "buddy.svg"
CSS = HERE / "buddy_tray.css"
APP_ID = "local.buddy.Tray"
SNI_BUS = "org.kde.StatusNotifierItem"
SNI_PATH = "/StatusNotifierItem"
SNI_IFACE = "org.kde.StatusNotifierItem"
WATCHER_BUS = "org.kde.StatusNotifierWatcher"
WATCHER_PATH = "/StatusNotifierWatcher"
PANEL_WIDTH = 372
PANEL_HEIGHT = 450
POLL_SECONDS = 1.25

NEEDS = {
    "hunger": ("My bowl is looking empty", "Could we get some dinner? I will sit nicely.", "package-x-generic-symbolic", 0.66, 20 * 60, "food_out", "feed", "Feed him"),
    "thirst": ("A fresh bowl of water?", "My tongue is getting a little dry.", "weather-showers-symbolic", 0.66, 15 * 60, "water_out", "water", "Fresh water"),
    "boredom": ("I could use a game", "I have been good. Could we play for a minute?", "media-playback-start-symbolic", 0.70, 20 * 60, None, "play", "Let's play"),
    "social": ("Do you have a minute?", "I would like to be near you for a little while.", "emblem-favorite-symbolic", 0.72, 20 * 60, None, "pet", "Sit with him"),
    "fatigue": ("My eyes are getting heavy", "I might curl up for a little while.", "weather-clear-night-symbolic", 0.78, 60 * 60, None, "sleep", "Let him nap"),
}


def _doing(activity: str) -> str:
    return {"lie": "lying down", "sit": "sitting", "stand": "standing", "walk": "walking", "sleep": "sleeping", "eat": "eating", "drink": "drinking"}.get(activity, activity.replace("_", " "))


def _icon(name: str) -> Gtk.Image:
    image = Gtk.Image.new_from_icon_name(name)
    image.set_pixel_size(20)
    return image


class Notice:
    def __init__(self, key: str, kind: str, title: str, body: str, icon: str, actions: list[dict] | None = None, meta: str = ""):
        self.key = key
        self.kind = kind
        self.title = title
        self.body = body
        self.icon = icon
        self.actions = actions or []
        self.meta = meta
        self.created = time.time()

    def signature(self) -> tuple:
        return (self.key, self.title, self.body, tuple((a.get("label"), a.get("command")) for a in self.actions))


class NoticeModel:
    """Turns daemon state into a small, deduplicated, quiet-aware notice list."""

    def __init__(self):
        self.notices: list[Notice] = []
        self._offers: set[str] = set()
        self._last_need: dict[str, float] = {}
        self._last_user_here: bool | None = None
        self.unread = 0

    def find(self, key: str) -> Notice | None:
        return next((n for n in self.notices if n.key == key), None)

    def remove(self, key: str, cooldown: bool = False, now: float | None = None) -> bool:
        before = len(self.notices)
        self.notices = [n for n in self.notices if n.key != key]
        if len(self.notices) != before:
            if cooldown:
                self._last_need[key] = now if now is not None else time.monotonic()
            return True
        return False

    def add(self, notice: Notice) -> bool:
        if self.find(notice.key):
            return False
        self.notices.insert(0, notice)
        self.notices = self.notices[:9]
        self.unread += 1
        return True

    def update(self, status: dict | None, waiting: list | None, now: float | None = None) -> list[Notice]:
        now = time.monotonic() if now is None else now
        added: list[Notice] = []
        if not status:
            return added
        state = status.get("state") or {}
        needs = status.get("needs") or {}
        user_here = bool(state.get("user_here", False))
        if self._last_user_here is False and user_here and not state.get("quiet"):
            if self.add(Notice("welcome-back", "info", "You're back", "I kept your spot warm.", "emblem-favorite-symbolic", [], "just now")):
                added.append(self.find("welcome-back"))
        self._last_user_here = user_here
        current_offers = {f"offer-{item.get('id')}": item for item in (waiting or []) if item.get("id") is not None}
        for key in list(self._offers):
            if key not in current_offers:
                self._offers.discard(key)
                self.remove(key)
        for key, item in current_offers.items():
            if key in self._offers:
                continue
            self._offers.add(key)
            would = str(item.get("would") or "help with the next step")
            idea = bool(item.get("his_idea"))
            notice = Notice(
                key,
                "offer",
                "I have an idea" if idea else "You asked me to do something",
                f"I could {would}. Shall I?",
                "starred-symbolic",
                [
                    {"label": "Yes, go ahead", "command": "approve", "args": {"id": item.get("id")}, "style": "suggested"},
                    {"label": "Not now", "command": "deny", "args": {"id": item.get("id")}},
                ],
                "his idea" if idea else "waiting for your answer",
            )
            if self.add(notice):
                added.append(notice)
        for name, (title, body, icon, threshold, cooldown, flag, command, label) in NEEDS.items():
            key = f"need-{name}"
            value = float(needs.get(name, 0.0) or 0.0)
            supplied = bool(state.get(flag, False)) if flag else False
            if name == "fatigue" and state.get("activity") == "sleep":
                supplied = True
            if value < threshold - 0.12 or supplied:
                self.remove(key)
                self._last_need.pop(key, None)
                continue
            if value < threshold or self.find(key) or now - self._last_need.get(key, -1e9) < cooldown:
                continue
            notice = Notice(
                key,
                "need",
                title,
                body,
                icon,
                [{"label": label, "command": command, "style": "suggested"}, {"label": "Later", "dismiss": True}],
                f"{name} · {value:.0%}",
            )
            if self.add(notice):
                added.append(notice)
        if state.get("quiet"):
            for notice in added:
                notice.meta = (notice.meta + " · quiet") if notice.meta else "quiet"
        return added


class NoticeCard(Gtk.Box):
    def __init__(self, app: "TrayApp", notice: Notice):
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.add_css_class("tray-card")
        top = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=9)
        top.append(_icon(notice.icon))
        title = Gtk.Label(label=notice.title, xalign=0)
        title.add_css_class("tray-card-title")
        title.set_hexpand(True)
        title.set_wrap(True)
        top.append(title)
        close = Gtk.Button(label="×")
        close.add_css_class("flat")
        close.add_css_class("tray-close")
        close.set_tooltip_text("Dismiss")
        close.connect("clicked", lambda *_: app.dismiss(notice))
        top.append(close)
        self.append(top)
        body = Gtk.Label(label=notice.body, xalign=0)
        body.add_css_class("tray-card-body")
        body.set_wrap(True)
        body.set_max_width_chars(48)
        self.append(body)
        if notice.actions:
            row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
            row.set_margin_top(2)
            for action in notice.actions:
                button = Gtk.Button(label=action.get("label", "OK"))
                if action.get("style") == "suggested":
                    button.add_css_class("suggested-action")
                else:
                    button.add_css_class("flat")
                button.connect("clicked", lambda _button, a=action: app.perform(notice, a))
                row.append(button)
            self.append(row)
        meta = Gtk.Label(label=notice.meta or "just now", xalign=0)
        meta.add_css_class("tray-card-meta")
        self.append(meta)


class PanelWindow(Gtk.Window):
    def __init__(self, app: "TrayApp"):
        super().__init__(application=app, title="Buddy")
        self.app = app
        self.set_decorated(False)
        self.set_resizable(False)
        self.set_default_size(PANEL_WIDTH, PANEL_HEIGHT)
        self.set_size_request(PANEL_WIDTH, PANEL_HEIGHT)
        self.add_css_class("tray-window")
        self._cards_signature: tuple = ()
        self._anchor = (0, 0)
        self._fade_source = None
        self._ever_active = False
        self._open = False

        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        outer.set_vexpand(True)
        outer.set_margin_top(12)
        outer.set_margin_bottom(12)
        outer.set_margin_start(12)
        outer.set_margin_end(12)
        header = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        avatar = Gtk.Image.new_from_file(str(ICON))
        avatar.set_pixel_size(28)
        avatar.set_valign(Gtk.Align.CENTER)
        header.append(avatar)
        titles = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=1)
        heading = Gtk.Label(label="Buddy", xalign=0)
        heading.add_css_class("tray-heading")
        self.subtitle = Gtk.Label(label="Looking for him…", xalign=0)
        self.subtitle.add_css_class("tray-subtitle")
        titles.append(heading)
        titles.append(self.subtitle)
        titles.set_hexpand(True)
        header.append(titles)
        menu = Gtk.MenuButton()
        menu.set_icon_name("open-menu-symbolic")
        menu.add_css_class("flat")
        menu.set_tooltip_text("Tray actions")
        menu.set_popover(self._build_menu())
        header.append(menu)
        close = Gtk.Button(label="×")
        close.add_css_class("flat")
        close.add_css_class("tray-close")
        close.set_tooltip_text("Close")
        close.connect("clicked", lambda *_: self.hide_panel())
        header.append(close)
        outer.append(header)

        self.list = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_min_content_height(90)
        scroll.set_max_content_height(390)
        scroll.set_vexpand(True)
        scroll.set_child(self.list)
        empty_page = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        empty_page.set_valign(Gtk.Align.CENTER)
        empty_page.set_halign(Gtk.Align.CENTER)
        self.empty = Gtk.Label(label="Nothing needs you right now.\nI am happy to just be here.", xalign=0.5)
        self.empty.add_css_class("tray-empty")
        self.empty.set_wrap(True)
        empty_page.append(self.empty)
        self.stack = Gtk.Stack()
        self.stack.set_vexpand(True)
        self.stack.set_transition_type(Gtk.StackTransitionType.CROSSFADE)
        self.stack.add_named(scroll, "list")
        self.stack.add_named(empty_page, "empty")
        self.stack.set_visible_child_name("empty")
        outer.append(self.stack)

        footer = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        open_button = Gtk.Button(label="Open Buddy")
        open_button.add_css_class("suggested-action")
        open_button.connect("clicked", lambda *_: self.open_buddy())
        footer.append(open_button)
        self.clear_button = Gtk.Button(label="Clear")
        self.clear_button.add_css_class("flat")
        self.clear_button.connect("clicked", lambda *_: self.clear())
        footer.append(self.clear_button)
        spacer = Gtk.Box()
        spacer.set_hexpand(True)
        footer.append(spacer)
        outer.append(footer)
        self.set_child(outer)

        key = Gtk.EventControllerKey()
        key.connect("key-pressed", self._on_key)
        self.add_controller(key)
        self.connect("close-request", lambda *_: (self.hide_panel(), True)[1])
        self.connect("notify::is-active", self._on_active)
        self._hover = False
        motion = Gtk.EventControllerMotion()
        motion.connect("enter", lambda *_: setattr(self, "_hover", True))
        motion.connect("leave", lambda *_: setattr(self, "_hover", False))
        self.add_controller(motion)

    def _build_menu(self) -> Gtk.Popover:
        menu = Gtk.Popover()
        menu.add_css_class("tray-menu")
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        for edge in ("top", "bottom", "start", "end"):
            getattr(box, f"set_margin_{edge}")(8)
        for label, callback in [
            ("Open Buddy", lambda *_: self.open_buddy()),
            ("Settings", lambda *_: self.open_buddy("settings")),
            ("Quiet mode", lambda *_: self.toggle_quiet()),
            ("Privacy mode", lambda *_: self.toggle_privacy()),
            ("Quit tray", lambda *_: self.app.quit()),
        ]:
            item = Gtk.Button(label=label)
            item.add_css_class("flat")
            item.set_halign(Gtk.Align.FILL)
            item.connect("clicked", callback)
            box.append(item)
        menu.set_child(box)
        return menu

    def _on_active(self, *_):
        if self.is_active():
            self._ever_active = True
        elif self._ever_active and self.is_open():
            GLib.timeout_add(120, self._hide_if_inactive)

    def _hide_if_inactive(self) -> bool:
        if self.is_open() and not self.is_active():
            self.hide_panel()
        return False

    def _on_key(self, _controller, keyval, _code, _state) -> bool:
        if keyval == Gdk.KEY_Escape:
            self.hide_panel()
            return True
        return False

    def is_open(self) -> bool:
        return self._open

    def show_at(self, x: int, y: int):
        self._anchor = (int(x), int(y))
        self._open = True
        if self.get_visible():
            self.set_visible(True)
            self.present()
        else:
            self.present()
        self._set_input(True)
        self.grab_focus()
        self._place()
        self.set_opacity(0.0)
        steps = [0.15, 0.35, 0.6, 0.82, 1.0]

        def fade(index=0):
            if not self._open:
                return False
            self.set_opacity(steps[index])
            if index + 1 < len(steps):
                self._fade_source = GLib.timeout_add(24, fade, index + 1)
            else:
                self._fade_source = None
            return False

        if self._fade_source:
            GLib.source_remove(self._fade_source)
        fade()

    def hide_panel(self):
        # Keep the window mapped (moved off-screen) rather than unmapping the only
        # Gtk.Application window: unmapping it lets the application exit after a
        # short idle period even though the SNI itself has no window. An empty
        # input region is essential: an invisible window at the screen origin must
        # never intercept clicks meant for the desktop.
        self._open = False
        self.set_opacity(0.0)
        self._set_input(False)
        self._move_offscreen()

    def _set_input(self, on: bool):
        try:
            surface = self.get_surface()
            if surface is not None and cairo is not None:
                surface.set_input_region(None if on else cairo.Region())
        except Exception:
            pass

    def _move_offscreen(self):
        try:
            surface = self.get_surface()
            if isinstance(surface, GdkX11.X11Surface):
                self._x11_move(surface.get_xid(), -10000, -10000)
        except Exception:
            pass

    @staticmethod
    def _x11_move(xid: int, x: int, y: int):
        try:
            from Xlib import display
        except Exception:
            return
        try:
            xd = display.Display()
            xd.create_resource_object("window", xid).configure(x=x, y=y)
            xd.flush()
        except Exception:
            pass

    def _place(self):
        try:
            surface = self.get_surface()
            if not isinstance(surface, GdkX11.X11Surface):
                return
            display = Gdk.Display.get_default()
            monitors = display.get_monitors()
            monitor = monitors.get_item(0) if monitors and monitors.get_n_items() else None
            geometry = monitor.get_geometry() if monitor else Gdk.Rectangle(x=0, y=0, width=1920, height=1080)
            scale = monitor.get_scale_factor() if monitor else 1
            width, height = PANEL_WIDTH * scale, PANEL_HEIGHT * scale
            ax, ay = self._anchor
            x = ax - width + 24 * scale
            y = ay + 8 * scale
            x = max(geometry.x + 8 * scale, min(x, geometry.x + geometry.width - width - 8 * scale))
            if y + height > geometry.y + geometry.height - 8 * scale:
                y = max(geometry.y + 8 * scale, ay - height - 8 * scale)
            self._x11_place(surface.get_xid(), x, y, width, height)
        except Exception:
            pass

    @staticmethod
    def _x11_place(xid: int, x: int, y: int, width: int, height: int):
        try:
            from Xlib import X, Xatom, display
        except Exception:
            return
        try:
            xd = display.Display()
            window = xd.create_resource_object("window", xid)
            window.change_attributes(override_redirect=0)
            state = xd.intern_atom("_NET_WM_STATE")
            above = xd.intern_atom("_NET_WM_STATE_ABOVE")
            skip = xd.intern_atom("_NET_WM_STATE_SKIP_TASKBAR")
            pager = xd.intern_atom("_NET_WM_STATE_SKIP_PAGER")
            window.change_property(state, Xatom.ATOM, 32, [above, skip, pager])
            window.configure(x=x, y=y, width=width, height=height, stack_mode=X.Above)
            xd.flush()
        except Exception:
            pass

    def refresh(self):
        signature = tuple(n.signature() for n in self.app.model.notices)
        if signature != self._cards_signature:
            self._cards_signature = signature
            while (child := self.list.get_first_child()) is not None:
                self.list.remove(child)
            for notice in self.app.model.notices:
                card = NoticeCard(self.app, notice)
                reveal = Gtk.Revealer()
                reveal.set_transition_type(Gtk.RevealerTransitionType.SLIDE_DOWN)
                reveal.set_transition_duration(180)
                reveal.set_child(card)
                reveal.set_reveal_child(False)
                self.list.append(reveal)
                GLib.idle_add(lambda r=reveal: (r.set_reveal_child(True), False)[1])
            self.stack.set_visible_child_name("list" if self.app.model.notices else "empty")
            self.clear_button.set_sensitive(bool(self.app.model.notices))
        status = self.app.last_status
        state = (status or {}).get("state", {})
        if status:
            doing = _doing(str(state.get("activity", "stand")))
            count = len(self.app.model.notices)
            if count:
                self.subtitle.set_label(f"{doing} · {count} message{'s' if count != 1 else ''}")
            else:
                self.subtitle.set_label(f"{doing} · nothing needs you")
        else:
            self.subtitle.set_label("Buddy is not running")

    def clear(self):
        now = time.monotonic()
        for notice in self.app.model.notices:
            if notice.kind == "need":
                self.app.model._last_need[notice.key] = now
        self.app.model.notices.clear()
        self.app.model.unread = 0
        self._cards_signature = ()
        self.refresh()
        if self.app.sni:
            self.app.sni.update()
        if self.app.control:
            self.app.control.update()

    def open_buddy(self, page: str | None = None):
        self.app.open_buddy(page)
        self.hide_panel()

    def toggle_quiet(self):
        self.app.toggle_quiet()

    def toggle_privacy(self):
        self.app.toggle_privacy()


class TrayControl:
    """A tiny session D-Bus door for the GNOME panel button.

    AppIndicator's GNOME host deliberately reserves double-click for Activate and
    gives a single click to a native menu. The project's own Shell extension uses
    this interface to open the custom panel on a single click instead.
    """

    XML = """<node>
      <interface name="local.buddy.TrayControl">
        <method name="TogglePanel"/>
        <method name="OpenPanel"/>
        <method name="ClosePanel"/>
        <property name="Unread" type="u" access="read"/>
      </interface>
    </node>"""

    def __init__(self, app: "TrayApp"):
        self.app = app
        self.connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.interface = Gio.DBusNodeInfo.new_for_xml(self.XML).interfaces[0]
        self.registration = self.connection.register_object("/Control", self.interface, self._method_call, self._get_property, None)
        Gio.bus_own_name_on_connection(self.connection, "local.buddy.TrayControl", Gio.BusNameOwnerFlags.NONE, None, None)

    def _get_property(self, _connection, _sender, _path, _iface, prop):
        if prop == "Unread":
            return GLib.Variant("u", self.app.model.unread)
        return None

    def _method_call(self, _connection, _sender, _path, _iface, method, _parameters, invocation):
        if method == "TogglePanel":
            GLib.idle_add(self.app.toggle_panel, 0, 0)
        elif method == "OpenPanel":
            GLib.idle_add(self.app.open_panel, 0, 0)
        elif method == "ClosePanel":
            GLib.idle_add(self.app.panel.hide_panel if self.app.panel else lambda: None)
        invocation.return_value(GLib.Variant("()", ()))

    def update(self):
        try:
            self.connection.emit_signal(None, "/Control", "local.buddy.TrayControl", "PropertiesChanged", GLib.Variant("(sa{sv}as)", (
                "local.buddy.TrayControl", {"Unread": GLib.Variant("u", self.app.model.unread)}, [])))
        except Exception:
            pass

    def close(self):
        try:
            self.connection.unregister_object(self.registration)
        except Exception:
            pass


class DBusMenu:
    """The small native menu AppIndicator hosts for the tray icon.

    The custom panel is still Buddy's own window; this menu is only the
    conventional tray menu behind a single click, and every item calls the same
    app action as the panel.
    """

    XML = """<node>
      <interface name="com.canonical.dbusmenu">
        <method name="GetLayout">
          <arg name="parentId" type="i" direction="in"/><arg name="recursionDepth" type="i" direction="in"/>
          <arg name="propertyNames" type="as" direction="in"/><arg name="revision" type="u" direction="out"/>
          <arg name="layout" type="(ia{sv}av)" direction="out"/>
        </method>
        <method name="GetGroupProperties">
          <arg name="ids" type="ai" direction="in"/><arg name="propertyNames" type="as" direction="in"/>
          <arg name="properties" type="a(ia{sv})" direction="out"/>
        </method>
        <method name="GetProperty"><arg name="id" type="i" direction="in"/><arg name="name" type="s" direction="in"/><arg name="value" type="v" direction="out"/></method>
        <method name="Event"><arg name="id" type="i" direction="in"/><arg name="eventId" type="s" direction="in"/><arg name="data" type="v" direction="in"/><arg name="timestamp" type="u" direction="in"/></method>
        <method name="EventGroup"><arg name="events" type="a(isvu)" direction="in"/><arg name="errors" type="ai" direction="out"/></method>
        <method name="AboutToShow"><arg name="id" type="i" direction="in"/><arg name="needUpdate" type="b" direction="out"/></method>
        <method name="AboutToShowGroup"><arg name="ids" type="ai" direction="in"/><arg name="updatesNeeded" type="ai" direction="out"/><arg name="idErrors" type="ai" direction="out"/></method>
        <property name="Version" type="u" access="read"/>
        <property name="TextDirection" type="s" access="read"/>
        <property name="Status" type="s" access="read"/>
        <property name="IconThemePath" type="as" access="read"/>
        <signal name="ItemsPropertiesUpdated"><arg type="a(ia{sv})"/><arg type="a(ias)"/></signal>
        <signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
      </interface>
    </node>"""

    def __init__(self, connection, app: "TrayApp"):
        self.connection = connection
        self.app = app
        self.interface = Gio.DBusNodeInfo.new_for_xml(self.XML).interfaces[0]
        self.registration = connection.register_object("/MenuBar", self.interface, self._method_call, self._get_property, None)
        self._items = {
            0: ("", [1, 2, 3, 4]),
            1: ("Open Buddy messages", []),
            2: ("Quiet mode", []),
            3: ("Privacy mode", []),
            4: ("Quit tray", []),
        }

    def _props(self, item: int):
        if item == 0:
            return {"children-display": GLib.Variant("s", "submenu")}
        label = self._items[item][0]
        if not label:
            return {}
        return {"label": GLib.Variant("s", label), "enabled": GLib.Variant("b", True), "visible": GLib.Variant("b", True)}

    def _layout_tuple(self, item: int):
        children = [GLib.Variant("(ia{sv}av)", self._layout_tuple(child)) for child in self._items[item][1]]
        return (item, self._props(item), children)

    def _get_property(self, _connection, _sender, _path, _iface, prop):
        if prop == "Version":
            return GLib.Variant("u", 3)
        if prop == "TextDirection":
            return GLib.Variant("s", "ltr")
        if prop == "Status":
            return GLib.Variant("s", "normal")
        if prop == "IconThemePath":
            return GLib.Variant("as", [])
        return None

    def _method_call(self, _connection, _sender, _path, _iface, method, parameters, invocation):
        try:
            if method == "GetLayout":
                parent_id, _depth, _names = parameters.unpack()
                root = self._layout_tuple(parent_id if parent_id in self._items else 0)
                invocation.return_value(GLib.Variant("(u(ia{sv}av))", (1, root)))
                return
            if method == "GetGroupProperties":
                ids, _names = parameters.unpack()
                values = [(item, self._props(item)) for item in ids if item in self._items]
                invocation.return_value(GLib.Variant("(a(ia{sv}))", (values,)))
                return
            if method == "GetProperty":
                item, name = parameters.unpack()
                values = self._props(item)
                invocation.return_value(GLib.Variant("(v)", (values.get(name, GLib.Variant("s", "")),)))
                return
            if method in ("Event", "EventGroup"):
                events = [parameters.unpack()] if method == "Event" else parameters.unpack()[0]
                for event in events:
                    item, event_id = int(event[0]), str(event[1])
                    if event_id in ("clicked", "activate"):
                        self.activate(item)
                invocation.return_value(GLib.Variant("(ai)", ([],)) if method == "EventGroup" else GLib.Variant("()", ()))
                return
            if method in ("AboutToShow", "AboutToShowGroup"):
                invocation.return_value(GLib.Variant("(b)", (False,)) if method == "AboutToShow" else GLib.Variant("(aiai)", ([], [])))
                return
            invocation.return_value(None)
        except Exception as error:
            print(f"buddy-tray: menu call {method}: {error}", file=sys.stderr)
            invocation.return_value(None)

    def activate(self, item: int):
        if item == 1:
            GLib.idle_add(self.app.open_panel, *self.app._last_xy)
        elif item == 2:
            quiet = bool(((self.app.last_status or {}).get("state") or {}).get("quiet", False))
            self.app.run_command("quiet", {"on": not quiet})
        elif item == 3:
            privacy = bool(((self.app.last_status or {}).get("state") or {}).get("privacy", False))
            self.app.run_command("privacy", {"on": not privacy})
        elif item == 4:
            self.app.quit()

    def close(self):
        try:
            self.connection.unregister_object(self.registration)
        except Exception:
            pass


class StatusNotifier:
    """A small StatusNotifierItem backed by GDBus, with no external tray library."""

    XML = """<node>
      <interface name="org.kde.StatusNotifierItem">
        <method name="Activate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
        <method name="SecondaryActivate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
        <method name="ContextMenu"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
        <method name="Scroll"><arg name="delta" type="i" direction="in"/><arg name="orientation" type="s" direction="in"/></method>
        <property name="Category" type="s" access="read"/>
        <property name="Id" type="s" access="read"/>
        <property name="Title" type="s" access="read"/>
        <property name="Status" type="s" access="read"/>
        <property name="WindowId" type="i" access="read"/>
        <property name="IconName" type="s" access="read"/>
        <property name="IconPixmap" type="a(iiay)" access="read"/>
        <property name="OverlayIconName" type="s" access="read"/>
        <property name="OverlayIconPixmap" type="a(iiay)" access="read"/>
        <property name="AttentionIconName" type="s" access="read"/>
        <property name="AttentionIconPixmap" type="a(iiay)" access="read"/>
        <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
        <property name="ItemIsMenu" type="b" access="read"/>
        <property name="Menu" type="o" access="read"/>
        <signal name="NewTitle"/>
        <signal name="NewIcon"/>
        <signal name="NewAttentionIcon"/>
        <signal name="NewOverlayIcon"/>
        <signal name="NewToolTip"/>
        <signal name="NewStatus"><arg name="status" type="s"/></signal>
      </interface>
    </node>"""

    def __init__(self, app: "TrayApp"):
        self.app = app
        self.connection = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.bus_name = f"org.kde.StatusNotifierItem-{os.getpid()}-1"
        self.interface = Gio.DBusNodeInfo.new_for_xml(self.XML).interfaces[0]
        self.registration = self.connection.register_object(SNI_PATH, self.interface, self._method_call, self._get_property, None)
        Gio.bus_own_name_on_connection(self.connection, self.bus_name, Gio.BusNameOwnerFlags.NONE, None, None)
        self._pixmaps = self._make_pixmaps()
        self._title = "Buddy"
        self._tooltip = (ICON.stem, [], "Buddy", "A Golden Retriever on your desktop")
        self._status = "Active"
        self._unread = 0
        self._registered = False
        self._tries = 0
        self.menu = DBusMenu(self.connection, app)
        self._register()

    @staticmethod
    def _make_pixmaps():
        made = []
        for size in (22, 24, 32, 48):
            try:
                pixbuf = GdkPixbuf.Pixbuf.new_from_file_at_size(str(ICON), size, size)
                pixels = pixbuf.get_pixels()
                stride = pixbuf.get_rowstride()
                channels = pixbuf.get_n_channels()
                data = bytearray()
                for y in range(size):
                    for x in range(size):
                        offset = y * stride + x * channels
                        r, g, b = pixels[offset], pixels[offset + 1], pixels[offset + 2]
                        a = pixels[offset + 3] if channels == 4 else 255
                        data.extend((a, r, g, b))
                made.append((size, size, bytes(data)))
            except Exception:
                continue
        return made

    def _register(self):
        try:
            proxy = Gio.DBusProxy.new_sync(
                self.connection,
                Gio.DBusProxyFlags.NONE,
                None,
                WATCHER_BUS,
                WATCHER_PATH,
                "org.kde.StatusNotifierWatcher",
                None,
            )
            proxy.call_sync("RegisterStatusNotifierItem", GLib.Variant("(s)", (self.bus_name,)), Gio.DBusCallFlags.NONE, 3000, None)
            self._registered = True
        except Exception as error:
            self._tries += 1
            if self._tries <= 15:
                print(f"buddy-tray: no StatusNotifierWatcher yet: {error}", file=sys.stderr)
                GLib.timeout_add_seconds(2, self._retry)
            elif self._tries == 16:
                print("buddy-tray: no tray host; the icon will not appear (GNOME needs the AppIndicator extension)", file=sys.stderr)

    def _retry(self) -> bool:
        if self._registered:
            return False
        self._register()
        return not self._registered

    def _method_call(self, _connection, _sender, _path, _iface, method, parameters, invocation):
        try:
            if method in ("Activate", "SecondaryActivate", "ContextMenu"):
                x, y = parameters.unpack()
                GLib.idle_add(self.app.toggle_panel, int(x), int(y))
            invocation.return_value(GLib.Variant("()", ()))
        except Exception:
            invocation.return_value(GLib.Variant("()", ()))

    def _get_property(self, _connection, _sender, _path, _iface, prop):
        if prop == "Category":
            return GLib.Variant("s", "ApplicationStatus")
        if prop == "Id":
            return GLib.Variant("s", "buddy")
        if prop == "Title":
            return GLib.Variant("s", self._title)
        if prop == "Status":
            return GLib.Variant("s", self._status)
        if prop == "WindowId":
            return GLib.Variant("i", 0)
        if prop == "IconName":
            return GLib.Variant("s", "buddy")
        if prop == "IconPixmap":
            return GLib.Variant("a(iiay)", self._pixmaps)
        if prop == "OverlayIconName":
            return GLib.Variant("s", "emblem-important-symbolic" if self._unread else "")
        if prop == "OverlayIconPixmap":
            return GLib.Variant("a(iiay)", [])
        if prop == "AttentionIconName":
            return GLib.Variant("s", "")
        if prop == "AttentionIconPixmap":
            return GLib.Variant("a(iiay)", [])
        if prop == "ToolTip":
            return GLib.Variant("(sa(iiay)ss)", self._tooltip)
        if prop == "ItemIsMenu":
            return GLib.Variant("b", False)
        if prop == "Menu":
            return GLib.Variant("o", "/MenuBar")
        return None

    def update(self):
        unread = self.app.model.unread
        status = "NeedsAttention" if unread else "Active"
        title = "Buddy"
        subtitle = "No new messages"
        if unread:
            subtitle = f"{unread} message{'s' if unread != 1 else ''} from Buddy"
        elif self.app.last_status:
            state = self.app.last_status.get("state") or {}
            subtitle = f"Buddy is {_doing(str(state.get('activity', 'here')))}"
        changed_status = status != self._status
        self._status = status
        self._unread = unread
        self._title = title
        self._tooltip = (ICON.stem, [], title, subtitle)
        try:
            if changed_status:
                self.connection.emit_signal(None, SNI_PATH, SNI_IFACE, "NewStatus", GLib.Variant("(s)", (status,)))
            self.connection.emit_signal(None, SNI_PATH, SNI_IFACE, "NewToolTip", None)
            self.connection.emit_signal(None, SNI_PATH, SNI_IFACE, "NewOverlayIcon", None)
            self.connection.emit_signal(None, SNI_PATH, SNI_IFACE, "NewTitle", None)
        except Exception:
            pass

    def close(self):
        try:
            self.connection.unregister_object(self.registration)
        except Exception:
            pass
        if self.menu:
            self.menu.close()


class TrayApp(Gtk.Application):
    def __init__(self):
        super().__init__(application_id=APP_ID, flags=0)
        self.model = NoticeModel()
        self.panel: PanelWindow | None = None
        self.control: TrayControl | None = None
        self.sni: StatusNotifier | None = None
        self.is_gnome = (os.environ.get("XDG_CURRENT_DESKTOP") or "").lower().startswith("gnome")
        self.last_status: dict | None = None
        self._last_xy = (0, 0)
        self._stop = threading.Event()
        self._worker: threading.Thread | None = None

    def do_startup(self):
        Gtk.Application.do_startup(self)
        provider = Gtk.CssProvider()
        try:
            provider.load_from_path(str(CSS))
        except GLib.Error as error:
            print(f"buddy-tray: {error}", file=sys.stderr)
        Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

    def do_activate(self):
        # Keep the application alive even before the first notice: the SNI has no
        # visible window of its own, only the host's tray icon.
        self.hold()
        if self.panel is None:
            self.panel = PanelWindow(self)
            self.panel.set_visible(False)
            self.control = TrayControl(self)
            # GNOME's AppIndicator host gives single-click to a native menu and
            # double-click to Activate. On GNOME the project's Shell extension
            # calls TrayControl for a direct single click; KDE/other SNI hosts
            # still get the standard tray item.
            if not self.is_gnome:
                self.sni = StatusNotifier(self)
                self.sni.update()
            self._start_polling()
        else:
            self.toggle_panel(*self._last_xy)

    def do_shutdown(self):
        self._stop.set()
        if self.sni:
            self.sni.close()
        if self.control:
            self.control.close()
        self.release()
        Gtk.Application.do_shutdown(self)

    def _start_polling(self):
        if self._worker and self._worker.is_alive():
            return
        self._worker = threading.Thread(target=self._poll, name="buddy-tray-poll", daemon=True)
        self._worker.start()

    def _poll(self):
        while not self._stop.is_set():
            status = waiting = None
            try:
                status = Api.request("status", None, 3.0)
                waiting = Api.request("waiting", None, 3.0)
            except ApiError:
                pass
            except Exception as error:  # noqa: BLE001
                print(f"buddy-tray: {error}", file=sys.stderr)
            GLib.idle_add(self._apply, status, waiting)
            self._stop.wait(POLL_SECONDS)

    def _apply(self, status, waiting) -> bool:
        self.last_status = status
        added = self.model.update(status, waiting)
        if self.panel:
            self.panel.refresh()
        if self.sni:
            self.sni.update()
        if self.control:
            self.control.update()
        if added and status and not status.get("state", {}).get("quiet", False):
            self.open_panel(*self._last_xy)
            GLib.timeout_add_seconds(12, self._auto_hide_panel)
        return False

    def _auto_hide_panel(self) -> bool:
        if self.panel and self.panel.is_open() and not self.panel._hover:
            self.panel.hide_panel()
        return False

    def toggle_quiet(self):
        quiet = bool(((self.last_status or {}).get("state") or {}).get("quiet", False))
        self.run_command("quiet", {"on": not quiet})

    def toggle_privacy(self):
        privacy = bool(((self.last_status or {}).get("state") or {}).get("privacy", False))
        self.run_command("privacy", {"on": not privacy})

    def open_panel(self, x: int = 0, y: int = 0):
        if not self.panel:
            return
        if x <= 0 and y <= 0:
            x, y = self._last_xy
            if x <= 0 and y <= 0:
                display = Gdk.Display.get_default()
                monitor = display.get_monitors().get_item(0)
                geometry = monitor.get_geometry()
                x, y = geometry.x + geometry.width - 20, geometry.y + 52
        self._last_xy = (x, y)
        self.model.unread = 0
        if self.sni:
            self.sni.update()
        if self.control:
            self.control.update()
        self.panel.show_at(x, y)

    def toggle_panel(self, x: int = 0, y: int = 0):
        if self.panel and self.panel.is_open():
            self.panel.hide_panel()
        else:
            self.open_panel(x, y)

    def dismiss(self, notice: Notice):
        # "Later" and the close button put a need on its cooldown; otherwise it
        # would be re-added on the very next poll.
        self.model.remove(notice.key, cooldown=notice.kind == "need")
        if self.panel:
            self.panel.refresh()
        if self.sni:
            self.sni.update()
        if self.control:
            self.control.update()

    def perform(self, notice: Notice, action: dict):
        if action.get("dismiss"):
            self.dismiss(notice)
            return
        self.dismiss(notice)
        if action.get("command"):
            self.run_command(action["command"], action.get("args"))

    def run_command(self, command: str, args):
        def work():
            try:
                Api.request(command, args, 10.0)
                GLib.idle_add(self._refresh)
            except ApiError as error:
                GLib.idle_add(self.error, str(error))

        threading.Thread(target=work, name=f"buddy-tray-{command}", daemon=True).start()

    def _refresh(self) -> bool:
        try:
            status = Api.request("status", None, 3.0)
            waiting = Api.request("waiting", None, 3.0)
        except ApiError:
            status, waiting = None, None
        self._apply(status, waiting)
        return False

    def open_buddy(self, page: str | None = None):
        binary = os.environ.get("BUDDY_UI_BIN") or shutil.which("buddy-ui") or str(Path.home() / ".local" / "bin" / "buddy-ui")
        if not Path(binary).exists():
            self.error("The control panel is not installed. Run scripts/install.sh.")
            return
        env = os.environ.copy()
        if page:
            env["BUDDY_UI_PAGE"] = page
        try:
            subprocess.Popen([binary], env=env, start_new_session=True)
        except OSError as error:
            self.error(f"Could not open the control panel: {error}")

    def error(self, message: str) -> bool:
        notice = Notice(f"error-{time.time_ns()}", "error", "That did not work", message, "dialog-error-symbolic", [{"label": "Dismiss", "dismiss": True}], "please try again")
        self.model.add(notice)
        if self.panel:
            self.panel.refresh()
            if not self.panel.is_open():
                self.open_panel(*self._last_xy)
        if self.sni:
            self.sni.update()
        return False


def main():
    GLib.set_prgname("buddy-tray")
    GLib.set_application_name("Buddy Tray")
    app = TrayApp()
    return app.run(sys.argv)


if __name__ == "__main__":
    raise SystemExit(main())
