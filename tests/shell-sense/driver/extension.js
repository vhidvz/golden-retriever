// Test only. Opens two windows on the headless desktop and then does the things
// Buddy's desktop sense is supposed to notice, one after another.
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Clutter from 'gi://Clutter';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const sleep = ms => new Promise(r => GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => { r(); return GLib.SOURCE_REMOVE; }));
const DIR = GLib.getenv('BUDDY_TEST_DIR');
const findWindow = title => global.get_window_actors().map(a => a.meta_window).find(w => w.get_title() === title);

export default class Driver extends Extension {
    enable() { this._run().catch(e => { GLib.file_set_contents(`${DIR}/out/driver-error.txt`, `${e}\n${e.stack}`); global.context.terminate(); }); }
    disable() {}

    async _open(title) {
        const launcher = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.NONE});
        launcher.setenv('GDK_BACKEND', 'wayland', true);
        launcher.spawnv(['python3', `${DIR}/window.py`, title]);
        for (let waited = 0; waited < 8000; waited += 100) {
            if (findWindow(title)) return findWindow(title);
            await sleep(100);
        }
        throw new Error(`window ${title} never appeared`);
    }

    async _run() {
        await sleep(4000);   // let Buddy's extension connect and receive its policy
        if (!Main.panel.statusArea['buddy-tray'])
            throw new Error("Buddy's top-bar tray button did not load");
        const alpha = await this._open('alpha');
        const beta = await this._open('beta');
        for (const window of [alpha, beta, alpha, beta, alpha]) {
            window.activate(global.get_current_time());
            await sleep(2600);   // long enough to count as working in it
        }
        alpha.make_fullscreen(); await sleep(1500);
        alpha.unmake_fullscreen(); await sleep(1200);
        Main.overview.show(); await sleep(1500);
        Main.overview.hide(); await sleep(1200);
        Main.notify('Build finished', 'All tests passed'); await sleep(1000);
        const pointer = global.stage.context.get_backend().get_default_seat().create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        for (const [x, y] of [[200, 200], [600, 400], [900, 700]]) {
            pointer.notify_absolute_motion(GLib.get_monotonic_time(), x, y); await sleep(450);
        }
        pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.PRESSED);
        await sleep(120);
        pointer.notify_button(GLib.get_monotonic_time(), 1, Clutter.ButtonState.RELEASED);
        await sleep(800);
        global.context.terminate();
    }
}
