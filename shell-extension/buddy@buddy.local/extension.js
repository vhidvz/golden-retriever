// Buddy's "desktop" sense. GNOME on Wayland tells ordinary programs nothing about
// other windows or the pointer, so this small extension observes them from inside
// the shell and passes on only what Buddy is allowed to know.
//
// It speaks the same line-per-JSON-object protocol as every other client of the
// daemon, over the daemon's Unix socket, and obeys the policy the daemon sends:
// when the desktop sense is off or privacy mode is on, it reports nothing at all.

import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const RECONNECT_SECONDS = 5;
const POINTER_MILLISECONDS = 200;
const POINTER_MIN_MOVE = 4;

export default class BuddySense extends Extension {
    enable() {
        this._policy = {desktop: false, window_titles: false};
        this._last = {};
        this._signals = [];
        this._sources = new Map();
        this._connect();

        this._watch(global.display, 'notify::focus-window', () => this._reportFocus());
        // Click context is content-free: button, current app and a coarse region.
        this._signals.push([global.stage, global.stage.connect('captured-event', (_actor, event) => this._onCapturedEvent(event))]);
        this._watch(global.display, 'in-fullscreen-changed', () => this._reportFullscreen());
        this._watch(Main.overview, 'showing', () => this._report('overview', {on: true}));
        this._watch(Main.overview, 'hidden', () => this._report('overview', {on: false}));
        this._watch(Main.messageTray, 'source-added', (_tray, source) => this._watchSource(source));
        for (const source of Main.messageTray.getSources())
            this._watchSource(source);

        this._pointerTimer = GLib.timeout_add(GLib.PRIORITY_LOW, POINTER_MILLISECONDS, () => {
            this._reportPointer();
            return GLib.SOURCE_CONTINUE;
        });

        // Guided capture: one keypress saves the focused accessible control. The daemon
        // does the reading and deciding; this only sends the request.
        try {
            this._settings = this.getSettings();
            Main.wm.addKeybinding(
                'capture-step',
                this._settings,
                Meta.KeyBindingFlags.NONE,
                Shell.ActionMode.NORMAL,
                () => this._captureStep(),
            );
        } catch (error) {
            logError(error, "Buddy's capture shortcut is unavailable");
        }
    }

    disable() {
        // Also runs when the screen locks: the daemon learns of the lock itself, from the session bus.
        for (const [object, id] of this._signals)
            object.disconnect(id);
        for (const [source, id] of this._sources)
            source.disconnect(id);
        this._signals = [];
        this._sources.clear();
        if (this._pointerTimer)
            GLib.source_remove(this._pointerTimer);
        if (this._retryTimer)
            GLib.source_remove(this._retryTimer);
        this._pointerTimer = this._retryTimer = null;
        this._cancel?.cancel();
        this._connection?.close(null);
        this._connection = this._output = this._cancel = null;
        if (this._settings) {
            try {
                Main.wm.removeKeybinding('capture-step');
            } catch {
                // the shell is already tearing down this extension
            }
            this._settings = null;
        }
    }

    _watch(object, signal, handler) {
        this._signals.push([object, object.connect(signal, handler)]);
    }

    _watchSource(source) {
        if (!this._sources.has(source))
            this._sources.set(source, source.connect('notification-added', () => this._report('notification', {app: source.title ?? ''}, true)));
    }

    // --- the connection to the daemon

    _socketPath() {
        return GLib.getenv('BUDDY_SOCKET') ?? GLib.build_filenamev([GLib.get_user_runtime_dir(), 'buddy', 'buddy.sock']);
    }

    _connect() {
        this._retryTimer = null;
        this._cancel = new Gio.Cancellable();
        const client = new Gio.SocketClient();
        const address = Gio.UnixSocketAddress.new(this._socketPath());
        client.connect_async(address, this._cancel, (_client, result) => {
            try {
                this._connection = client.connect_finish(result);
            } catch {
                this._retryLater();   // Buddy is not running; keep trying quietly
                return;
            }
            this._output = this._connection.get_output_stream();
            this._send({type: 'hello', role: 'sense'});
            this._readLines(new Gio.DataInputStream({base_stream: this._connection.get_input_stream()}));
        });
    }

