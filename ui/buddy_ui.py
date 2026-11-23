#!/usr/bin/env python3
"""Buddy's control panel: a native GTK4 / libadwaita window for everything he does.

The daemon remains the single decision-maker. This program only speaks the same
newline-delimited JSON protocol as the CLI over the Unix socket, in a background
thread so the window never blocks. It is deliberately a client: closing it does
not close Buddy.
"""

from __future__ import annotations

import datetime
import json
import os
import shutil
import socket
import subprocess
import sys
import threading
from pathlib import Path

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
gi.require_version("Gdk", "4.0")
gi.require_version("Pango", "1.0")
from gi.repository import Adw, Gdk, Gio, GLib, Gtk, Pango  # noqa: E402

APP_ID = "local.buddy.ControlPanel"
APP_VERSION = "1.0.0"
HERE = Path(__file__).resolve().parent
ICON_FILE = HERE / "buddy.svg"

NEEDS = [
    ("fatigue", "Tiredness", "How tired he is"),
    ("hunger", "Hunger", "When he last ate"),
    ("thirst", "Thirst", "When he last drank"),
    ("boredom", "Boredom", "How much he wants something to do"),
    ("social", "Company", "How much he wants you near"),
    ("itch", "Itch", "A small, passing annoyance"),
    ("stiffness", "Stiffness", "How much he wants to move"),
]

TRICKS = [
    "come",
    "sit",
    "down",
    "stand",
    "stay",
    "sleep",
    "wake",
    "speak",
    "paw",
    "spin",
    "roll_over",
    "bow",
    "shake",
    "stretch",
    "play",
    "look",
]

PAGE_TITLES = {
    "home": "Home",
    "talk": "Talk",
    "train": "Train",
    "senses": "Senses",
    "ai": "Local AI",
    "skills": "Skills",
    "workflows": "Workflows",
    "journal": "Journal",
    "settings": "Settings",
}

PAGE_SUBS = {
    "home": "How he is and what he is doing",
    "talk": "Say something, or ask him to do something",
    "train": "Words, tricks and what he remembers",
    "senses": "What he may notice, one source at a time",
    "ai": "His local model, voice and capabilities",
    "skills": "What he may do on the computer",
    "workflows": "Application routines you can record and replay",
    "journal": "The last things that happened",
    "settings": "His body, his window and his data",
}


class ApiError(RuntimeError):
    pass


def socket_path() -> Path:
    if value := os.environ.get("BUDDY_SOCKET"):
        return Path(value)
    runtime = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    return Path(runtime) / "buddy" / "buddy.sock"


def data_dir() -> Path:
    if value := os.environ.get("BUDDY_DATA_DIR"):
        return Path(value)
    base = os.environ.get("XDG_DATA_HOME") or str(Path.home() / ".local" / "share")
    return Path(base) / "buddy"


def cache_dir() -> Path:
    base = os.environ.get("XDG_CACHE_HOME") or str(Path.home() / ".cache")
    return Path(base) / "buddy" / "ui"


def clean(value) -> str:
    return str(value or "").replace("_", " ")


def titlecase(value) -> str:
    text = clean(value)
    return text[:1].upper() + text[1:] if text else ""


def seconds(value) -> str:
    value = float(value or 0.0)
    if value < 60:
        return f"{value:.0f}s"
    if value < 3600:
        return f"{value / 60:.0f} min"
    return f"{value / 3600:.1f} h"


def age_text(days: float) -> str:
    days = float(days or 0.0)
    if days < 1:
        return f"{days * 24:.0f} hours old"
    if days < 14:
        return f"{days:.1f} days old"
    return f"{days:.0f} days old"


def activity_sentence(state: dict) -> str:
    activity = clean(state.get("activity"))
    phrases = {
        "sleep": "curled up asleep",
        "lie": "lying down",
        "sit": "sitting",
        "stand": "standing",
        "walk": "wandering about",
        "greet": "happy to see you",
        "play": "playing",
        "groom": "grooming",
        "eat": "eating",
        "drink": "drinking",
        "stretch": "stretching",
        "shake": "shaking himself",
        "look": "looking around",
        "bark": "barking",
        "dig": "digging",
        "sniff": "sniffing",
        "roll": "rolling over",
        "chase": "chasing something",
    }
    if state.get("hidden"):
        return "hidden for now"
    return phrases.get(activity, f"busy with {activity or 'something'}")


def mood_words(status: dict) -> list[tuple[str, str]]:
    feeling = status.get("feeling") or {}
    state = status.get("state") or {}
    if state.get("privacy"):
        return [("Private", "quiet")]
    if state.get("hidden"):
        return [("Out of the way", "quiet")]
    if state.get("activity") == "sleep" or state.get("sleepiness", 0) > 0.75:
        return [("Sleepy", "warm")]
    words: list[tuple[str, str]] = []
    if feeling.get("happiness", 0) > 0.72:
        words.append(("Happy", "ok"))
    elif feeling.get("contentment", 0) > 0.65:
        words.append(("Content", "ok"))
    if feeling.get("playfulness", 0) > 0.58:
        words.append(("Playful", "warm"))
    if feeling.get("curiosity", 0) > 0.55:
        words.append(("Curious", "warm"))
    if feeling.get("loneliness", 0) > 0.45:
        words.append(("Lonely", "bad"))
    if feeling.get("confusion", 0) > 0.35:
        words.append(("Puzzled", "bad"))
    return words or [("Settled", "quiet")]


def need_class(value: float) -> str:
    if value >= 0.75:
        return "need-high"
    if value <= 0.25:
        return "need-calm"
    return ""


def brief(value, limit: int = 180) -> str:
    if value is None:
        return ""
    if isinstance(value, str):
        text = value
    else:
        text = json.dumps(value, ensure_ascii=False, separators=(", ", ": "))
    text = text.replace("\n", " ")
    return text if len(text) <= limit else text[: limit - 1] + "…"


def when(ts) -> str:
    try:
        return datetime.datetime.fromtimestamp(float(ts)).strftime("%b %d, %H:%M")
    except (TypeError, ValueError, OSError):
        return "?"


class Api:
    """One command and one reply; a fresh socket per call, exactly like the CLI."""

    @staticmethod
    def request(name: str, args=None, timeout: float = 8.0):
        path = socket_path()
        if not path.exists():
            raise ApiError(f"Buddy is not running (no socket at {path}). Start him with: systemctl --user start buddy")
        sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        sock.settimeout(timeout)
        try:
            try:
                sock.connect(str(path))
            except OSError as error:
                raise ApiError(f"Buddy is not reachable at {path}: {error}") from error
            payload = json.dumps({"type": "command", "name": name, "args": args}, ensure_ascii=False) + "\n"
            hello = json.dumps({"type": "hello", "role": "cli"}) + "\n"
            stream = sock.makefile("rwb")
            stream.write(hello.encode("utf-8"))
            stream.write(payload.encode("utf-8"))
            stream.flush()
            for raw in stream:
                try:
                    message = json.loads(raw.decode("utf-8"))
                except (UnicodeDecodeError, json.JSONDecodeError) as error:
                    raise ApiError(f"Buddy sent something this window did not understand: {error}") from error
                if message.get("type") == "reply":
                    if message.get("ok"):
                        return message.get("data")
                    raise ApiError(message.get("error") or "Buddy could not do that")
            raise ApiError("Buddy closed the connection without answering")
        except socket.timeout as error:
            raise ApiError(f"Buddy took longer than {timeout:.0f} seconds to answer") from error
        finally:
            try:
                sock.close()
            except OSError:
                pass


