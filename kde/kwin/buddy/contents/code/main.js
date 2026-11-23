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
        callDBus("local.buddy.Sense", "/Sense", "local.buddy.Sense", "Focus", app);
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
