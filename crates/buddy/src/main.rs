//! `buddy` — the daemon that keeps Buddy alive, and the command-line remote for it.

mod client;
mod daemon;
mod renderer;

use anyhow::Result;
use serde_json::{Value, json};

const USAGE: &str = "\
buddy — a dog who lives on your desktop

Usage:
  buddy daemon [--no-renderer]   run Buddy (normally started by the systemd user service)
  buddy status [--json]          how he is, what is running, what is being observed
  buddy hide [MINUTES]           hide him, for a while or until `buddy show`
  buddy show                     bring him back
  buddy sleep | wake             send him to sleep / wake him
  buddy privacy on|off           suspend / resume every perception source
  buddy set KEY VALUE            name | size_px (adult height) | adult_days (growth timeline)
  buddy journal [N]              the last N things that happened (default 20)
  buddy export                   everything he knows, as JSON
  buddy quit                     stop the daemon
";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| args.get(i).map(String::as_str);
    let command: (&str, Value) = match (arg(0), arg(1)) {
        (Some("daemon"), flag) => return daemon::run(flag == Some("--no-renderer")),
        (Some("status"), flag) => return client::status(flag == Some("--json")),
        (Some("hide"), minutes) => (
            "hide",
            json!({ "minutes": minutes.and_then(|m| m.parse::<f64>().ok()) }),
        ),
        (Some("show"), _) => ("show", Value::Null),
        (Some("sleep"), _) => ("sleep", Value::Null),
        (Some("wake"), _) => ("wake", Value::Null),
        (Some("privacy"), Some(v @ ("on" | "off"))) => ("privacy", json!({ "on": v == "on" })),
        (Some("set"), Some(key)) if arg(2).is_some() => {
            ("set", json!({ "key": key, "value": arg(2) }))
        }
        (Some("journal"), n) => (
            "journal",
            json!({ "limit": n.and_then(|n| n.parse::<u64>().ok()).unwrap_or(20) }),
        ),
        (Some("export"), _) => ("export", Value::Null),
        (Some("quit"), _) => ("quit", Value::Null),
        _ => {
            print!("{USAGE}");
            return Ok(());
        }
    };
    client::run(command.0, command.1)
}