class BuddyWindow(Adw.ApplicationWindow):
    def __init__(self, app: "BuddyApp"):
        super().__init__(application=app, title="Buddy")
        self.app = app
        self.set_default_size(1240, 840)
        self.set_size_request(940, 640)
        self.set_icon_name("buddy")
        self.add_css_class("buddy")
        self.status: dict | None = None
        self.senses: dict | None = None
        self.providers: dict | None = None
        self._busy = 0
        self._poll_running = False
        self._offline = False
        self._updating = False
        self._waiting_signature = None
        self._last_activity = None
        self._snapshot_tries = 0
        self._build_shell()
        self._build_pages()
        if hasattr(self, "nav"):
            self.nav.select_row(self.nav.get_row_at_index(0))
        self._wire_theme()
        self._refresh_all()
        GLib.timeout_add_seconds(2, self._poll)

    # ---------------------------------------------------------------- shell

    def _build_shell(self):
        self.style_manager = Adw.StyleManager.get_default()
        self.toast_overlay = Adw.ToastOverlay()
        self.set_content(self.toast_overlay)

        root = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=0)
        root.set_vexpand(True)
        self.toolbar_view = Adw.ToolbarView()
        self.toolbar_view.set_content(root)
        self.toast_overlay.set_child(self.toolbar_view)

        sidebar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=10)
        sidebar.add_css_class("sidebar")
        sidebar.set_size_request(236, -1)
        sidebar.set_hexpand(False)
        sidebar.set_halign(Gtk.Align.START)

        brand = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        brand.set_margin_bottom(10)
        brand.set_margin_start(4)
        icon = Gtk.Image.new_from_file(str(ICON_FILE))
        icon.set_pixel_size(44)
        icon.add_css_class("about-icon")
        brand_text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        name = Gtk.Label(label="Buddy")
        name.set_xalign(0)
        name.add_css_class("brand-name")
        sub = Gtk.Label(label="Your desktop dog")
        sub.set_xalign(0)
        sub.add_css_class("brand-sub")
        brand_text.append(name)
        brand_text.append(sub)
        brand.append(icon)
        brand.append(brand_text)
        sidebar.append(brand)

        self.nav = Gtk.ListBox()
        self.nav.set_selection_mode(Gtk.SelectionMode.SINGLE)
        self.nav.add_css_class("nav")
        self.nav.connect("row-selected", self._on_nav)
        entries = [
            ("home", "go-home-symbolic", "Home"),
            ("talk", "chat-bubbles-symbolic", "Talk"),
            ("train", "starred-symbolic", "Train"),
            ("senses", "preferences-system-symbolic", "Senses"),
            ("ai", "applications-science-symbolic", "Local AI"),
            ("skills", "applications-utilities-symbolic", "Skills"),
            ("workflows", "media-playlist-repeat-symbolic", "Workflows"),
            ("journal", "document-open-recent-symbolic", "Journal"),
            ("settings", "emblem-system-symbolic", "Settings"),
        ]
        for key, icon_name, label in entries:
            row = Gtk.ListBoxRow()
            row.page_key = key
            row.add_css_class("nav-row")
            content = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
            content.set_margin_start(2)
            content.set_margin_end(2)
            content.append(self._icon(icon_name, 16))
            text = Gtk.Label(label=label)
            text.set_xalign(0)
            text.add_css_class("nav-label")
            content.append(text)
            row.set_child(content)
            self.nav.append(row)
        sidebar.append(self.nav)

        spacer = Gtk.Box()
        spacer.set_vexpand(True)
        sidebar.append(spacer)

        status_card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=3)
        status_card.add_css_class("sidebar-status")
        top = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=7)
        self.side_dot = Gtk.Label(label="●")
        self.side_dot.add_css_class("status-dot")
        self.side_dot.add_css_class("offline")
        self.side_title = Gtk.Label(label="Checking…")
        self.side_title.set_xalign(0)
        self.side_title.add_css_class("status-title")
        top.append(self.side_dot)
        top.append(self.side_title)
        self.side_sub = Gtk.Label(label="Looking for Buddy")
        self.side_sub.set_xalign(0)
        self.side_sub.add_css_class("status-sub")
        self.side_sub.set_wrap(True)
        self.side_sub.set_max_width_chars(28)
        self.side_sub.set_ellipsize(Pango.EllipsizeMode.END)
        status_card.append(top)
        status_card.append(self.side_sub)
        sidebar.append(status_card)

        root.append(sidebar)

        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        content.set_hexpand(True)

        self.offline_bar = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        self.offline_bar.add_css_class("offline-bar")
        self.offline_bar.set_visible(False)
        offline_label = Gtk.Label(label="Buddy isn't running. Start him with: systemctl --user start buddy")
        offline_label.set_xalign(0)
        offline_label.set_hexpand(True)
        offline_label.set_wrap(True)
        offline_label.add_css_class("offline-label")
        start = self._text_button("Start him", self._start_daemon, "small-button", "suggested")
        start.set_valign(Gtk.Align.CENTER)
        retry = self._text_button("Try again", self._refresh_all, "small-button")
        retry.set_valign(Gtk.Align.CENTER)
        self.offline_bar.append(offline_label)
        self.offline_bar.append(start)
        self.offline_bar.append(retry)
        content.append(self.offline_bar)

        self.stack = Gtk.Stack()
        self.stack.set_transition_type(Gtk.StackTransitionType.CROSSFADE)
        self.stack.set_transition_duration(160)
        self.stack.set_vexpand(True)
        self.stack.set_hexpand(True)
        content.append(self.stack)
        root.append(content)

        self._build_header()

    def _build_header(self):
        header = Gtk.HeaderBar()
        header.set_show_title_buttons(True)
        title_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=0)
        title_box.set_valign(Gtk.Align.CENTER)
        self.header_title = Gtk.Label(label="Buddy")
        self.header_title.set_xalign(0)
        self.header_title.add_css_class("header-title")
        self.header_sub = Gtk.Label(label="Home")
        self.header_sub.set_xalign(0)
        self.header_sub.add_css_class("header-sub")
        title_box.append(self.header_title)
        title_box.append(self.header_sub)
        header.set_title_widget(title_box)

        self.online_pill = Gtk.Label(label="Checking…")
        self.online_pill.add_css_class("pill")
        self.online_pill.add_css_class("quiet")
        self.online_pill.set_valign(Gtk.Align.CENTER)
        header.pack_end(self.online_pill)

        self.waiting_button = Gtk.Button()
        self.waiting_button.add_css_class("pill")
        self.waiting_button.add_css_class("warm")
        self.waiting_button.set_valign(Gtk.Align.CENTER)
        self.waiting_button.set_visible(False)
        self.waiting_button.set_tooltip_text("Buddy is waiting for your answer")
        self.waiting_button.connect("clicked", lambda *_: self.go_page(0))
        header.pack_end(self.waiting_button)

        self.spinner = Gtk.Spinner()
        self.spinner.set_size_request(18, 18)
        self.spinner.set_valign(Gtk.Align.CENTER)
        self.spinner.add_css_class("status-spinner")
        header.pack_end(self.spinner)

        refresh = Gtk.Button.new_from_icon_name("view-refresh-symbolic")
        refresh.set_tooltip_text("Refresh everything (Ctrl+R)")
        refresh.connect("clicked", lambda *_: self._refresh_all())
        header.pack_end(refresh)

        menu = Gio.Menu()
        menu.append("Refresh", "app.refresh")
        appearance = Gio.Menu()
        for label, value in [("System", "system"), ("Light", "light"), ("Dark", "dark")]:
            item = Gio.MenuItem.new(label, None)
            item.set_action_and_target_value("app.appearance", GLib.Variant.new_string(value))
            appearance.append_item(item)
        menu.append_submenu("Appearance", appearance)
        menu.append("Open data folder", "app.data-folder")
        menu.append("About Buddy", "app.about")
        menu.append("Quit", "app.quit")
        menu_button = Gtk.MenuButton()
        menu_button.set_icon_name("open-menu-symbolic")
        menu_button.set_menu_model(menu)
        menu_button.set_tooltip_text("Menu")
        header.pack_end(menu_button)
        self.toolbar_view.add_top_bar(header)

    def _wire_theme(self):
        self._sync_dark()
        self.style_manager.connect("notify::dark", lambda *_: self._sync_dark())

    def _sync_dark(self):
        if self.style_manager.get_dark():
            self.add_css_class("dark")
        else:
            self.remove_css_class("dark")

    # ---------------------------------------------------------------- pages

    def _new_page(self, key: str):
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_vexpand(True)
        clamp = Adw.Clamp()
        clamp.set_maximum_size(1040)
        clamp.set_tightening_threshold(700)
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        body.add_css_class("page")
        header = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        title = Gtk.Label(label=PAGE_TITLES[key])
        title.set_xalign(0)
        title.add_css_class("page-title")
        sub = Gtk.Label(label=PAGE_SUBS[key])
        sub.set_xalign(0)
        sub.set_wrap(True)
        sub.add_css_class("page-sub")
        header.append(title)
        header.append(sub)
        body.append(header)
        clamp.set_child(body)
        scroll.set_child(clamp)
        self.stack.add_named(scroll, key)
        return body

    def _build_pages(self):
        self._build_home(self._new_page("home"))
        self._build_talk(self._new_page("talk"))
        self._build_train(self._new_page("train"))
        self._build_senses(self._new_page("senses"))
        self._build_ai(self._new_page("ai"))
        self._build_skills(self._new_page("skills"))
        self._build_workflows(self._new_page("workflows"))
        self._build_journal(self._new_page("journal"))
        self._build_settings(self._new_page("settings"))

    def _on_nav(self, listbox, row):
        if row is None:
            return
        for child in self._rows():
            child.remove_css_class("active")
        row.add_css_class("active")
        key = row.page_key
        self.stack.set_visible_child_name(key)
        self.header_sub.set_text(PAGE_TITLES[key])
        self._refresh_page(key)

    def _rows(self):
        row = self.nav.get_row_at_index(0)
        index = 0
        while row is not None:
            yield row
            index += 1
            row = self.nav.get_row_at_index(index)

    def _refresh_page(self, key: str):
        refreshers = {
            "home": self._refresh_home,
            "talk": self._refresh_waiting,
            "train": self._refresh_train,
            "senses": self._refresh_senses,
            "ai": self._refresh_ai,
            "skills": self._refresh_skills,
            "workflows": self._refresh_workflows,
            "journal": self._refresh_journal,
            "settings": self._apply_status_to_settings,
        }
        try:
            refreshers.get(key, lambda: None)()
        except Exception as error:  # noqa: BLE001 - never let a page crash the window
            self._toast(f"Could not refresh {PAGE_TITLES[key]}: {error}")

    def _refresh_all(self):
        self._refresh_status_and_waiting()
        key = self.stack.get_visible_child_name()
        self._refresh_page(key or "home")

    # ---------------------------------------------------------------- widgets

    def _icon(self, name: str, size: int = 18):
        theme = Gtk.IconTheme.get_for_display(Gdk.Display.get_default())
        if not theme.has_icon(name):
            name = "application-x-executable-symbolic"
        image = Gtk.Image.new_from_icon_name(name)
        image.set_pixel_size(size)
        return image

    def _text_button(self, label: str, callback, *classes: str, icon: str | None = None):
        button = Gtk.Button()
        if icon:
            content = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
            content.append(self._icon(icon, 15))
            text = Gtk.Label(label=label)
            content.append(text)
            button.set_child(content)
        else:
            button.set_label(label)
        for css in classes:
            if css:
                button.add_css_class(css)
        if callback is not None:
            button.connect("clicked", lambda *_: callback())
        button.set_valign(Gtk.Align.CENTER)
        button.set_halign(Gtk.Align.START)
        return button

    def _pill(self, text: str, kind: str = "quiet"):
        label = Gtk.Label(label=text)
        label.add_css_class("pill")
        label.add_css_class(kind)
        label.set_valign(Gtk.Align.CENTER)
        return label

    def _card(self, title: str | None = None, subtitle: str | None = None, action: Gtk.Widget | None = None):
        card = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        card.add_css_class("card")
        if title is not None:
            head = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
            text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            text.set_hexpand(True)
            label = Gtk.Label(label=title)
            label.set_xalign(0)
            label.add_css_class("card-title")
            text.append(label)
            if subtitle:
                hint = Gtk.Label(label=subtitle)
                hint.set_xalign(0)
                hint.set_wrap(True)
                hint.add_css_class("card-sub")
                text.append(hint)
            head.append(text)
            if action is not None:
                head.append(action)
            card.append(head)
        return card

    def _section(self, card: Gtk.Box, title: str, hint: str | None = None):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        label = Gtk.Label(label=title)
        label.set_xalign(0)
        label.add_css_class("field-label")
        box.append(label)
        if hint:
            sub = Gtk.Label(label=hint)
            sub.set_xalign(0)
            sub.set_wrap(True)
            sub.add_css_class("field-hint")
            box.append(sub)
        card.append(box)

    def _field_row(self, label: str, hint: str | None, widget: Gtk.Widget):
        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        row.set_margin_top(2)
        row.set_margin_bottom(2)
        text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=1)
        text.set_hexpand(True)
        title = Gtk.Label(label=label)
        title.set_xalign(0)
        title.add_css_class("field-label")
        text.append(title)
        if hint:
            sub = Gtk.Label(label=hint)
            sub.set_xalign(0)
            sub.set_wrap(True)
            sub.add_css_class("field-hint")
            text.append(sub)
        row.append(text)
        row.append(widget)
        return row

    def _switch(self, active: bool, callback, tooltip: str | None = None):
        switch = Gtk.Switch()
        switch.set_active(bool(active))
        switch.set_valign(Gtk.Align.CENTER)
        if tooltip:
            switch.set_tooltip_text(tooltip)

        def changed(_switch, state):
            if not self._updating:
                callback(bool(state))
            return False

        switch.connect("state-set", changed)
        return switch

    def _set_switch_value(self, switch: Gtk.Switch, value: bool):
        self._updating = True
        try:
            switch.set_active(bool(value))
        finally:
            self._updating = False

    def _entry(self, placeholder: str, on_activate=None, text: str = ""):
        entry = Gtk.Entry()
        entry.set_placeholder_text(placeholder)
        entry.set_text(text)
        entry.set_hexpand(True)
        entry.add_css_class("entry-row-entry")
        if on_activate:
            entry.connect("activate", lambda *_: on_activate())
        return entry

    def _clear(self, box: Gtk.Box):
        child = box.get_first_child()
        while child is not None:
            following = child.get_next_sibling()
            box.remove(child)
            child = following

    def _empty(self, text: str):
        label = Gtk.Label(label=text)
        label.set_xalign(0)
        label.set_wrap(True)
        label.add_css_class("empty")
        return label

    def _scrolled_text(self, text: str, height: int = 380):
        view = Gtk.TextView()
        view.set_editable(False)
        view.set_cursor_visible(False)
        view.set_wrap_mode(Gtk.WrapMode.WORD_CHAR)
        view.add_css_class("mono")
        view.get_buffer().set_text(text)
        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.set_child(view)
        scroll.set_size_request(-1, height)
        return scroll

    def _text_dialog(self, heading: str, body: str, text: str):
        dialog = Adw.MessageDialog(transient_for=self, heading=heading, body=body)
        dialog.add_response("close", "Close")
        dialog.set_default_response("close")
        dialog.set_extra_child(self._scrolled_text(text))
        dialog.connect("response", lambda dlg, _: dlg.close())
        dialog.present()

    # ---------------------------------------------------------------- async

    def _toast(self, text: str):
        toast = Adw.Toast.new(text)
        toast.set_timeout(3)
        self.toast_overlay.add_toast(toast)

    def _busy_push(self):
        self._busy += 1
        if self._busy == 1:
            self.spinner.start()

    def _busy_pop(self):
        self._busy = max(0, self._busy - 1)
        if self._busy == 0:
            self.spinner.stop()

    def _run(self, work, done, busy: bool = True):
        if busy:
            self._busy_push()

        def worker():
            try:
                result = work()
                error = None
            except Exception as exc:  # noqa: BLE001 - surfaced in the window
                result = None
                error = exc

            def finish():
                if busy:
                    self._busy_pop()
                try:
                    done(result, error)
                except Exception as exc:  # noqa: BLE001
                    self._toast(f"Something went wrong: {exc}")
                return False

            GLib.idle_add(finish)

        threading.Thread(target=worker, daemon=True).start()

    def _command(self, name: str, args=None, done=None, timeout: float = 10.0, busy: bool = True, success: str | None = None):
        def finished(data, error):
            if error is not None:
                self._toast(str(error))
                if done:
                    done(None, error)
                return
            if success:
                self._toast(success)
            if done:
                done(data, None)

        self._run(lambda: Api.request(name, args, timeout), finished, busy)

    def _refresh_status_and_waiting(self):
        if self._poll_running:
            return
        self._poll_running = True

        def work():
            status = Api.request("status", None, 5)
            try:
                waiting = Api.request("waiting", None, 5)
            except ApiError:
                waiting = []
            return status, waiting

        def done(result, error):
            self._poll_running = False
            if error is not None:
                self._set_offline(str(error))
                return
            status, waiting = result
            self._set_online(status, waiting)

        self._run(work, done, busy=False)

    def _poll(self):
        self._refresh_status_and_waiting()
        return True

    def _refresh_home(self):
        self._refresh_status_and_waiting()
        self._refresh_why()

    # ---------------------------------------------------------------- online

    def _start_daemon(self):
        def work():
            result = subprocess.run(
                ["systemctl", "--user", "start", "buddy.service"],
                capture_output=True,
                text=True,
                timeout=20,
                check=False,
            )
            if result.returncode != 0:
                message = (result.stderr or result.stdout or "systemctl could not start Buddy").strip()
                raise ApiError(message)
            return True

        def done(data, error):
            if error is not None:
                self._toast(str(error))
                return
            self._toast("Starting Buddy…")
            GLib.timeout_add(1200, self._refresh_all)

        self._run(work, done)

    def _set_offline(self, why: str):
        self._offline = True
        self.offline_bar.set_visible(True)
        self.online_pill.set_text("Offline")
        for css in ("ok", "warm", "bad", "quiet"):
            self.online_pill.remove_css_class(css)
        self.online_pill.add_css_class("bad")
        self.side_dot.remove_css_class("online")
        self.side_dot.remove_css_class("asleep")
        self.side_dot.add_css_class("offline")
        self.side_title.set_text("Not running")
        self.side_sub.set_text("Start him with: systemctl --user start buddy")
        self.header_title.set_text("Buddy")
        self.header_sub.set_text("Waiting for him to wake up")

    def _set_online(self, status: dict, waiting: list):
        self._offline = False
        self.offline_bar.set_visible(False)
        self.status = status
        state = status.get("state") or {}
        settings = status.get("settings") or {}
        feeling = status.get("feeling") or {}

        asleep = bool(settings.get("asleep_by_user")) or state.get("activity") == "sleep"
        hidden = bool(state.get("hidden"))
        privacy = bool(state.get("privacy")) or bool(settings.get("privacy_mode"))
        if privacy:
            pill_text, pill_kind = "Private", "warm"
        elif hidden:
            pill_text, pill_kind = "Hidden", "quiet"
        elif asleep:
            pill_text, pill_kind = "Asleep", "warm"
        else:
            pill_text, pill_kind = "Here", "ok"
        self.online_pill.set_text(pill_text)
        for css in ("ok", "warm", "bad", "quiet"):
            self.online_pill.remove_css_class(css)
        self.online_pill.add_css_class(pill_kind)

        self.side_dot.remove_css_class("offline")
        self.side_dot.remove_css_class("online")
        self.side_dot.remove_css_class("asleep")
        self.side_dot.add_css_class("asleep" if (asleep and not privacy) else "online")
        self.side_title.set_text(state.get("name") or "Buddy")
        bits = []
        if state.get("stage"):
            bits.append(titlecase(state["stage"]))
        if state.get("age_days") is not None:
            bits.append(age_text(state["age_days"]))
        if hidden:
            bits.append("hidden")
        if privacy:
            bits.append("privacy on")
        self.side_sub.set_text(" · ".join(bits) if bits else "He is here")
        self.header_title.set_text(state.get("name") or "Buddy")
        self.header_sub.set_text(PAGE_TITLES.get(self.stack.get_visible_child_name() or "home", "Home"))

        self._update_home(status, waiting)
        self._apply_status_to_settings()
        self._update_voice_from_status(status)
        if state.get("activity") != self._last_activity:
            self._last_activity = state.get("activity")
            self._refresh_why()

    def _update_home(self, status: dict, waiting: list):
        state = status.get("state") or {}
        settings = status.get("settings") or {}
        life = status.get("life") or {}

        if hasattr(self, "home_name"):
            self.home_name.set_text(state.get("name") or "Buddy")
            stage = titlecase(state.get("stage") or "puppy")
            growth = float(state.get("growth") or 0.0) * 100
            self.home_meta.set_text(f"{stage} · {age_text(state.get('age_days', 0))} · {growth:.0f}% grown")
            activity = activity_sentence(state)
            suffix = []
            if settings.get("quiet"):
                suffix.append("quiet")
            if state.get("privacy"):
                suffix.append("privacy on")
            self.home_activity.set_text(activity + (f" · {', '.join(suffix)}" if suffix else ""))
            calls = int(life.get("name_calls") or 0)
            self.home_life.set_text(
                f"{seconds(life.get('awake_secs', 0))} awake · {life.get('sessions', 0)} sessions · petted {life.get('pets', 0)} times · "
                f"called {calls} time{'s' if calls != 1 else ''}"
            )
            for child in list(self.home_mood_box):
                self.home_mood_box.remove(child)
            for text, kind in mood_words(status)[:3]:
                self.home_mood_box.append(self._pill(text, kind))
            self._update_quick_buttons(state, settings)
            self._update_needs(status.get("needs") or {})
            self._render_waiting(waiting)
            if hasattr(self, "home_renderer"):
                self.home_renderer.set_text(status.get("renderer") or "unknown")
                self.home_pictures.set_text(brief(status.get("pictures"), 140))

    def _update_needs(self, needs: dict):
        if not hasattr(self, "need_widgets"):
            return
        for key, (value_label, bar) in self.need_widgets.items():
            value = float(needs.get(key) or 0.0)
            value_label.set_text(f"{value * 100:.0f}%")
            bar.set_fraction(max(0.0, min(1.0, value)))
            for css in ("need-high", "need-calm"):
                value_label.remove_css_class(css)
                bar.remove_css_class(css)
            css = need_class(value)
            if css:
                value_label.add_css_class(css)
                bar.add_css_class(css)

    def _update_quick_buttons(self, state: dict, settings: dict):
        if not hasattr(self, "quick_buttons"):
            return
        asleep = bool(settings.get("asleep_by_user"))
        hidden = bool(state.get("hidden"))
        quiet = bool(settings.get("quiet"))
        privacy = bool(state.get("privacy")) or bool(settings.get("privacy_mode"))
        labels = {
            "call": f"Call “{state.get('name') or 'Buddy'}”",
            "sleep": "Wake him" if asleep else "Let him sleep",
            "hide": "Show him" if hidden else "Hide him",
            "quiet": f"Quiet: {'on' if quiet else 'off'}",
            "privacy": f"Privacy: {'on' if privacy else 'off'}",
        }
        for key, label in labels.items():
            if key in self.quick_buttons:
                self.quick_buttons[key].set_label(label)

    def _render_waiting(self, waiting: list):
        if not hasattr(self, "waiting_box"):
            return
        # Do not rebuild the buttons merely because the age of the request moved on.
        signature = json.dumps(
            [(item.get("id"), item.get("would"), item.get("his_idea")) for item in waiting],
            sort_keys=True,
            ensure_ascii=False,
        )
        if signature == self._waiting_signature:
            return
        self._waiting_signature = signature
        self._clear(self.waiting_box)
        if hasattr(self, "waiting_button"):
            self.waiting_button.set_visible(bool(waiting))
            self.waiting_button.set_label(f"{len(waiting)} waiting")
        if not waiting:
            self.waiting_box.append(self._empty("Nothing is waiting for an answer. When he wants to do something on the computer, it will appear here."))
            self.waiting_count.set_text("0")
            return
        self.waiting_count.set_text(str(len(waiting)))
        for item in waiting:
            row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
            row.add_css_class("waiting-row")
            text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
            text.set_hexpand(True)
            what = Gtk.Label(label=str(item.get("would") or "something"))
            what.set_xalign(0)
            what.set_wrap(True)
            what.add_css_class("field-label")
            meta = Gtk.Label(
                label=f"{'His idea' if item.get('his_idea') else 'You asked'} · waiting {seconds(item.get('for_secs', 0))}"
            )
            meta.set_xalign(0)
            meta.add_css_class("field-hint")
            text.append(what)
            text.append(meta)
            row.append(text)
            identifier = item.get("id")
            row.append(self._text_button("Approve", lambda i=identifier: self._answer(i, True, False), "small-button", "suggested"))
            row.append(self._text_button("Always", lambda i=identifier: self._answer(i, True, True), "small-button"))
            row.append(self._text_button("Deny", lambda i=identifier: self._answer(i, False, False), "small-button"))
            row.append(self._text_button("Never", lambda i=identifier: self._answer(i, False, True), "small-button", "destructive"))
            self.waiting_box.append(row)

    def _answer(self, identifier, yes: bool, lasting: bool):
        name = "approve" if yes else "deny"
        args = {"id": identifier, "lasting": lasting}

        def done(data, error):
            if error is not None:
                self._waiting_signature = None
                self._refresh_status_and_waiting()
                return
            if data and data.get("waiting_for_manual"):
                self._show_manual_prompt(data["waiting_for_manual"])
            elif data and data.get("declined"):
                self._toast(f"All right: he will not {data['declined']}")
            elif data and data.get("did"):
                self._toast(f"Done: {data['did']}")
            else:
                self._toast("He is working on it…")
            self._waiting_signature = None
            self._refresh_status_and_waiting()

        # A yes can start a long skill or a workflow replay; wait as long as the CLI does.
        self._command(name, args, done=done, timeout=16 * 60, success=None)

    def _refresh_why(self):
        def work():
            return Api.request("why", None, 6)

        def done(data, error):
            if error is not None or not hasattr(self, "why_label"):
                return
            reason = clean(data.get("because"))
            doing = clean(data.get("activity"))
            secs = seconds(data.get("for_secs", 0))
            others = []
            for item in (data.get("also_considered") or [])[:3]:
                others.append(f"{clean(item.get('activity'))} ({clean(item.get('reason'))})")
            text = f"{titlecase(doing) or 'He'} for {secs} — {reason or 'because that is what fits now'}."
            if others:
                text += " He also considered " + ", ".join(others) + "."
            self.why_label.set_text(text)
            you = data.get("you") or {}
            if hasattr(self, "why_you"):
                self.why_you.set_text(
                    f"You are {'away' if you.get('away') else 'here'} · "
                    f"{'busy at the keyboard' if you.get('busy', 0) > 0.5 else 'not very busy'} · "
                    f"favourite spot {float(data.get('favourite_spot') or 0.0) * 100:.0f}% across the screen"
                )

        self._run(work, done, busy=False)

    # ---------------------------------------------------------------- home

    def _build_home(self, body: Gtk.Box):
        hero = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=20)
        hero.add_css_class("hero")
        icon = Gtk.Image.new_from_file(str(ICON_FILE))
        icon.set_pixel_size(104)
        icon.add_css_class("hero-icon")
        hero.append(icon)
        info = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        info.set_hexpand(True)
        self.home_name = Gtk.Label(label="Buddy")
        self.home_name.set_xalign(0)
        self.home_name.add_css_class("hero-name")
        self.home_meta = Gtk.Label(label="Looking for him…")
        self.home_meta.set_xalign(0)
        self.home_meta.add_css_class("hero-meta")
        self.home_activity = Gtk.Label(label="")
        self.home_activity.set_xalign(0)
        self.home_activity.set_wrap(True)
        self.home_activity.add_css_class("hero-activity")
        self.home_life = Gtk.Label(label="")
        self.home_life.set_xalign(0)
        self.home_life.set_wrap(True)
        self.home_life.add_css_class("hero-meta")
        self.home_mood_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        self.home_mood_box.set_margin_top(2)
        info.append(self.home_name)
        info.append(self.home_meta)
        info.append(self.home_activity)
        info.append(self.home_mood_box)
        info.append(self.home_life)
        hero.append(info)
        body.append(hero)

        quick = self._card("Quick actions", "The same gestures as the CLI, one click away.")
        flow = Gtk.FlowBox()
        flow.set_selection_mode(Gtk.SelectionMode.NONE)
        flow.set_max_children_per_line(9)
        flow.set_min_children_per_line(2)
        flow.set_row_spacing(8)
        flow.set_column_spacing(8)
        quick_actions = [
            ("call", "Call him", "call"),
            ("pet", "Pet him", "pet"),
            ("praise", "Good boy", "praise"),
            ("treat", "Give a treat", "treat"),
            ("feed", "Feed him", "feed"),
            ("water", "Fresh water", "water"),
        ]
        self.quick_buttons = {}
        for key, label, command in quick_actions:
            button = self._text_button(label, lambda c=command: self._quick(c), "quick-button")
            self.quick_buttons[key] = button
            flow.append(button)
        for key, label, command in [
            ("sleep", "Let him sleep", "sleep"),
            ("hide", "Hide him", "hide"),
            ("quiet", "Quiet: off", "quiet"),
            ("privacy", "Privacy: off", "privacy"),
        ]:
            button = self._text_button(label, lambda c=command: self._quick(c), "quick-button")
            self.quick_buttons[key] = button
            flow.append(button)
        quick.append(flow)
        body.append(quick)

        needs = self._card("How he feels", "0% is content; 100% is urgent. These drift with time and what you do together.")
        grid = Gtk.Grid()
        grid.set_column_spacing(24)
        grid.set_row_spacing(12)
        grid.set_column_homogeneous(True)
        self.need_widgets = {}
        for index, (key, label, hint) in enumerate(NEEDS):
            cell = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
            top = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
            text = Gtk.Label(label=label)
            text.set_xalign(0)
            text.set_hexpand(True)
            text.add_css_class("field-label")
            value = Gtk.Label(label="0%")
            value.set_xalign(1)
            value.add_css_class("need-value")
            top.append(text)
            top.append(value)
            bar = Gtk.ProgressBar()
            bar.set_show_text(False)
            bar.add_css_class("need-bar")
            cell.append(top)
            cell.append(bar)
            tooltip = f"{hint}. Lower is better."
            cell.set_tooltip_text(tooltip)
            self.need_widgets[key] = (value, bar)
            grid.attach(cell, index % 2, index // 2, 1, 1)
        needs.append(grid)
        body.append(needs)

        waiting = self._card(
            "Waiting for your answer",
            "He asks before changing anything on the computer. You can answer here, or just say yes to him.",
        )
        self.waiting_count = Gtk.Label(label="0")
        self.waiting_count.add_css_class("pill")
        self.waiting_count.add_css_class("quiet")
        self.waiting_count.set_halign(Gtk.Align.START)
        waiting.append(self.waiting_count)
        self.waiting_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        waiting.append(self.waiting_box)
        body.append(waiting)

        why = self._card("Why he is doing that", "His own account of the moment, not a guess from this window.")
        self.why_label = Gtk.Label(label="Asking him…")
        self.why_label.set_xalign(0)
        self.why_label.set_wrap(True)
        self.why_label.add_css_class("field-hint")
        self.why_you = Gtk.Label(label="")
        self.why_you.set_xalign(0)
        self.why_you.set_wrap(True)
        self.why_you.add_css_class("tiny")
        why.append(self.why_label)
        why.append(self.why_you)
        why.append(self._text_button("Ask again", self._refresh_why, "link-button"))
        body.append(why)

        window_card = self._card("His window", "A snapshot of only his own window, saved by the renderer. Nothing else on your screen is included.")
        snapshot_buttons = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        snapshot_buttons.append(self._text_button("Take a snapshot", self._take_snapshot, "small-button", icon="camera-photo-symbolic"))
        self.snapshot_note = Gtk.Label(label="")
        self.snapshot_note.set_xalign(0)
        self.snapshot_note.set_wrap(True)
        self.snapshot_note.add_css_class("field-hint")
        window_card.append(snapshot_buttons)
        self.snapshot_picture = Gtk.Picture()
        self.snapshot_picture.set_can_shrink(True)
        self.snapshot_picture.set_size_request(-1, 260)
        self.snapshot_picture.add_css_class("snapshot-picture")
        self.snapshot_picture.set_visible(False)
        window_card.append(self.snapshot_picture)
        window_card.append(self.snapshot_note)
        self.home_renderer = Gtk.Label(label="—")
        self.home_renderer.add_css_class("tiny")
        self.home_pictures = Gtk.Label(label="—")
        self.home_pictures.set_wrap(True)
        self.home_pictures.add_css_class("tiny")
        window_card.append(self._field_row("Renderer", None, self.home_renderer))
        window_card.append(self._field_row("Pictures", None, self.home_pictures))
        body.append(window_card)

    def _quick(self, command: str):
        if command == "call":
            self._call_his_name()
            return
        if command in ("sleep", "hide", "quiet", "privacy"):
            state = (self.status or {}).get("state") or {}
            settings = (self.status or {}).get("settings") or {}
            if command == "sleep":
                args = None if settings.get("asleep_by_user") else None
                name = "wake" if settings.get("asleep_by_user") else "sleep"
            elif command == "hide":
                name = "show" if state.get("hidden") else "hide"
                args = {"minutes": 15} if name == "hide" else None
            elif command == "quiet":
                name = "quiet"
                args = {"on": not bool(settings.get("quiet"))}
            else:
                name = "privacy"
                args = {"on": not (bool(state.get("privacy")) or bool(settings.get("privacy_mode")))}
        else:
            name = command
            args = None

        def done(data, error):
            if error is None:
                self._waiting_signature = None
                self._refresh_status_and_waiting()

        self._command(name, args, done=done)

    def _take_snapshot(self):
        target = cache_dir() / "snapshot.png"
        target.parent.mkdir(parents=True, exist_ok=True)
        try:
            if target.exists():
                target.unlink()
        except OSError:
            pass
        self.snapshot_note.set_text("Asking the renderer for only his window…")
        self._snapshot_tries = 0

        def after_sent(data, error):
            if error is not None:
                self.snapshot_note.set_text(str(error))
                return
            self._load_snapshot(target, 0)

        self._command("snapshot", {"path": str(target)}, done=after_sent, timeout=10)

    def _load_snapshot(self, path: Path, tries: int):
        if path.exists() and path.stat().st_size > 0:
            try:
                texture = Gdk.Texture.new_from_filename(str(path))
                self.snapshot_picture.set_paintable(texture)
                self.snapshot_picture.set_visible(True)
                self.snapshot_note.set_text(f"Saved to {path}")
            except Exception as error:  # noqa: BLE001
                self.snapshot_note.set_text(f"The picture was saved but could not be shown: {error}")
            return False
        if tries < 20:
            GLib.timeout_add(150, lambda: self._load_snapshot(path, tries + 1))
        else:
            self.snapshot_note.set_text("The renderer did not write a picture. Is his window open? (buddy status)")
        return False

    # ---------------------------------------------------------------- talk

    def _build_talk(self, body: Gtk.Box):
        card = self._card(
            "Say something to him",
            "English or Persian. Try a cue, praise, a memory, or something for him to do on the computer.",
        )
        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.talk_entry = self._entry("sit · good boy · remember that I like tea · find my tax report", self._send_talk)
        self.talk_entry.add_css_class("entry-row-entry")
        self.talk_button = self._text_button("Say it", self._send_talk, "suggested")
        row.append(self.talk_entry)
        row.append(self.talk_button)
        card.append(row)
        chips = Gtk.FlowBox()
        chips.set_selection_mode(Gtk.SelectionMode.NONE)
        chips.set_max_children_per_line(9)
        chips.set_row_spacing(7)
        chips.set_column_spacing(7)
        for phrase in ["sit", "come", "down", "good boy", "treat", "quiet", "stop"]:
            chips.append(self._text_button(phrase, lambda p=phrase: self._send_talk_text(p), "small-button"))
        card.append(chips)
        self.talk_note = Gtk.Label(label="He does not use a human voice. Watch his body; the answer is also written here.")
        self.talk_note.set_xalign(0)
        self.talk_note.set_wrap(True)
        self.talk_note.add_css_class("field-hint")
        card.append(self.talk_note)
        body.append(card)

        result = self._card("What happened", "His vocabulary is tried first; the local model only helps when it has to.")
        self.talk_result = Gtk.Label(label="Nothing has been said yet.")
        self.talk_result.set_xalign(0)
        self.talk_result.set_wrap(True)
        self.talk_result.set_selectable(True)
        self.talk_result.add_css_class("field-hint")
        result.append(self.talk_result)
        self.talk_approval_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        result.append(self.talk_approval_box)
        body.append(result)

        waiting = self._card("Waiting for your answer", "A request or one of his own ideas can wait here.")
        self.talk_waiting_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        waiting.append(self.talk_waiting_box)
        body.append(waiting)

    def _send_talk(self):
        text = self.talk_entry.get_text().strip()
        if text:
            self._send_talk_text(text)

    def _send_talk_text(self, text: str):
        self.talk_entry.set_text(text)
        self.talk_button.set_sensitive(False)
        self.talk_result.set_text("He is listening…")
        self._clear(self.talk_approval_box)

        def done(data, error):
            self.talk_button.set_sensitive(True)
            if error is not None:
                self.talk_result.set_text(f"He could not answer: {error}")
                return
            if isinstance(data, dict) and data.get("waiting_for_yes"):
                would = data.get("would") or "do something"
                self.talk_result.set_text(f"He would like to {would}. He is waiting for your answer.")
                self._show_approval(data, self.talk_approval_box)
                self._refresh_status_and_waiting()
                return
            if isinstance(data, dict) and data.get("did"):
                detail = data.get("detail") or ""
                self.talk_result.set_text(f"Done: {data['did']}" + (f"\n{detail}" if detail else ""))
                if data.get("can_be_undone"):
                    self.talk_result.set_text(self.talk_result.get_text() + "\n\nYou can take it back from Skills.")
                self._refresh_status_and_waiting()
                return
            if isinstance(data, dict) and "understood_as" in data:
                self.talk_result.set_text(self._format_say(data))
                if data.get("puzzled"):
                    self._toast("He tilted his head: he is not sure what you meant.")
                return
            if data is None:
                self.talk_result.set_text("He heard you.")
                return
            self.talk_result.set_text(json.dumps(data, indent=2, ensure_ascii=False))

        self._command("say", {"text": text}, done=done, timeout=160)

    def _format_say(self, data: dict) -> str:
        meaning = data.get("understood_as")
        if not isinstance(meaning, dict):
            return json.dumps(data, indent=2, ensure_ascii=False)
        kind = clean(meaning.get("kind"))
        detail = ""
        raw = meaning.get("detail")
        if isinstance(raw, str) and raw:
            detail = f" “{raw}”"
        elif isinstance(raw, dict):
            detail = f" “{raw.get('cue', '?')}” means {clean(raw.get('trick'))}"
        lines = [f"Understood as: {kind}{detail} (by {clean(data.get('by'))})."]
        if kind == "called":
            lines = [f"He heard his name — {clean(data.get('now')) or 'he looked up'} because: {clean(data.get('because')) or 'you called him'}."]
            return "\n".join(lines)
        if data.get("puzzled"):
            if kind == "cue":
                lines.append("He knows that word only a little and did not do it this time. Praise him when he does, and he will learn it.")
            else:
                lines.append("He looks at you and tilts his head: he does not know what you want.")
        elif kind == "cue" and data.get("did_as_asked"):
            lines.append(f"He does it: {clean(data.get('now'))} — because: {clean(data.get('because'))}.")
        else:
            now = clean(data.get("now"))
            doing = {
                "stand": "standing",
                "sit": "sitting",
                "lie": "lying down",
                "sleep": "asleep",
                "walk": "wandering",
                "greet": "happy to see you",
                "play": "playing",
                "groom": "grooming",
                "eat": "eating",
                "drink": "drinking",
            }.get(now, now or "still")
            lines.append(f"He is {doing} — because: {clean(data.get('because')) or 'that is what fits now'}.")
        if data.get("reply"):
            lines.append(str(data["reply"]))
        return "\n".join(lines)

    def _show_approval(self, data: dict, container: Gtk.Box):
        identifier = data.get("waiting_for_yes")
        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        row.set_margin_top(4)
        row.append(self._text_button("Approve", lambda: self._answer(identifier, True, False), "small-button", "suggested"))
        row.append(self._text_button("Always", lambda: self._answer(identifier, True, True), "small-button"))
        row.append(self._text_button("Deny", lambda: self._answer(identifier, False, False), "small-button"))
        row.append(self._text_button("Never", lambda: self._answer(identifier, False, True), "small-button", "destructive"))
        container.append(row)

    def _refresh_waiting(self):
        def work():
            return Api.request("waiting", None, 5)

        def done(data, error):
            if error is not None:
                return
            self._render_waiting(data or [])
            if hasattr(self, "talk_waiting_box"):
                self._clear(self.talk_waiting_box)
                if not data:
                    self.talk_waiting_box.append(self._empty("Nothing is waiting for an answer."))
                for item in data:
                    row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
                    text = Gtk.Label(label=str(item.get("would") or "something"))
                    text.set_xalign(0)
                    text.set_hexpand(True)
                    text.set_wrap(True)
                    text.add_css_class("field-label")
                    row.append(text)
                    identifier = item.get("id")
                    row.append(self._text_button("Approve", lambda i=identifier: self._answer(i, True, False), "small-button", "suggested"))
                    row.append(self._text_button("Deny", lambda i=identifier: self._answer(i, False, False), "small-button"))
                    self.talk_waiting_box.append(row)

        self._run(work, done, busy=False)

    # ---------------------------------------------------------------- train

    def _build_train(self, body: Gtk.Box):
        name_card = self._card(
            "His name",
            "The name he answers to. He hears it in typed speech and, with the microphone on, in the room. The first call gets a bright greeting; afterwards he comes and sits.",
        )
        self.train_name_entry = self._entry("e.g. پشمک or Peshmark")
        name_card.append(self._field_row("Name", "Use any English or Persian name; punctuation and Persian spelling variants do not matter.", self.train_name_entry))
        name_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        name_row.append(self._text_button("Rename", self._rename_from_train, "small-button", "suggested"))
        name_row.append(self._text_button("Call him", self._call_his_name, "small-button"))
        name_card.append(name_row)
        body.append(name_card)

        teach = self._card(
            "Teach a word",
            "Telling him outright is the quickest way. Praise him when he does it, and the word becomes surer.",
        )
        self.teach_cue = self._entry("a word or phrase, e.g. “biscuit”")
        self.teach_trick = self._entry("a trick, e.g. sit")
        teach.append(self._field_row("When you say", None, self.teach_cue))
        teach.append(self._field_row("He should", None, self.teach_trick))
        suggestions = Gtk.FlowBox()
        suggestions.set_selection_mode(Gtk.SelectionMode.NONE)
        suggestions.set_max_children_per_line(8)
        suggestions.set_row_spacing(6)
        suggestions.set_column_spacing(6)
        for trick in TRICKS:
            suggestions.append(
                self._text_button(trick.replace("_", " "), lambda t=trick: self.teach_trick.set_text(t), "small-button")
            )
        teach.append(suggestions)
        teach.append(self._text_button("Teach him", self._teach, "suggested"))
        body.append(teach)

        known = self._card("Words he knows", "Instinct is what he was born with; taught words are yours. Strength grows with praise.")
        self.known_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        known.append(self.known_box)
        body.append(known)

        memories = self._card("What he remembers", "Facts and preferences he was told. They are stored in his own database.")
        memory_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.memory_entry = self._entry("something to remember, e.g. “I prefer working late”", self._remember)
        memory_row.append(self.memory_entry)
        memory_row.append(self._text_button("Remember", self._remember, "suggested"))
        memories.append(memory_row)
        self.memory_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        memories.append(self.memory_box)
        body.append(memories)

        practice = self._card(
            "Practice together",
            "Praise, correction and treats are part of training too. He learns from all three.",
        )
        practice_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        for label, phrase in [
            ("Praise", "good boy"),
            ("Correct", "no"),
            ("Stop", "stop"),
            ("Treat", "treat"),
            ("Quiet", "quiet"),
        ]:
            practice_row.append(self._text_button(label, lambda p=phrase: self._send_talk_text(p), "small-button"))
        practice.append(practice_row)
        practice.append(
            Gtk.Label(
                label="He is a dog: praise and tone matter more than a perfect sentence. Persian works too.",
            )
        )
        practice.get_last_child().add_css_class("field-hint")
        practice.get_last_child().set_wrap(True)
        practice.get_last_child().set_xalign(0)
        body.append(practice)

    def _teach(self):
        cue = self.teach_cue.get_text().strip()
        trick = self.teach_trick.get_text().strip()
        if not cue or not trick:
            self._toast("Give him a word and a trick first.")
            return

        def done(data, error):
            if error is None:
                self._toast(f"Taught: “{cue}” means {trick.replace('_', ' ')}.")
                self.teach_cue.set_text("")
                self._refresh_train()

        self._command("teach", {"cue": cue, "trick": trick}, done=done)

    def _rename_from_train(self):
        name = self.train_name_entry.get_text().strip()
        if not name:
            self._toast("Give him a name first.")
            return

        def done(data, error):
            if error is None:
                self._toast(f"His name is {name}.")
                self._refresh_status_and_waiting()
                self._refresh_train()

        self._command("set", {"key": "name", "value": name}, done=done)

    def _call_his_name(self):
        name = ((self.status or {}).get("state") or {}).get("name") or "Buddy"
        self._toast(f"Calling {name}…")
        self._send_talk_text(name)

    def _remember(self):
        text = self.memory_entry.get_text().strip()
        if not text:
            return
        # A memory is stored by saying it, so the same words are heard by his own vocabulary.
        self._send_talk_text(f"remember that {text}")

    def _refresh_train(self):
        def work():
            known = Api.request("knows", None, 6)
            memories = Api.request("memories", None, 6)
            return known, memories

        def done(result, error):
            if error is not None:
                return
            known, memories = result
            if hasattr(self, "train_name_entry") and not self.train_name_entry.has_focus():
                self.train_name_entry.set_text(str(((self.status or {}).get("state") or {}).get("name") or "Buddy"))
            self._clear(self.known_box)
            rows = known or []
            rows.sort(key=lambda item: (-float(item.get("strength") or 0), str(item.get("cue") or "")))
            for item in rows:
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
                row.add_css_class("known-row")
                cue = Gtk.Label(label=str(item.get("cue") or "?"))
                cue.set_xalign(0)
                cue.set_width_chars(18)
                cue.add_css_class("field-label")
                trick_name = clean(item.get("trick"))
                trick = Gtk.Label(label="—" if trick_name == str(item.get("cue") or "") else trick_name)
                trick.set_xalign(0)
                trick.set_width_chars(12)
                if trick_name == str(item.get("cue") or ""):
                    trick.set_opacity(0.4)
                strength = float(item.get("strength") or 0.0)
                bar = Gtk.ProgressBar()
                bar.set_fraction(max(0.0, min(1.0, strength)))
                bar.set_size_request(120, -1)
                bar.add_css_class("need-bar")
                value = Gtk.Label(label=f"{strength * 100:.0f}%")
                value.add_css_class("need-value")
                source = Gtk.Label(label=titlecase(item.get("source")))
                source.add_css_class("tiny")
                source.set_opacity(0.6)
                row.append(cue)
                row.append(trick)
                row.append(bar)
                row.append(value)
                row.append(source)
                row.append(
                    self._text_button("Forget", lambda c=item.get("cue"): self._unlearn(c), "small-button", "destructive")
                )
                self.known_box.append(row)
            if not rows:
                self.known_box.append(self._empty("He knows no words yet."))

            self._clear(self.memory_box)
            for item in memories or []:
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
                row.add_css_class("memory-row")
                text = Gtk.Label(label=str(item.get("text") or "?"))
                text.set_xalign(0)
                text.set_wrap(True)
                text.set_hexpand(True)
                meta = Gtk.Label(label=f"{titlecase(item.get('source'))} · {item.get('language') or '?'}")
                meta.add_css_class("tiny")
                meta.set_opacity(0.6)
                row.append(text)
                row.append(meta)
                row.append(
                    self._text_button("Forget", lambda i=item.get("id"): self._forget(i), "small-button", "destructive")
                )
                self.memory_box.append(row)
            if not memories:
                self.memory_box.append(self._empty("He has not been told to remember anything yet. Try: remember that I prefer working late."))

        self._run(work, done, busy=False)

    def _unlearn(self, cue):
        def done(data, error):
            if error is None:
                self._refresh_train()

        self._command("unlearn", {"cue": cue}, done=done)

    def _forget(self, identifier):
        def done(data, error):
            if error is None:
                self._refresh_train()

        self._command("forget", {"id": identifier}, done=done)

    # ---------------------------------------------------------------- senses

    def _build_senses(self, body: Gtk.Box):
        privacy = self._card(
            "Privacy",
            "One switch for every sense. Nothing is observed, remembered or sent while it is on.",
        )
        self.privacy_switch = self._switch(False, self._set_privacy, "Suspend every sense")
        privacy.append(self._field_row("Privacy mode", "He keeps living; he just stops noticing.", self.privacy_switch))
        body.append(privacy)

        senses = self._card(
            "What he may notice",
            "Every source is optional and has its own permission. A source can be allowed but not reporting yet.",
        )
        self.senses_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        senses.append(self.senses_box)
        body.append(senses)

        look = self._card(
            "Look once",
            "He looks at the screen or one image and keeps only a one-line impression in memory. The first screen look may ask GNOME to share it.",
        )
        look_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.look_button = self._text_button("Look at the screen", self._look_screen, "small-button", icon="camera-photo-symbolic")
        self.look_file_button = self._text_button("Choose an image…", self._look_file, "small-button", icon="image-x-generic-symbolic")
        look_row.append(self.look_button)
        look_row.append(self.look_file_button)
        look.append(look_row)
        self.look_result = Gtk.Label(label="")
        self.look_result.set_xalign(0)
        self.look_result.set_wrap(True)
        self.look_result.add_css_class("field-hint")
        look.append(self.look_result)
        body.append(look)

        note = self._card("Why some things are not here", "His senses are deliberately separate from his AI. Voice and camera are opt-in.")
        for line in [
            "The screen and window titles are off by default; turn them on here when you want him to use them.",
            "Camera and microphone wake only while allowed, and privacy mode silences both at once.",
            "Accessibility is only the focused control's semantic information, used for guided workflow capture.",
        ]:
            label = Gtk.Label(label="• " + line)
            label.set_xalign(0)
            label.set_wrap(True)
            label.add_css_class("field-hint")
            note.append(label)
        body.append(note)

    def _refresh_senses(self):
        def work():
            return Api.request("senses", None, 6)

        def done(data, error):
            if error is not None:
                return
            self.senses = data
            privacy = bool(data.get("privacy_mode"))
            self._set_switch_value(self.privacy_switch, privacy)
            self._clear(self.senses_box)
            self.sense_switches = {}
            for item in data.get("senses") or []:
                source = str(item.get("source") or "?")
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
                row.add_css_class("sense-row")
                info = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
                info.set_hexpand(True)
                title = Gtk.Label(label=titlecase(source))
                title.set_xalign(0)
                title.add_css_class("field-label")
                what = Gtk.Label(label=str(item.get("what") or ""))
                what.set_xalign(0)
                what.set_wrap(True)
                what.add_css_class("field-hint")
                info.append(title)
                info.append(what)
                reporting = item.get("reporting")
                if reporting:
                    label = Gtk.Label(label=str(reporting))
                    label.set_xalign(0)
                    label.set_wrap(True)
                    label.add_css_class("tiny")
                    label.set_opacity(0.55)
                    info.append(label)
                switch = self._switch(bool(item.get("allowed")), lambda state, s=source: self._set_sense(s, state), f"Allow {source}")
                self.sense_switches[source] = switch
                row.append(info)
                row.append(switch)
                self.senses_box.append(row)

        self._run(work, done, busy=False)

    def _set_privacy(self, value: bool):
        def done(data, error):
            if error is None:
                self._toast("Privacy mode is on. Nothing is being observed." if value else "Privacy mode is off. His allowed senses resume.")
            self._refresh_senses()
            self._refresh_status_and_waiting()

        self._command("privacy", {"on": value}, done=done)

    def _set_sense(self, source: str, value: bool):
        def done(data, error):
            if error is None:
                self._toast(f"{titlecase(source)} is {'on' if value else 'off'}.")
            self._refresh_senses()
            self._refresh_status_and_waiting()

        self._command("sense", {"source": source, "on": value}, done=done)

    def _look_screen(self):
        self.look_button.set_sensitive(False)
        self.look_result.set_text("Looking… (the first time, GNOME may ask which screen to share.)")

        def done(data, error):
            self.look_button.set_sensitive(True)
            if error is not None:
                self.look_result.set_text(f"Could not look: {error}")
                return
            if isinstance(data, dict):
                self.look_result.set_text(
                    f"{data.get('summary') or 'He looked.'}\n"
                    f"Model: {data.get('model') or '?'} · {float(data.get('secs') or 0):.1f} s · only this summary is kept."
                )
            else:
                self.look_result.set_text(str(data))
            self._refresh_status_and_waiting()

        self._command("look", None, done=done, timeout=245)

    def _look_file(self):
        dialog = Gtk.FileDialog()
        dialog.set_title("Choose an image for Buddy to look at")
        image_filter = Gtk.FileFilter()
        image_filter.set_name("Images")
        image_filter.add_mime_type("image/*")
        filters = Gio.ListStore.new(Gtk.FileFilter)
        filters.append(image_filter)
        dialog.set_filters(filters)
        dialog.set_default_filter(image_filter)
        dialog.open(self, None, self._on_look_file)

    def _on_look_file(self, dialog, result):
        try:
            file = dialog.open_finish(result)
        except GLib.Error:
            return
        if file is None:
            return
        path = file.get_path()
        if not path:
            return
        self.look_file_button.set_sensitive(False)
        self.look_result.set_text("Looking at the image…")

        def done(data, error):
            self.look_file_button.set_sensitive(True)
            if error is not None:
                self.look_result.set_text(f"Could not look: {error}")
                return
            if isinstance(data, dict):
                self.look_result.set_text(f"{data.get('summary') or 'He looked.'}\nModel: {data.get('model') or '?'}")
            else:
                self.look_result.set_text(str(data))

        self._command("look", {"file": path}, done=done, timeout=245)

    # ---------------------------------------------------------------- AI

    def _build_ai(self, body: Gtk.Box):
        runtime = self._card(
            "His own local runtime",
            "Buddy manages Ollama for himself on a private port. The system Ollama, if you have one, is untouched.",
        )
        self.ai_available = Gtk.Label(label="Checking…")
        self.ai_available.set_xalign(0)
        self.ai_available.add_css_class("field-label")
        self.ai_detail = Gtk.Label(label="")
        self.ai_detail.set_xalign(0)
        self.ai_detail.set_wrap(True)
        self.ai_detail.add_css_class("field-hint")
        runtime.append(self.ai_available)
        runtime.append(self.ai_detail)
        self.ai_caps_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        runtime.append(self.ai_caps_box)
        buttons = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        buttons.append(self._text_button("Refresh", self._refresh_ai, "small-button"))
        buttons.append(self._text_button("Run doctor", self._ai_doctor, "small-button", icon="dialog-information-symbolic"))
        runtime.append(buttons)
        body.append(runtime)

        models = self._card(
            "Which model does what",
            "Choose from the models installed in his store. A model can do more than one job.",
        )
        self.model_rows = {}
        for key, label, hint in [
            ("language_model", "Language", "Understands requests and helps choose a skill."),
            ("vision_model", "Images", "Looks at the screen or at a picture you choose."),
            ("audio_model", "Audio", "Understands speech when the microphone is on."),
        ]:
            dropdown = Gtk.DropDown()
            dropdown.set_enable_search(False)
            dropdown.set_size_request(220, -1)
            self.model_rows[key] = dropdown
            models.append(self._field_row(label, hint, dropdown))
        self.model_caps = Gtk.Label(label="")
        self.model_caps.set_xalign(0)
        self.model_caps.set_wrap(True)
        self.model_caps.add_css_class("field-hint")
        models.append(self.model_caps)
        models.append(self._text_button("Apply models", self._apply_models, "suggested"))
        body.append(models)

        voice = self._card(
            "Voice",
            "Speech is understood locally by the audio model. The microphone stays off until you turn it on here.",
        )
        self.voice_switch = self._switch(False, self._set_voice, "Listen through the audio model")
        voice.append(self._field_row("Listen", "Say his name, or speak to him within twenty seconds of it.", self.voice_switch))
        self.voice_note = Gtk.Label(label="")
        self.voice_note.set_xalign(0)
        self.voice_note.set_wrap(True)
        self.voice_note.add_css_class("field-hint")
        voice.append(self.voice_note)
        body.append(voice)

        setup = self._card(
            "First-time setup and maintenance",
            "The installer chooses a model for this machine. These terminal commands cover the long, network-facing steps; the rest is available above.",
        )
        for command, note in [
            ("buddy ai setup", "Assess this machine, recommend a model, install it and verify it."),
            ("buddy ai repair", "Re-pull the selected model if it is damaged or missing."),
            ("buddy ai prune", "Remove models his store no longer uses. Read the list before confirming."),
        ]:
            row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
            row.append(self._text_button("Copy", lambda c=command: self._copy(c), "small-button"))
            text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=1)
            text.set_hexpand(True)
            code = Gtk.Label(label=command)
            code.set_xalign(0)
            code.add_css_class("mono")
            hint = Gtk.Label(label=note)
            hint.set_xalign(0)
            hint.set_wrap(True)
            hint.add_css_class("field-hint")
            text.append(code)
            text.append(hint)
            row.append(text)
            setup.append(row)
        body.append(setup)

    def _refresh_ai(self):
        def work():
            return Api.request("providers", None, 22)

        def done(data, error):
            if error is not None:
                self.ai_available.set_text("Could not ask the runtime")
                self.ai_detail.set_text(str(error))
                return
            self.providers = data
            local = data.get("local") or {}
            if local.get("available"):
                self.ai_available.set_text(f"Running · Ollama {local.get('version') or 'unknown version'}")
                self.ai_detail.set_text(
                    f"{local.get('endpoint') or '?'} · models: {', '.join(local.get('models') or []) or 'none installed'}"
                )
                self._clear(self.ai_caps_box)
                caps = (local.get("capabilities") or {}).get("language") or []
                for cap in caps:
                    self.ai_caps_box.append(self._pill(clean(cap), "ok"))
                self._fill_model_dropdowns(local)
                self._update_model_caps(local)
            else:
                self.ai_available.set_text("Runtime not available")
                self.ai_detail.set_text(local.get("why") or "No local model is installed yet.")
                self._clear(self.ai_caps_box)
            self._update_voice_from_status(self.status)
            self._apply_status_to_settings()

        self._run(work, done, busy=False)

    def _fill_model_dropdowns(self, local: dict):
        models = [str(model) for model in (local.get("models") or [])]
        settings = ((self.status or {}).get("settings") or {})
        for key, dropdown in self.model_rows.items():
            chosen = str(settings.get(key) or "")
            choices = list(models)
            if chosen and chosen not in choices:
                choices.append(chosen)
            if not choices:
                choices = ["no model installed"]
            current = choices.index(chosen) if chosen in choices else 0
            dropdown.set_model(Gtk.StringList.new(choices))
            dropdown.set_selected(current)

    def _update_model_caps(self, local: dict):
        caps = local.get("capabilities") or {}
        parts = []
        for label, key in [("Language", "language"), ("Images", "vision"), ("Audio", "audio")]:
            values = caps.get(key) or []
            if values:
                parts.append(f"{label}: {', '.join(clean(v) for v in values)}")
        self.model_caps.set_text(" · ".join(parts) if parts else "No capabilities were reported for these models.")

    def _apply_models(self):
        updates = {}
        for key, dropdown in self.model_rows.items():
            model = dropdown.get_selected_item()
            if model is not None:
                updates[key] = model.get_string()
        if not updates:
            return

        def work():
            for key, value in updates.items():
                Api.request("set", {"key": key, "value": value}, 10)
            return True

        def done(data, error):
            if error is None:
                self._toast("The model choice is saved.")
                self._refresh_ai()
                self._refresh_status_and_waiting()

        self._run(work, done)

    def _ai_doctor(self):
        binary = shutil.which("buddy") or str(Path.home() / ".local" / "bin" / "buddy")

        def work():
            if not Path(binary).exists():
                raise ApiError("The buddy CLI is not on PATH. Run: buddy ai doctor")
            result = subprocess.run(
                [binary, "ai", "doctor"],
                capture_output=True,
                text=True,
                timeout=40,
                check=False,
            )
            return (result.stdout or "") + (("\n" + result.stderr) if result.stderr else "")

        def done(data, error):
            if error is not None:
                self._text_dialog("AI doctor", "Buddy's CLI could not be run.", str(error))
            else:
                self._text_dialog("AI doctor", "The same check the CLI makes.", data or "(no output)")

        self._run(work, done)

    def _update_voice_from_status(self, status):
        if not hasattr(self, "voice_switch") or status is None:
            return
        settings = status.get("settings") or {}
        senses = settings.get("senses") or {}
        mic = bool(senses.get("microphone"))
        if self.voice_switch.get_active() != mic:
            self._set_switch_value(self.voice_switch, mic)
        local = ((self.providers or {}).get("local") or {})
        caps = ((local.get("capabilities") or {}).get("audio") or [])
        if local.get("available") and "audio" in caps:
            self.voice_note.set_text(f"Ready. He will listen through {local.get('understands_audio_with') or 'the audio model'}.")
        elif local.get("available"):
            self.voice_note.set_text("The selected audio model reports no audio input. Choose another model above.")
        else:
            self.voice_note.set_text("The local runtime is not available, so voice is unavailable too.")

    def _set_voice(self, value: bool):
        if not value:
            def off(data, error):
                if error is None:
                    self._toast("Microphone off.")
                self._refresh_status_and_waiting()
            self._command("sense", {"source": "microphone", "on": False}, done=off)
            return

        def work():
            return Api.request("providers", None, 22)

        def check(providers, error):
            if error is not None:
                self._set_switch_value(self.voice_switch, False)
                self._toast(str(error))
                return
            self.providers = providers
            local = providers.get("local") or {}
            caps = ((local.get("capabilities") or {}).get("audio") or [])
            if not local.get("available") or "audio" not in caps:
                self._set_switch_value(self.voice_switch, False)
                self._toast("Choose an audio-capable model before turning the microphone on.")
                return

            def done(data, error):
                if error is None:
                    self._toast("Listening through the local audio model.")
                else:
                    self._set_switch_value(self.voice_switch, False)
                self._refresh_status_and_waiting()

            self._command("sense", {"source": "microphone", "on": True}, done=done)

        self._run(work, check)

    def _copy(self, text: str):
        clipboard = Gdk.Display.get_default().get_clipboard()
        clipboard.set(text)
        self._toast("Copied.")

    # ---------------------------------------------------------------- skills

    def _build_skills(self, body: Gtk.Box):
        intro = self._card(
            "What he may do",
            "Ask means he proposes it and waits for you. Allow means he may go ahead. Deny means he will not.",
        )
        self.skills_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        intro.append(self.skills_box)
        body.append(intro)

        folders = self._card(
            "Folders he may use",
            "He can only search, read or move files inside these folders. Adding a folder is a deliberate permission.",
        )
        self.folders_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        folders.append(self.folders_box)
        folder_buttons = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        folder_buttons.append(self._text_button("Add a folder…", self._add_folder, "small-button", icon="folder-new-symbolic"))
        folder_buttons.append(self._text_button("Refresh", self._refresh_folders, "small-button"))
        folders.append(folder_buttons)
        self.commands_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        folders.append(self.commands_box)
        body.append(folders)

        undo = self._card(
            "The last change",
            "A note, a moved file or a command can be taken back if he kept the way.",
        )
        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        row.append(self._text_button("Undo the last change", self._undo, "small-button", icon="edit-undo-symbolic"))
        row.append(self._text_button("Show the full result", self._show_result, "small-button", icon="document-open-symbolic"))
        undo.append(row)
        body.append(undo)

        waiting = self._card("Waiting for your answer", "The same list as Home, available while you are configuring him.")
        self.skills_waiting_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        waiting.append(self.skills_waiting_box)
        body.append(waiting)

    def _refresh_skills(self):
        def work():
            skills = Api.request("skills", None, 6)
            folders = Api.request("folders", None, 6)
            waiting = Api.request("waiting", None, 6)
            return skills, folders, waiting

        def done(result, error):
            if error is not None:
                return
            skills, folders, waiting = result
            self._clear(self.skills_box)
            for item in skills or []:
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
                row.add_css_class("skill-row")
                text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
                text.set_hexpand(True)
                name = Gtk.Label(label=clean(item.get("name")))
                name.set_xalign(0)
                name.add_css_class("field-label")
                what = Gtk.Label(label=str(item.get("what") or ""))
                what.set_xalign(0)
                what.set_wrap(True)
                what.add_css_class("field-hint")
                risk = Gtk.Label(label=titlecase(item.get("risk")))
                risk.set_xalign(0)
                risk.add_css_class("tiny")
                risk.set_opacity(0.55)
                text.append(name)
                text.append(what)
                text.append(risk)
                row.append(text)
                sensitive = "sensitive" in str(item.get("risk") or "")
                choices = ["ask", "deny"] if sensitive else ["ask", "allow", "deny"]
                dropdown = Gtk.DropDown.new_from_strings(choices)
                current = str(item.get("policy") or "ask")
                dropdown.set_selected(choices.index(current) if current in choices else 0)
                dropdown.set_size_request(130, -1)
                dropdown.connect("notify::selected", lambda dd, s=item: self._set_skill_policy(s, dd, choices))
                row.append(dropdown)
                self.skills_box.append(row)

            self._clear(self.folders_box)
            folder_list = (folders or {}).get("folders") or []
            for folder in folder_list:
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
                row.add_css_class("memory-row")
                label = Gtk.Label(label=str(folder))
                label.set_xalign(0)
                label.set_wrap(True)
                label.set_hexpand(True)
                label.add_css_class("mono")
                row.append(label)
                row.append(self._text_button("Remove", lambda f=folder: self._remove_folder(f), "small-button", "destructive"))
                self.folders_box.append(row)
            if not folder_list:
                self.folders_box.append(self._empty("No folders yet. He cannot search or move files until you add one."))

            self._clear(self.commands_box)
            commands = (folders or {}).get("commands") or []
            for command in commands:
                label = Gtk.Label(label=f"Command allowed for good: {command}")
                label.set_xalign(0)
                label.set_wrap(True)
                label.add_css_class("tiny")
                self.commands_box.append(label)

            self._clear(self.skills_waiting_box)
            if not waiting:
                self.skills_waiting_box.append(self._empty("Nothing is waiting for an answer."))
            for item in waiting or []:
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
                text = Gtk.Label(label=str(item.get("would") or "something"))
                text.set_xalign(0)
                text.set_hexpand(True)
                text.set_wrap(True)
                row.append(text)
                identifier = item.get("id")
                row.append(self._text_button("Approve", lambda i=identifier: self._answer(i, True, False), "small-button", "suggested"))
                row.append(self._text_button("Deny", lambda i=identifier: self._answer(i, False, False), "small-button"))
                self.skills_waiting_box.append(row)

        self._run(work, done, busy=False)

    def _set_skill_policy(self, item: dict, dropdown: Gtk.DropDown, choices: list[str]):
        selected = dropdown.get_selected()
        if selected < 0 or selected >= len(choices):
            return
        policy = choices[selected]
        if policy == str(item.get("policy")):
            return
        name = item.get("name")
        def done(data, error):
            if error is None:
                self._toast(f"{clean(name)} is now {policy}.")
            self._refresh_skills()
        self._command("skill", {"name": name, "policy": policy}, done=done)

    def _refresh_folders(self):
        self._refresh_skills()

    def _add_folder(self):
        dialog = Gtk.FileDialog()
        dialog.set_title("Choose a folder Buddy may use")
        dialog.select_folder(self, None, self._on_add_folder)

    def _on_add_folder(self, dialog, result):
        try:
            file = dialog.select_folder_finish(result)
        except GLib.Error:
            return
        path = file.get_path() if file else None
        if not path:
            return
        def done(data, error):
            if error is None:
                self._toast(f"Folder added: {path}")
                self._refresh_skills()
        self._command("folders", {"add": path}, done=done)

    def _remove_folder(self, folder):
        def done(data, error):
            if error is None:
                self._refresh_skills()
        self._command("folders", {"remove": folder}, done=done)

    def _undo(self):
        def done(data, error):
            if error is not None:
                return
            if isinstance(data, dict) and data.get("undid"):
                self._toast(f"Taken back: {data['undid']}")
            self._refresh_skills()
        self._command("undo", None, done=done, timeout=90)

    def _show_result(self):
        def work():
            return Api.request("result", None, 8)

        def done(data, error):
            if error is not None:
                self._text_dialog("The last result", "There is nothing to show.", str(error))
            else:
                self._text_dialog("The last result", "The full output of the last thing he did.", str(data or ""))

        self._run(work, done)

    # ---------------------------------------------------------------- workflows

    def _build_workflows(self, body: Gtk.Box):
        record = self._card(
            "Record an application routine",
            "Start, work through the applications you use, then stop. He records which application came to the front, not the keys you pressed.",
        )
        self.record_name = self._entry("a name, e.g. “morning mail”")
        record.append(self._field_row("Name", "Only applications, in order, are stored.", self.record_name))
        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        row.append(self._text_button("Start recording", lambda: self._record("start"), "small-button", "suggested"))
        row.append(self._text_button("Stop", lambda: self._record("stop"), "small-button"))
        row.append(self._text_button("Cancel", lambda: self._record("cancel"), "small-button", "destructive"))
        record.append(row)
        body.append(record)

        capture = self._card(
            "Capture actions, text and assisted steps",
            "Start a capture, then use the buttons or the Super+Shift+B shortcut. Accessible actions and text need the accessibility sense and privacy off. "
            "An application with no accessibility tree can use a manual step: Buddy brings it forward and waits while you do the work.",
        )
        self.capture_name = self._entry("a name, e.g. “file my report”")
        capture.append(self._field_row("Name", "A workflow can have several steps across applications.", self.capture_name))
        row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        row.append(self._text_button("Start capture", lambda: self._capture("start"), "small-button", "suggested"))
        row.append(self._text_button("Capture step", self._capture_step, "small-button", icon="input-keyboard-symbolic"))
        row.append(self._text_button("Stop", lambda: self._capture("stop"), "small-button"))
        row.append(self._text_button("Cancel", lambda: self._capture("cancel"), "small-button", "destructive"))
        capture.append(row)

        divider = Gtk.Label(label="Text entry")
        divider.set_xalign(0)
        divider.add_css_class("field-label")
        divider.set_margin_top(6)
        capture.append(divider)
        self.text_step_entry = self._entry("text to enter, e.g. “quarterly report”")
        capture.append(self._field_row("Text", "Stored as part of the workflow in his database. Do not use passwords or secrets.", self.text_step_entry))
        text_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.text_delay = Gtk.SpinButton.new_with_range(1, 15, 1)
        self.text_delay.set_value(5)
        self.text_delay.set_size_request(90, -1)
        text_row.append(self._text_button("Capture text step", self._capture_text, "small-button", "suggested"))
        text_row.append(Gtk.Label(label="after"))
        text_row.append(self.text_delay)
        text_row.append(Gtk.Label(label="seconds; focus the target field during the countdown."))
        capture.append(text_row)

        divider = Gtk.Label(label="Application without an accessibility tree")
        divider.set_xalign(0)
        divider.add_css_class("field-label")
        divider.set_margin_top(6)
        capture.append(divider)
        self.manual_note_entry = self._entry("what you will do, e.g. “send the prepared file”")
        self.manual_app_entry = self._entry("application id; leave blank and focus it during the countdown")
        capture.append(self._field_row("Manual step", "No keystrokes, coordinates or blind clicks are used. You perform it; Buddy only brings the app forward and waits.", self.manual_note_entry))
        capture.append(self._field_row("Application", "Optional when the countdown leaves you time to focus it.", self.manual_app_entry))
        manual_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        self.manual_delay = Gtk.SpinButton.new_with_range(1, 30, 1)
        self.manual_delay.set_value(5)
        self.manual_delay.set_size_request(90, -1)
        manual_row.append(self._text_button("Add manual step", self._capture_manual, "small-button", "suggested"))
        manual_row.append(Gtk.Label(label="after"))
        manual_row.append(self.manual_delay)
        manual_row.append(Gtk.Label(label="seconds."))
        capture.append(manual_row)
        self.capture_note = Gtk.Label(label="Coordinates, keystrokes and password fields are never recorded. A manual step is explicitly assisted; it is not automated.")
        self.capture_note.set_xalign(0)
        self.capture_note.set_wrap(True)
        self.capture_note.add_css_class("field-hint")
        capture.append(self.capture_note)
        body.append(capture)

        listing = self._card(
            "What he has recorded",
            "Replaying checks each application came to the front and each accessible control still exists. A manual step waits for you.",
            action=self._text_button("Refresh", self._refresh_workflows, "small-button"),
        )
        self.manual_bar = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        self.manual_bar.add_css_class("waiting-row")
        self.manual_bar.set_visible(False)
        self.manual_label = Gtk.Label(label="")
        self.manual_label.set_xalign(0)
        self.manual_label.set_wrap(True)
        self.manual_label.add_css_class("field-label")
        manual_buttons = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        manual_buttons.append(self._text_button("I did it", self._continue_workflow, "small-button", "suggested"))
        manual_buttons.append(self._text_button("Cancel replay", self._abort_workflow, "small-button", "destructive"))
        self.manual_bar.append(self.manual_label)
        self.manual_bar.append(manual_buttons)
        listing.append(self.manual_bar)
        self.workflows_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        listing.append(self.workflows_box)
        body.append(listing)

    def _record(self, action: str):
        args = {"action": action}
        if action == "start":
            name = self.record_name.get_text().strip()
            if not name:
                self._toast("Give the routine a name first.")
                return
            args["name"] = name

        def done(data, error):
            if error is None:
                if action == "start":
                    self._toast(f"Recording “{args['name']}”. Work through the applications, then press Stop.")
                elif action == "stop":
                    self._toast("The routine is saved.")
                self._refresh_workflows()

        self._command("workflow_record", args, done=done)

    def _capture(self, action: str):
        args = {"action": action}
        if action == "start":
            name = self.capture_name.get_text().strip()
            if not name:
                self._toast("Give the capture a name first.")
                return
            args["name"] = name

        def done(data, error):
            if error is None:
                if action == "start":
                    self._toast(f"Capturing “{args['name']}”. Focus a button or menu item and capture it.")
                elif action == "stop":
                    self._toast("The captured routine is saved.")
                self._refresh_workflows()

        self._command("workflow_capture", args, done=done)

    def _capture_step(self):
        def done(data, error):
            if error is None:
                if isinstance(data, dict) and data.get("captured"):
                    step = data["captured"]
                    self._toast(f"Captured {clean(step.get('role'))} “{step.get('name')}”.")
                else:
                    self._toast("Captured the focused control.")
            else:
                self._toast(str(error))

        self._command("workflow_capture_step", None, done=done)

    def _schedule(self, seconds: int, action):
        def fire():
            action()
            return False

        GLib.timeout_add(max(1, seconds) * 1000, fire)

    def _capture_text(self):
        text = self.text_step_entry.get_text()
        if not text:
            self._toast("Type the text to enter first.")
            return
        settings = (self.status or {}).get("settings") or {}
        if (settings.get("senses") or {}).get("accessibility") is not True or settings.get("privacy_mode"):
            self._toast("Turn on the accessibility sense and turn privacy off first.")
            return
        seconds = int(self.text_delay.get_value())
        self._toast(f"Focus the text field; capturing in {seconds} seconds…")

        def send():
            def done(data, error):
                if error is None:
                    self._toast("Text step captured.")

            self._command("workflow_type_step", {"text": text}, done=done)

        self._schedule(seconds, send)

    def _capture_manual(self):
        note = self.manual_note_entry.get_text().strip()
        if not note:
            self._toast("Describe what you will do in the manual step first.")
            return
        app = self.manual_app_entry.get_text().strip() or None
        seconds = int(self.manual_delay.get_value())
        self._toast(f"Focus the application; recording the manual step in {seconds} seconds…")

        def send():
            def done(data, error):
                if error is None:
                    self._toast("Manual step captured.")

            self._command("workflow_manual_step", {"note": note, "app": app}, done=done)

        self._schedule(seconds, send)

    def _show_manual_prompt(self, step: dict):
        self._manual_step = step
        text = f"Step {step.get('step', '?')}/{step.get('total', '?')} in {step.get('app', '?')}: {step.get('note', '')}"
        if hasattr(self, "manual_bar"):
            self.manual_label.set_text(text)
            self.manual_bar.set_visible(True)
        dialog = Adw.MessageDialog(transient_for=self, heading="Buddy is waiting for you", body=text)
        dialog.add_response("later", "Later")
        dialog.add_response("done", "I did it")
        dialog.set_response_appearance("done", Adw.ResponseAppearance.SUGGESTED)
        dialog.set_default_response("done")
        dialog.set_close_response("later")

        def response(dlg, choice):
            dlg.close()
            if choice == "done":
                self._continue_workflow()

        dialog.connect("response", response)
        dialog.present()

    def _continue_workflow(self):
        def done(data, error):
            if error is not None:
                self._toast(str(error))
                return
            if isinstance(data, dict) and data.get("waiting_for_manual"):
                self._show_manual_prompt(data["waiting_for_manual"])
                return
            if hasattr(self, "manual_bar"):
                self.manual_bar.set_visible(False)
            self._manual_step = None
            if isinstance(data, dict) and data.get("did"):
                self._toast(f"Done: {data['did']}")
            else:
                self._toast("The workflow is finished.")
            self._refresh_workflows()

        self._command("workflow_continue", None, done=done, timeout=16 * 60)

    def _abort_workflow(self):
        def done(data, error):
            if hasattr(self, "manual_bar"):
                self.manual_bar.set_visible(False)
            self._manual_step = None
            if error is None:
                self._toast("The replay is stopped.")
                self._refresh_workflows()

        self._command("workflow_abort", None, done=done)

    def _refresh_workflows(self):
        def work():
            return Api.request("workflows", None, 6)

        def done(data, error):
            if error is not None:
                return
            self._clear(self.workflows_box)
            workflows = (data or {}).get("workflows") or []
            if not workflows:
                self.workflows_box.append(
                    self._empty("No routines yet. Record applications, or capture accessible actions, above.")
                )
                return
            for item in workflows:
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
                row.add_css_class("workflow-row")
                text = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
                text.set_hexpand(True)
                name = Gtk.Label(label=str(item.get("name") or "?"))
                name.set_xalign(0)
                name.add_css_class("field-label")
                apps = [str(a) for a in (item.get("apps") or [])]
                steps = item.get("steps") or []
                summary = f"{len(steps)} step{'s' if len(steps) != 1 else ''}"
                if apps:
                    summary += " · " + " → ".join(apps)
                text_steps = sum(1 for step in steps if step.get("kind") == "type_text")
                manual_steps = sum(1 for step in steps if step.get("kind") == "manual")
                if text_steps:
                    summary += f" · {text_steps} text"
                if manual_steps:
                    summary += f" · {manual_steps} manual"
                meta = Gtk.Label(label=f"#{item.get('id')} · {summary}")
                meta.set_xalign(0)
                meta.set_wrap(True)
                meta.add_css_class("field-hint")
                record = Gtk.Label(
                    label=f"{item.get('runs', 0)} runs · {item.get('successes', 0)} worked · {item.get('failures', 0)} failed"
                )
                record.set_xalign(0)
                record.add_css_class("tiny")
                record.set_opacity(0.55)
                text.append(name)
                text.append(meta)
                text.append(record)
                row.append(text)
                row.append(self._text_button("Show", lambda i=item: self._show_workflow(i), "small-button"))
                row.append(self._text_button("Replay", lambda i=item: self._replay_workflow(i), "small-button", "suggested"))
                row.append(self._text_button("Delete", lambda i=item: self._delete_workflow(i), "small-button", "destructive"))
                self.workflows_box.append(row)

        self._run(work, done, busy=False)

    def _show_workflow(self, item):
        identifier = item.get("id")

        def work():
            return Api.request("workflow", {"id": identifier}, 6)

        def done(data, error):
            if error is not None:
                self._text_dialog("Workflow", "Could not load it.", str(error))
            else:
                self._text_dialog(
                    f"Workflow: {item.get('name')}",
                    "Stored steps and how it has gone.",
                    json.dumps(data, indent=2, ensure_ascii=False),
                )

        self._run(work, done)

    def _replay_workflow(self, item):
        name = item.get("name")

        def done(data, error):
            if error is not None:
                return
            if isinstance(data, dict) and data.get("waiting_for_manual"):
                self._show_manual_prompt(data["waiting_for_manual"])
            elif isinstance(data, dict) and data.get("waiting_for_yes"):
                self._show_approval(data, self.talk_approval_box)
                self._toast("He is waiting for your answer on Home or Talk.")
                self._refresh_status_and_waiting()
            elif isinstance(data, dict) and data.get("did"):
                self._toast(f"Done: {data['did']}")
                self._refresh_workflows()
            else:
                self._toast("Replay finished.")

        self._command("replay_workflow", {"workflow": name}, done=done, timeout=16 * 60)

    def _delete_workflow(self, item):
        dialog = Adw.MessageDialog(
            transient_for=self,
            heading=f"Delete “{item.get('name')}”?",
            body="The recorded routine is removed. This cannot be undone.",
        )
        dialog.add_response("cancel", "Keep it")
        dialog.add_response("delete", "Delete")
        dialog.set_response_appearance("delete", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.set_default_response("cancel")
        dialog.set_close_response("cancel")

        def response(dlg, choice):
            dlg.close()
            if choice != "delete":
                return
            def done(data, error):
                if error is None:
                    self._toast("The routine is deleted.")
                    self._refresh_workflows()
            self._command("workflow_delete", {"id": item.get("id")}, done=done)

        dialog.connect("response", response)
        dialog.present()

    # ---------------------------------------------------------------- journal

    def _build_journal(self, body: Gtk.Box):
        card = self._card(
            "Recent events",
            "The daemon's own journal: what he did, what he noticed, and what changed.",
            action=self._text_button("Refresh", self._refresh_journal, "small-button"),
        )
        self.journal_box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        card.append(self.journal_box)
        body.append(card)

        export = self._card(
            "Keep a copy",
            "Export everything he knows as JSON: identity, life, mind, settings, memories and workflows.",
        )
        export.append(self._text_button("Export to his data folder", self._export, "small-button", icon="document-save-symbolic"))
        self.export_note = Gtk.Label(label="")
        self.export_note.set_xalign(0)
        self.export_note.set_wrap(True)
        self.export_note.add_css_class("field-hint")
        export.append(self.export_note)
        body.append(export)

    def _refresh_journal(self):
        def work():
            return Api.request("journal", {"limit": 60}, 8)

        def done(data, error):
            if error is not None:
                return
            self._clear(self.journal_box)
            entries = data or []
            if not entries:
                self.journal_box.append(self._empty("Nothing has happened yet."))
                return
            for entry in reversed(entries):
                row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
                row.add_css_class("journal-row")
                ts = Gtk.Label(label=when(entry.get("ts")))
                ts.set_xalign(0)
                ts.set_width_chars(13)
                ts.add_css_class("tiny")
                ts.set_opacity(0.55)
                kind = Gtk.Label(label=clean(entry.get("kind")))
                kind.set_xalign(0)
                kind.set_width_chars(18)
                kind.add_css_class("field-label")
                data_label = Gtk.Label(label=brief(entry.get("data"), 220))
                data_label.set_xalign(0)
                data_label.set_wrap(True)
                data_label.set_hexpand(True)
                data_label.add_css_class("tiny")
                data_label.set_opacity(0.65)
                row.append(ts)
                row.append(kind)
                row.append(data_label)
                self.journal_box.append(row)

        self._run(work, done, busy=False)

    def _export(self):
        destination = data_dir() / f"export-{datetime.datetime.now().strftime('%Y%m%d-%H%M%S')}.json"

        def work():
            data = Api.request("export", None, 30)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(json.dumps(data, indent=2, ensure_ascii=False), encoding="utf-8")
            return destination

        def done(path, error):
            if error is not None:
                self.export_note.set_text(f"Export failed: {error}")
                return
            self.export_note.set_text(f"Saved to {path}")
            self._toast("A JSON copy is saved.")

        self._run(work, done)

    # ---------------------------------------------------------------- settings

    def _build_settings(self, body: Gtk.Box):
        identity = self._card("Name and body", "These are his, not this window's. Changes are saved by the daemon.")
        self.name_entry = self._entry("Buddy")
        identity.append(self._field_row("Name", "What you call him.", self.name_entry))
        name_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        name_row.append(self._text_button("Rename", self._rename, "small-button", "suggested"))
        identity.append(name_row)

        self.size_spin = Gtk.SpinButton.new_with_range(180, 1200, 10)
        self.size_spin.set_value(420)
        identity.append(self._field_row("Adult height", "How large he will be when fully grown, in pixels.", self.size_spin))
        self.adult_spin = Gtk.SpinButton.new_with_range(30, 2000, 10)
        self.adult_spin.set_value(365)
        identity.append(self._field_row("Grow up over", "Days from puppy to adult.", self.adult_spin))
        self.apply_body_button = self._text_button("Apply body settings", self._apply_body_settings, "small-button")
        identity.append(self.apply_body_button)
        body.append(identity)

        window = self._card("His window", "The renderer is replaceable; he keeps living without it.")
        self.fullscreen_switch = self._switch(False, self._set_fullscreen, "Show him inside fullscreen applications")
        window.append(self._field_row("In fullscreen applications", "When off, he steps out of the way.", self.fullscreen_switch))
        self.pictures_dropdown = Gtk.DropDown.new_from_strings(["away", "always", "off"])
        self.pictures_dropdown.set_size_request(160, -1)
        self.pictures_dropdown.connect("notify::selected", self._set_pictures)
        window.append(self._field_row("Pre-drawn pictures", "Away draws them only while you are away; always draws them whenever needed; off uses the live model.", self.pictures_dropdown))
        self.hide_show_button = self._text_button("Hide him", lambda: self._quick("hide"), "small-button")
        self.sleep_wake_button = self._text_button("Let him sleep", lambda: self._quick("sleep"), "small-button")
        actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        actions.append(self.hide_show_button)
        actions.append(self.sleep_wake_button)
        window.append(actions)
        body.append(window)

        sound = self._card("Sound and attention", "Quiet means no sounds and no asking for attention. He still lives.")
        self.quiet_switch = self._switch(False, self._set_quiet, "No sounds")
        sound.append(self._field_row("Quiet", "No sounds, no nudges.", self.quiet_switch))
        self.settings_privacy_switch = self._switch(False, self._set_privacy, "Suspend every sense")
        sound.append(self._field_row("Privacy", "Suspend every sense at once.", self.settings_privacy_switch))
        body.append(sound)

        data = self._card("His data", "One SQLite database, a daily backup beside it, and his notes.")
        self.data_path = Gtk.Label(label=str(data_dir() / "buddy.db"))
        self.data_path.set_xalign(0)
        self.data_path.set_selectable(True)
        self.data_path.set_wrap(True)
        self.data_path.add_css_class("mono")
        data.append(self._field_row("Database", None, self.data_path))
        self.schema_label = Gtk.Label(label="—")
        data.append(self._field_row("Schema", None, self.schema_label))
        open_data = self._text_button("Open data folder", self._open_data, "small-button", icon="folder-open-symbolic")
        data.append(open_data)
        body.append(data)

        about = self._card("About", "Buddy is a local-first desktop dog. He does not need a language model or a network to live.")
        about.append(self._text_button("About Buddy", self.app.show_about, "small-button"))
        body.append(about)

    def _apply_status_to_settings(self):
        if self.status is None or not hasattr(self, "name_entry"):
            return
        state = self.status.get("state") or {}
        settings = self.status.get("settings") or {}
        self._updating = True
        try:
            if not self.name_entry.has_focus():
                self.name_entry.set_text(str(state.get("name") or "Buddy"))
            try:
                self.size_spin.set_value(float(settings.get("size_px") or 420))
                self.adult_spin.set_value(float(settings.get("adult_days") or 365))
            except (TypeError, ValueError):
                pass
            self.fullscreen_switch.set_active(bool(settings.get("stay_in_fullscreen")))
            self.quiet_switch.set_active(bool(settings.get("quiet")))
            privacy = bool(state.get("privacy")) or bool(settings.get("privacy_mode"))
            self.settings_privacy_switch.set_active(privacy)
            pictures = str(settings.get("pictures") or "away")
            choices = ["away", "always", "off"]
            if pictures in choices:
                # Avoid triggering a set while we are only reflecting the daemon's state.
                self._pictures_guard = True
                self.pictures_dropdown.set_selected(choices.index(pictures))
                self._pictures_guard = False
            self.schema_label.set_text(f"v{self.status.get('schema_version', '?')}")
            self.hide_show_button.set_label("Show him" if state.get("hidden") else "Hide him")
            self.sleep_wake_button.set_label("Wake him" if settings.get("asleep_by_user") else "Let him sleep")
        finally:
            self._updating = False

    def _rename(self):
        name = self.name_entry.get_text().strip()
        if not name:
            return
        def done(data, error):
            if error is None:
                self._toast(f"His name is {name}.")
                self._refresh_status_and_waiting()
        self._command("set", {"key": "name", "value": name}, done=done)

    def _apply_body_settings(self):
        size = str(int(self.size_spin.get_value()))
        days = str(int(self.adult_spin.get_value()))

        def work():
            Api.request("set", {"key": "size_px", "value": size}, 10)
            Api.request("set", {"key": "adult_days", "value": days}, 10)
            return True

        def done(data, error):
            if error is None:
                self._toast("His body settings are saved.")
                self._refresh_status_and_waiting()

        self._run(work, done)

    def _set_fullscreen(self, value: bool):
        self._command("set", {"key": "stay_in_fullscreen", "value": "on" if value else "off"}, done=lambda *_: self._refresh_status_and_waiting())

    def _set_pictures(self, dropdown, _param):
        if getattr(self, "_pictures_guard", False):
            return
        item = dropdown.get_selected_item()
        if item is None:
            return
        value = item.get_string()
        self._command("set", {"key": "pictures", "value": value}, done=lambda *_: self._refresh_status_and_waiting())

    def _set_quiet(self, value: bool):
        self._command("quiet", {"on": value}, done=lambda *_: self._refresh_status_and_waiting())

    def _open_data(self):
        path = data_dir()
        try:
            Gio.AppInfo.launch_default_for_uri(path.as_uri(), None)
        except GLib.Error as error:
            self._toast(f"Could not open {path}: {error.message}")

    # ---------------------------------------------------------------- keyboard

    def go_page(self, index: int):
        row = self.nav.get_row_at_index(max(0, min(index, 8)))
        if row is not None:
            self.nav.select_row(row)

    def refresh_from_action(self):
        self._refresh_all()


class BuddyApp(Adw.Application):
    def __init__(self):
        super().__init__(application_id=APP_ID, flags=Gio.ApplicationFlags.DEFAULT_FLAGS)
        self.window: BuddyWindow | None = None
        self.style_manager = Adw.StyleManager.get_default()

    def do_startup(self):
        Adw.Application.do_startup(self)
        self._load_css()
        actions = [
            ("refresh", self._action_refresh),
            ("quit", self._action_quit),
            ("data-folder", self._action_data_folder),
            ("about", self._action_about),
        ]
        for name, callback in actions:
            action = Gio.SimpleAction.new(name, None)
            action.connect("activate", callback)
            self.add_action(action)
        appearance = Gio.SimpleAction.new("appearance", GLib.VariantType.new("s"))
        appearance.connect("activate", self._action_appearance)
        self.add_action(appearance)
        self.set_accels_for_action("app.refresh", ["<Primary>r"])
        self.set_accels_for_action("app.quit", ["<Primary>q"])
        self.set_accels_for_action("app.about", ["F1"])

    def _load_css(self):
        provider = Gtk.CssProvider()
        css = HERE / "buddy.css"
        try:
            provider.load_from_path(str(css))
            display = Gdk.Display.get_default()
            if display is not None:
                Gtk.StyleContext.add_provider_for_display(display, provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        except GLib.Error as error:
            print(f"buddy-ui: stylesheet: {error.message}", file=sys.stderr)

    def do_activate(self):
        if self.window is None:
            self.window = BuddyWindow(self)
            controller = Gtk.EventControllerKey()
            controller.connect("key-pressed", self._on_key)
            self.window.add_controller(controller)
            page = os.environ.get("BUDDY_UI_PAGE", "").strip().lower()
            if page in PAGE_TITLES:
                self.window.go_page(list(PAGE_TITLES).index(page))
        self.window.present()

    def _on_key(self, controller, keyval, keycode, state):
        ctrl = bool(state & Gdk.ModifierType.CONTROL_MASK)
        if not ctrl:
            return False
        if keyval in (Gdk.KEY_r, Gdk.KEY_R):
            self.window.refresh_from_action()
            return True
        if keyval in (Gdk.KEY_q, Gdk.KEY_Q):
            self.quit()
            return True
        if Gdk.KEY_1 <= keyval <= Gdk.KEY_9:
            self.window.go_page(keyval - Gdk.KEY_1)
            return True
        return False

    def _action_refresh(self, action, parameter):
        if self.window:
            self.window.refresh_from_action()

    def _action_quit(self, action, parameter):
        self.quit()

    def _action_data_folder(self, action, parameter):
        try:
            Gio.AppInfo.launch_default_for_uri(data_dir().as_uri(), None)
        except GLib.Error as error:
            if self.window:
                self.window._toast(f"Could not open the data folder: {error.message}")

    def _action_appearance(self, action, parameter):
        value = parameter.get_string() if parameter is not None else "system"
        schemes = {
            "system": Adw.ColorScheme.DEFAULT,
            "light": Adw.ColorScheme.FORCE_LIGHT,
            "dark": Adw.ColorScheme.FORCE_DARK,
        }
        self.style_manager.set_color_scheme(schemes.get(value, Adw.ColorScheme.DEFAULT))

    def _action_about(self, action, parameter):
        self.show_about()

    def show_about(self):
        about = Adw.AboutWindow(
            transient_for=self.window,
            application_name="Buddy",
            application_icon="buddy",
            version=APP_VERSION,
            developer_name="The Buddy project",
            comments="A Golden Retriever who lives on your desktop. Local first; every sense optional.",
            license_type=Gtk.License.MIT_X11,
        )
        about.set_developers(["The Buddy project"])
        about.present()


def main():
    GLib.set_prgname("buddy-ui")
    GLib.set_application_name("Buddy")
    app = BuddyApp()
    return app.run(sys.argv)


if __name__ == "__main__":
    raise SystemExit(main())
