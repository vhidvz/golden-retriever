// Throwaway harness: drives overlay windows inside an isolated headless shell and records what Mutter does.
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Meta from 'gi://Meta';
import Clutter from 'gi://Clutter';
import Shell from 'gi://Shell';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const sleep = ms => new Promise(r => GLib.timeout_add(GLib.PRIORITY_DEFAULT, ms, () => { r(); return GLib.SOURCE_REMOVE; }));
const DIR = GLib.getenv('BUDDY_SPIKE_DIR');   // where the test clients live
const OUT = GLib.getenv('BUDDY_SPIKE_OUT');   // where results, logs and screenshots go
const rectOf = w => { const r = w.get_frame_rect(); return [r.x, r.y, r.width, r.height]; };
const findWin = title => global.get_window_actors().map(a => a.meta_window).find(w => w.get_title() === title);
async function waitWin(title, ms = 8000) {
    for (let t = 0; t < ms; t += 100) { const w = findWin(title); if (w) return w; await sleep(100); }
    return null;
}
function spawn(argv, env) {
    const l = new Gio.SubprocessLauncher({flags: Gio.SubprocessFlags.NONE});
    for (const [k, v] of Object.entries(env)) l.setenv(k, v, true);
    return l.spawnv(argv);
}
function stackTop(a, b) {  // which of two windows is drawn on top
    const order = global.get_window_actors().map(x => x.meta_window);
    return order.indexOf(a) > order.indexOf(b) ? a.get_title() : b.get_title();
}

export default class BuddySpike extends Extension {
    enable() { this._run().catch(e => { GLib.file_set_contents(`${OUT}/harness-error.txt`, `${e}\n${e.stack}`); global.context.terminate(); }); }
    disable() {}

