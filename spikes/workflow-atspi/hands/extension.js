// Spike only. A stand-in for the person at the keyboard on a headless desktop: the probe
// writes a command into $SPIKE_HANDS/cmd, this presses keys or clicks through virtual
// input devices (as real input, not through the accessibility bus) and answers in
// $SPIKE_HANDS/done. It also notes whether the shell itself sees button presses that go
// to application windows.
import GLib from 'gi://GLib';
import Clutter from 'gi://Clutter';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const DIR = GLib.getenv('SPIKE_HANDS');
const sleep = ms => new Promise(r => GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => { r(); return GLib.SOURCE_REMOVE; }));
const now = () => GLib.get_monotonic_time();
const read = path => { try { return new TextDecoder().decode(GLib.file_get_contents(path)[1]); } catch { return null; } };
const write = (path, text) => { GLib.file_set_contents(`${path}.tmp`, text); GLib.rename(`${path}.tmp`, path); };
const rect = r => [r.x, r.y, r.width, r.height];

export default class Hands extends Extension {
    enable() {
        const seat = global.stage.context.get_backend().get_default_seat();
        this._keyboard = seat.create_virtual_device(Clutter.InputDeviceType.KEYBOARD_DEVICE);
        this._pointer = seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        this._seen = {press: 0, key: 0};
        this._captured = global.stage.connect('captured-event', (actor, event) => {
            const type = event.type();
            if (type === Clutter.EventType.BUTTON_PRESS) this._seen.press++;
            if (type === Clutter.EventType.KEY_PRESS) this._seen.key++;
            return Clutter.EVENT_PROPAGATE;
        });
        this._last = 0; this._busy = false;
        this._poll = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => { this._check(); return GLib.SOURCE_CONTINUE; });
        write(`${DIR}/ready`, 'yes');
    }

    disable() { GLib.source_remove(this._poll); global.stage.disconnect(this._captured); }

    _check() {
        if (this._busy) return;
        const text = read(`${DIR}/cmd`);
        if (!text) return;
        const cmd = JSON.parse(text);
        if (cmd.seq <= this._last) return;
        this._last = cmd.seq; this._busy = true;
        this._do(cmd.do).then(result => write(`${DIR}/done`, JSON.stringify({seq: cmd.seq, result})),
            e => write(`${DIR}/done`, JSON.stringify({seq: cmd.seq, error: `${e}`})))
            .finally(() => { this._busy = false; });
    }

    async _key(keyval, state) { this._keyboard.notify_keyval(now(), keyval, state); await sleep(25); }

    async _do([verb, ...args]) {
        if (verb === 'windows')
            return global.get_window_actors().map(a => a.meta_window).map(w => ({
                title: w.get_title(), wm_class: w.get_wm_class(), pid: w.get_pid(),
                frame: rect(w.get_frame_rect()), buffer: rect(w.get_buffer_rect()), focus: w.has_focus()}));
        if (verb === 'activate') {
            const w = global.get_window_actors().map(a => a.meta_window).find(w => w.get_pid() === args[0]);
            if (!w) throw new Error(`no window for ${args[0]}`);
            w.activate(global.get_current_time()); await sleep(300); return true;
        }
        if (verb === 'combo') {   // "Control_L+s": press in order, release in reverse
            const keys = args[0].split('+').map(name => Clutter[`KEY_${name}`]);
            for (const k of keys) await this._key(k, Clutter.KeyState.PRESSED);
            for (const k of keys.reverse()) await this._key(k, Clutter.KeyState.RELEASED);
            return true;
        }
        if (verb === 'type') {
            for (const ch of args[0]) {
                const k = Clutter.unicode_to_keysym(ch.codePointAt(0));
                await this._key(k, Clutter.KeyState.PRESSED); await this._key(k, Clutter.KeyState.RELEASED);
            }
            return true;
        }
        if (verb === 'click') {
            this._pointer.notify_absolute_motion(now(), args[0], args[1]); await sleep(80);
            this._pointer.notify_button(now(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.PRESSED); await sleep(60);
            this._pointer.notify_button(now(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.RELEASED); await sleep(80);
            return true;
        }
        if (verb === 'seen') return this._seen;
        if (verb === 'quit') { global.context.terminate(); return true; }
        throw new Error(`unknown ${verb}`);
    }
}