    _retryLater() {
        this._connection = this._output = null;
        this._policy = {desktop: false, window_titles: false};
        if (this._cancel && !this._cancel.is_cancelled() && !this._retryTimer) {
            this._retryTimer = GLib.timeout_add_seconds(GLib.PRIORITY_LOW, RECONNECT_SECONDS, () => {
                this._connect();
                return GLib.SOURCE_REMOVE;
            });
        }
    }

    _readLines(input) {
        input.read_line_async(GLib.PRIORITY_DEFAULT, this._cancel, (_stream, result) => {
            let line;
            try {
                [line] = input.read_line_finish_utf8(result);
            } catch {
                line = null;
            }
            if (line === null) {
                this._retryLater();
                return;
            }
            try {
                const message = JSON.parse(line);
                if (message.type === 'policy')
                    this._setPolicy(message);
                else if (message.type === 'reply' && message.ok === false)
                    Main.notify('Buddy', message.error ?? 'The capture step could not be saved.');
            } catch {
                // not for us
            }
            this._readLines(input);
        });
    }

    _setPolicy(policy) {
        const wasOpen = this._policy.desktop;
        this._policy = {desktop: !!policy.desktop, window_titles: !!policy.window_titles};
        if (this._policy.desktop && !wasOpen) {
            // Newly allowed: say how things stand right now.
            this._last = {};
            this._reportFocus();
            this._reportFullscreen();
            this._report('overview', {on: Main.overview.visible});
        }
    }

    _send(message) {
        try {
            this._output?.write_all(`${JSON.stringify(message)}\n`, null);
        } catch {
            this._retryLater();
        }
    }

    /// Reports one observation, unless not allowed to or nothing has changed.
    _captureStep() {
        this._send({type: 'command', name: 'workflow_capture_step', args: {}});
    }

    _report(kind, data, always = false) {
        if (!this._policy.desktop || !this._output)
            return;
        const text = JSON.stringify(data);
        if (!always && this._last[kind] === text)
            return;
        this._last[kind] = text;
        this._send({type: 'sense', source: 'shell', kind, data});
    }

    // --- what is observed

    _currentApp() {
        const window = global.display.focus_window;
        if (!window)
            return null;
        const app = Shell.WindowTracker.get_default().get_window_app(window);
        // Applications without a launcher get a throwaway id ("window:7"); their window class is steadier.
        const id = app?.get_id();
        return (id && !id.startsWith('window:') ? id : window.get_wm_class() ?? 'unknown').replace(/\.desktop$/, '');
    }

    _onCapturedEvent(event) {
        if (!this._policy.desktop || !this._output || event.type() !== Clutter.EventType.BUTTON_PRESS)
            return Clutter.EVENT_PROPAGATE;
        try {
            const [x, y] = event.get_coords();
            const button = event.get_button();
            const region = `${Math.floor(x / 240)}:${Math.floor(y / 180)}`;
            const data = {button, region};
            const app = this._currentApp();
            if (app)
                data.app = app;
            this._report('click', data, true);
        } catch {
            // An event we cannot describe is simply not reported.
        }
        return Clutter.EVENT_PROPAGATE;
    }

    _reportFocus() {
        const app = this._currentApp();
        if (!app)
            return;
        const data = {app};
        if (this._policy.window_titles) {
            const window = global.display.focus_window;
            data.title = window?.get_title() ?? '';
        }
        this._report('focus', data);
    }

    _reportFullscreen() {
        this._report('fullscreen', {on: global.display.get_monitor_in_fullscreen(global.display.get_primary_monitor())});
    }

    _reportPointer() {
        if (!this._policy.desktop || !this._output)
            return;
        const [x, y] = global.get_pointer();
        const [lastX, lastY] = this._pointer ?? [-1000, -1000];
        if (Math.abs(x - lastX) + Math.abs(y - lastY) < POINTER_MIN_MOVE)
            return;
        this._pointer = [x, y];
        this._send({type: 'sense', source: 'shell', kind: 'pointer', data: {x, y}});
    }
}
