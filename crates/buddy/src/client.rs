use anyhow::{Context, Result, anyhow, bail};
use buddy_core::{ClientMsg, ServerMsg, paths};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// Sends one command to the running daemon and returns its reply data.
pub fn request(name: &str, args: Value) -> Result<Value> {
    let path = paths::socket_path();
    let mut stream = UnixStream::connect(&path)
        .with_context(|| format!("Buddy is not running (no daemon at {})", path.display()))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    for msg in [
        ClientMsg::Hello { role: "cli".into() },
        ClientMsg::Command {
            name: name.into(),
            args,
        },
    ] {
        writeln!(stream, "{}", serde_json::to_string(&msg)?)?;
    }
    for line in BufReader::new(stream).lines() {
        if let ServerMsg::Reply { ok, data, error } = serde_json::from_str(&line?)? {
            return if ok {
                Ok(data)
            } else {
                Err(anyhow!(error.unwrap_or_else(|| "command failed".into())))
            };
        }
    }
    bail!("the daemon closed the connection without answering")
}

pub fn run(name: &str, args: Value) -> Result<()> {
    let data = request(name, args)?;
    match name {
        "journal" => {
            for e in data.as_array().into_iter().flatten().rev() {
                println!(
                    "{}  {:<16} {}",
                    e["ts"],
                    e["kind"].as_str().unwrap_or("?"),
                    e["data"]
                );
            }
        }
        _ if data.is_null() => println!("ok"),
        _ => println!("{}", serde_json::to_string_pretty(&data)?),
    }
    Ok(())
}

pub fn status(as_json: bool) -> Result<()> {
    let s = request("status", Value::Null)?;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&s)?);
        return Ok(());
    }
    let st = &s["state"];
    let text = |v: &Value| v.as_str().unwrap_or("?").replace('_', " ");
    println!(
        "{} — {}, {:.1} days old ({:.0}% grown)",
        text(&st["name"]),
        text(&st["stage"]),
        st["age_days"].as_f64().unwrap_or(0.0),
        100.0 * st["growth"].as_f64().unwrap_or(0.0)
    );
    println!(
        "  doing:       {}{}",
        text(&st["activity"]),
        if st["hidden"] == true {
            " (hidden)"
        } else {
            ""
        }
    );
    println!(
        "  experience:  {:.1} h awake over {} sessions, petted {} times",
        s["life"]["awake_secs"].as_f64().unwrap_or(0.0) / 3600.0,
        s["life"]["sessions"],
        s["life"]["pets"]
    );
    println!("  renderer:    {}", text(&s["renderer"]));
    println!(
        "  privacy:     {}",
        if st["privacy"] == true {
            "ON — all observation suspended"
        } else {
            "off"
        }
    );
    println!("  observing:   {}", text(&s["observing"]));
    println!(
        "  memory:      {} (schema v{})",
        text(&s["database"]),
        s["schema_version"]
    );
    Ok(())
}
