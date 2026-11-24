// Buddy's KDE desktop sense. KWin scripts cannot read window contents; this reports only
// the active application and pointer position to the daemon's session-bus bridge.
function appOf(window) {
    if (!window)
        return "";
    return window.resourceClass || window.resourceName || "unknown";
}

function focus(window) {
    const app = appOf(window);
    if (app)
        callDBus("local.buddy.Sense", "/Sense", "local.buddy.Sense", "Focus", app, window.caption || "");
}

let last = null;
function pointer() {
    const p = workspace.cursorPos;
    if (!p)
        return;
    if (last && Math.abs(p.x - last.x) + Math.abs(p.y - last.y) < 4)
        return;
    last = {x: p.x, y: p.y};
    callDBus("local.buddy.Sense", "/Sense", "local.buddy.Sense", "Pointer", p.x, p.y);
}

focus(workspace.activeWindow);
workspace.windowActivated.connect(focus);
workspace.cursorPosChanged.connect(pointer);

// KWin exposes no overview signal, so poll the effect state at a low frequency.
// QTimer is the supported timer in KWin scripts; setTimeout is not available.
let overview = workspace.isEffectActive("overview");
callDBus("local.buddy.Sense", "/Sense", "local.buddy.Sense", "Overview", overview);
const overviewTimer = new QTimer();
overviewTimer.interval = 250;
overviewTimer.timeout.connect(function () {
    const now = workspace.isEffectActive("overview");
    if (now !== overview) {
        overview = now;
        callDBus("local.buddy.Sense", "/Sense", "local.buddy.Sense", "Overview", now);
    }
});
overviewTimer.start();
