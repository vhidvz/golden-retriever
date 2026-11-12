//! The "system" sense: how long since the user touched the computer, whether
//! the screen is locked, and how hard the machine is working. Nothing here
//! sees any content. Polled every two seconds from a thread of its own.

use serde_json::{Value, json};
use std::time::Duration;
use zbus::blocking::Connection;

const EVERY: Duration = Duration::from_secs(2);

fn idle_secs(bus: &Connection) -> Option<f64> {
    let reply = bus
        .call_method(
            Some("org.gnome.Mutter.IdleMonitor"),
            "/org/gnome/Mutter/IdleMonitor/Core",
            Some("org.gnome.Mutter.IdleMonitor"),
            "GetIdletime",
            &(),
        )
        .ok()?;
    let millis: u64 = reply.body().deserialize().ok()?;
    Some(millis as f64 / 1000.0)
}

fn locked(bus: &Connection) -> Option<bool> {
    let reply = bus.call_method(Some("org.gnome.ScreenSaver"), "/org/gnome/ScreenSaver", Some("org.gnome.ScreenSaver"), "GetActive", &()).ok()?;
    reply.body().deserialize().ok()
}

/// One-minute load average per core, capped at 1.
fn load() -> Option<f64> {
    let text = std::fs::read_to_string("/proc/loadavg").ok()?;
    let one_minute: f64 = text.split_whitespace().next()?.parse().ok()?;
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
    Some((one_minute / cores).clamp(0.0, 1.0))
}

/// Calls `report(kind, data)` with each observation, until it returns false.
pub fn watch(mut report: impl FnMut(&str, Value) -> bool) {
    let bus = Connection::session().ok();
    if bus.is_none() {
        eprintln!("buddy: no session bus; idle time and screen lock will not be noticed");
    }
    let (mut was_locked, mut last_load) = (None, -1.0f64);
    loop {
        if let Some(bus) = &bus {
            if idle_secs(bus).is_some_and(|secs| !report("idle", json!({ "secs": secs }))) {
                return;
            }
            let now_locked = locked(bus);
            if now_locked.is_some() && now_locked != was_locked {
                was_locked = now_locked;
                report("locked", json!({ "on": now_locked }));
            }
        }
        if let Some(level) = load().filter(|level| (level - last_load).abs() >= 0.1) {
            last_load = level;
            report("load", json!({ "level": level }));
        }
        std::thread::sleep(EVERY);
    }
}
