#!/usr/bin/env python3
"""Observe semantic text changes in the focused accessible control, safely.

Buddy starts this only while the opt-in `context` sense is allowed and privacy
mode is off. It is a long-running provider: every debounced text-change burst
prints one JSON line with a short summary, never the text itself.

Safety rules, in order:
  * only the accessibility tree, never raw keys, clipboard or screen pixels;
  * only a control with the FOCUSED state and an editable text interface;
  * never a password field, terminal, shell, password manager, login/lock,
    consent or portal surface (application and frame title are both checked);
  * no text in stdout, no history, no journal; the daemon keeps one bounded
    summary for two minutes and can clear it.

Prints JSON lines:
  {"kind": "typing", "data": {"app": ..., "kind": ..., "words": ..., "chars": ...}}
  {"kind": "unavailable", "data": {"why": ...}}
"""
import json
import re
import sys

try:
    import gi

    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi, GLib
except Exception as error:  # noqa: BLE001 - the daemon must learn why this cannot run
    print(json.dumps({"kind": "unavailable", "data": {"why": f"AT-SPI is not available: {error}"}}), flush=True)
    sys.exit(1)

# Application names are matched case-insensitively. Tokens catch variants such
# as "org.gnome.Terminal" and "gnome-terminal-server".
BLOCKED_APPS = {
    "gnome-shell",
    "gdm",
    "gdm3",
    "sddm",
    "lightdm",
    "polkit",
    "policykit",
    "gnome-session",
    "xdg-desktop-portal",
    "xdg-desktop-portal-gnome",
    "xdg-desktop-portal-gtk",
    "buddy",
    "buddy-ui",
    "buddy-tray",
    "keepassxc",
    "bitwarden",
    "1password",
    "seahorse",
}
BLOCKED_APP_TOKENS = ("terminal", "konsole", "alacritty", "kitty", "xterm", "console", "shell", "password", "keyring")
BLOCKED_FRAME_WORDS = (
    "password",
    "passwd",
    "unlock",
    "lock screen",
    "authentication",
    "authenticate",
    "consent",
    "permission",
    "remote desktop",
    "share your screen",
    "screen sharing",
    "policykit",
    "polkit",
    "log in",
    "login",
    "sign in",
)
MAX_TEXT = 2000
DEBOUNCE_MS = 1200

URL_RE = re.compile(r"^(https?://|www\.)", re.IGNORECASE)
EMAIL_RE = re.compile(r"^[^\s@]+@[^\s@]+\.[^\s@]+$")
CODE_HINTS = ("fn ", "def ", "function ", "class ", "import ", "#include", "=>", "</", "{", "}", "SELECT ", "sudo ", "#!/")


def blocked_app(name):
    lowered = (name or "").strip().lower()
    if not lowered:
        return True
    if lowered in BLOCKED_APPS:
        return True
    return any(token in lowered for token in BLOCKED_APP_TOKENS)


def blocked_frame(name):
    lowered = (name or "").strip().lower()
    return not lowered or any(word in lowered for word in BLOCKED_FRAME_WORDS)


def frame_of(node):
    current = node
    while current is not None:
        try:
            if current.get_role_name() in ("frame", "window"):
                return current
            current = current.get_parent()
        except Exception:
            return None
    return None


def classify(text):
    stripped = text.strip()
    if not stripped:
        return "empty"
    words = stripped.split()
    if URL_RE.match(stripped):
        return "url"
    if EMAIL_RE.match(stripped) or (len(words) <= 2 and "@" in stripped and "." in stripped):
        return "email"
    if any(hint in stripped for hint in CODE_HINTS):
        return "code"
    lowered = stripped.lower()
    if stripped.endswith("?") or lowered.startswith(("how ", "what ", "why ", "where ", "when ", "who ", "search ")):
        return "search"
    if "\n" in stripped or len(words) > 8 or stripped.endswith("."):
        return "prose"
    return "form"


def summary_from(source):
    """A short, non-textual description of the focused editable control, or None."""
    try:
        if source is None:
            return None
        state = source.get_state_set()
        if state is None or not state.contains(Atspi.StateType.FOCUSED):
            return None
        if source.get_role_name() in ("password text", "terminal"):
            return None
        if not source.is_editable_text():
            return None
        app = source.get_application()
        app_name = app.get_name() if app is not None else ""
        if blocked_app(app_name):
            return None
        frame = frame_of(source)
        if frame is None or blocked_frame(frame.get_name()):
            return None
        count = Atspi.Text.get_character_count(source)
        if count <= 0:
            return None
        text = Atspi.Text.get_text(source, 0, min(count, MAX_TEXT))
        if not text or not text.strip():
            return None
        words = len(text.split())
        kind = classify(text)
        # Only counts and a classification leave this process.
        return {"app": app_name[:80], "kind": kind, "words": min(words, 100_000), "chars": min(len(text), 1_000_000)}
    except Exception:
        return None


class Observer:
    def __init__(self):
        self.pending = None
        self.timer = None
        self.listener = Atspi.EventListener.new(self.on_event)

    def start(self):
        if not self.listener.register("object:text-changed"):
            raise RuntimeError("the AT-SPI text-change signal could not be registered")

    def on_event(self, event):
        # Cheap work only: remember the source and reset a debounce timer. The
        # text is read once the burst ends, not on every character.
        try:
            source = event.source
            if source is None:
                return
            state = source.get_state_set()
            if state is None or not state.contains(Atspi.StateType.FOCUSED):
                return
            if source.get_role_name() in ("password text", "terminal"):
                return
            self.pending = source
            if self.timer is not None:
                GLib.source_remove(self.timer)
            self.timer = GLib.timeout_add(DEBOUNCE_MS, self.report)
        except Exception:
            return

    def report(self):
        self.timer = None
        source = self.pending
        self.pending = None
        summary = summary_from(source)
        if summary is not None:
            print(json.dumps({"kind": "typing", "data": summary}, ensure_ascii=False), flush=True)
        return False


def main():
    observer = Observer()
    try:
        observer.start()
    except Exception as error:  # noqa: BLE001
        print(json.dumps({"kind": "unavailable", "data": {"why": f"AT-SPI text events are not available: {error}"}}), flush=True)
        return 1
    loop = GLib.MainLoop()
    loop.run()
    return 0


if __name__ == "__main__":
    sys.exit(main())
