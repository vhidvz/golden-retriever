//! KDE Plasma sense bridge.
//!
//! KWin 6 has no readable "active window" D-Bus method. Its scripting API does expose
//! `workspace.activeWindow` and `workspace.cursorPos`, so a small KWin script calls this
//! session-bus service. The service feeds the same `Sense` messages as the GNOME Shell
//! extension. The bridge itself is testable without Plasma; the KWin script is installed
//! only when KDE tools are present.

use crate::daemon::Incoming;
use buddy_core::ClientMsg;
use serde_json::{Value, json};
use std::sync::mpsc::Sender;
use std::thread;
use zbus::interface;

pub(crate) struct Bridge {
    tx: Sender<Incoming>,
}

impl Bridge {
    fn report(&self, kind: &str, data: Value) {
        let _ = self.tx.send(Incoming::Message(0, ClientMsg::Sense { source: "kde".into(), kind: kind.into(), data }));
    }
}

#[interface(name = "local.buddy.Sense")]
impl Bridge {
    /// The application that came to the front. KWin scripting has no trustworthy window
    /// title permission of its own, so titles stay disabled on this path.
    fn focus(&self, app: String) {
        if !app.trim().is_empty() {
            self.report("focus", json!({ "app": app }));
        }
    }

    /// Global pointer coordinates from KWin. The daemon already treats pointer data as
    /// content-free and only uses it for his gaze.
    fn pointer(&self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            self.report("pointer", json!({ "x": x, "y": y }));
        }
    }

    /// Optional click hook for a future native KWin effect. Normal KWin scripts cannot
    /// see button events, so the shipped script does not call this yet.
    fn click(&self, button: u32, region: String) {
        self.report("click", json!({ "button": button, "region": region }));
    }

    /// KWin has no overview signal, but its scripting API can poll the effect state.
    fn overview(&self, on: bool) {
        self.report("overview", json!({ "on": on }));
    }
}

/// Starts the session-bus service in its own thread. A failure is not fatal: Buddy lives
/// without KDE, just as he lives without GNOME.
pub(crate) fn serve(tx: Sender<Incoming>) {
    thread::spawn(move || {
        let bridge = Bridge { tx };
        let built = zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.name("local.buddy.Sense"))
            .and_then(|builder| builder.serve_at("/Sense", bridge))
            .and_then(|builder| builder.build());
        match built {
            Ok(_connection) => {
                // Keep the connection and its object server alive for the daemon's lifetime.
                loop {
                    thread::park();
                }
            }
            Err(error) => eprintln!("buddy: KDE sense bridge is unavailable: {error:#}"),
        }
    });
}