    async _click(vp, x, y) {
        vp.notify_absolute_motion(GLib.get_monotonic_time(), x, y); await sleep(120);
        vp.notify_button(GLib.get_monotonic_time(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.PRESSED); await sleep(40);
        vp.notify_button(GLib.get_monotonic_time(), Clutter.BUTTON_PRIMARY, Clutter.ButtonState.RELEASED); await sleep(350);
    }
    async _shot(path) {
        try {
            const shooter = new Shell.Screenshot();
            const stream = Gio.File.new_for_path(path).replace(null, false, Gio.FileCreateFlags.NONE, null);
            await new Promise((res, rej) => shooter.screenshot(false, stream, (o, r) => { try { o.screenshot_finish(r); res(); } catch (e) { rej(e); } }));
            stream.close(null); return true;
        } catch (e) { return `failed: ${e}`; }
    }

    async _run() {
        await sleep(2500);
        const out = {mutter: `${Meta.MAJOR_VERSION ?? ''}`, env: {DISPLAY: GLib.getenv('DISPLAY'), WAYLAND_DISPLAY: GLib.getenv('WAYLAND_DISPLAY')}, variants: {}};
        const seat = global.stage.context.get_backend().get_default_seat();
        const vp = seat.create_virtual_device(Clutter.InputDeviceType.POINTER_DEVICE);
        const tlog = `${OUT}/log-target.txt`;
        spawn(['python3', `${DIR}/target.py`, '--title', 'buddy-target', '--log', tlog], {GDK_BACKEND: 'wayland'});
        const target = await waitWin('buddy-target');
        if (!target) throw new Error('target window never appeared');
        // A freshly mapped window can still be settling; insist on the position before testing against it.
        for (let waited = 0; waited < 5000 && rectOf(target).join() !== '100,100,1200,800'; waited += 200) {
            target.move_resize_frame(true, 100, 100, 1200, 800); await sleep(200);
        }
        out.target_rect = rectOf(target);

        if (GLib.getenv('BUDDY_MODE') === 'fur') {
            // GL fur client through the nested XWayland: does an ARGB GL window composite correctly?
            for (const [tag, extra] of [['argb', []], ['opaque', ['--opaque', '1']], ['argb-or', ['--or', '1']]]) {
                const args = [`${DIR}/furtest`, '--sweep', 'one', '--secs', '6', '--or', '0', '--type', 'utility', '--x', '500', '--y', '150', '--cull', '1', '--title', `buddy-fur-${tag}`, ...extra];
                spawn(args, {});
                const fw = await waitWin(`buddy-fur-${tag}`);
                await sleep(3000);
                out[`fur_${tag}`] = fw ? {rect: rectOf(fw), shot: await this._shot(`${OUT}/shot-fur-${tag}.png`)} : 'window never appeared';
                await sleep(3600);
            }
            GLib.file_set_contents(`${OUT}/harness-result-fur.json`, JSON.stringify(out, null, 1));
            global.context.terminate(); return;
        }
        const variants = [
            {name: 'wayland-managed-by-extension', env: {GDK_BACKEND: 'wayland'}, args: [], manage: true},
            {name: 'x11-self-managed-above', env: {GDK_BACKEND: 'x11'}, args: ['--above'], manage: false},
            {name: 'x11-override-redirect', env: {GDK_BACKEND: 'x11'}, args: ['--popup'], manage: false},
        ];
        for (const v of variants) {
            const r = {}; out.variants[v.name] = r;
            const title = `buddy-overlay-${v.name}`;
            const proc = spawn(['python3', `${DIR}/overlay.py`, '--title', title, '--log', `${OUT}/log-${v.name}.txt`, ...v.args], v.env);
            const ov = await waitWin(title);
            if (!ov) { r.error = 'overlay window never appeared'; proc.force_exit(); continue; }
            await sleep(500);
            r.client = ov.get_client_type() === Meta.WindowClientType.X11 ? 'x11' : 'wayland';
            r.override_redirect = ov.is_override_redirect();
            r.initial_rect = rectOf(ov);
            if (v.manage) {
                ov.make_above(); ov.stick(); ov.move_frame(true, 300, 200);
                try { ov.hide_from_window_list(); r.hide_from_window_list = 'called'; } catch (e) { r.hide_from_window_list = `threw: ${e.message}`; }
                await sleep(500);
            }
            r.rect = rectOf(ov); r.above = ov.is_above(); r.sticky = ov.is_on_all_workspaces(); r.skip_taskbar = ov.is_skip_taskbar();
            r.window_type = ov.get_window_type();
            target.activate(global.get_current_time()); await sleep(500);
            r.focus_before = global.display.get_focus_window()?.get_title() ?? null;
            r.on_top_normal = stackTop(ov, target);
            const [ox, oy] = r.rect;
            r.click_inside_region = [ox + 200, oy + 150]; await this._click(vp, ox + 200, oy + 150);
            r.focus_after_click_on_dog = global.display.get_focus_window()?.get_title() ?? null;
            r.click_outside_region = [ox + 30, oy + 250]; await this._click(vp, ox + 30, oy + 250);
            r.click_outside_window = [1100, 800]; await this._click(vp, 1100, 800);
            // can the overlay be driven across the screen at 50 Hz?
            if (!r.override_redirect) {
                let exact = 0; const N = 60;
                for (let i = 0; i < N; i++) { ov.move_frame(true, 300 + i * 8, 200); await sleep(20); if (rectOf(ov)[0] === 300 + i * 8) exact++; }
                r.move_frame_exact = `${exact}/${N}`; ov.move_frame(true, 300, 200); await sleep(300);
            }
            r.screenshot = await this._shot(`${OUT}/shot-${v.name}.png`);
            // fullscreen application underneath
            target.make_fullscreen(); await sleep(700); target.activate(global.get_current_time()); await sleep(400);
            r.on_top_when_target_fullscreen = stackTop(ov, target);
            r.monitor_in_fullscreen = global.display.get_monitor_in_fullscreen(0);
            target.unmake_fullscreen(); await sleep(700);
            target.move_resize_frame(true, 100, 100, 1200, 800); await sleep(300);
            proc.force_exit(); await sleep(600);
        }
        GLib.file_set_contents(`${OUT}/harness-result.json`, JSON.stringify(out, null, 1));
        global.context.terminate();
    }
}
