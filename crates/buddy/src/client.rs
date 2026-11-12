use anyhow::{Context, Result, anyhow, bail};
use buddy_core::{ClientMsg, ServerMsg, paths};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

/// Sends one command to the running daemon and returns its reply data.
pub fn request(name: &str, args: Value) -> Result<Value> {
    request_within(name, args, Duration::from_secs(5))
}

/// As `request`, for commands that take a while to answer.
pub fn request_within(name: &str, args: Value, patience: Duration) -> Result<Value> {
    let path = paths::socket_path();
    let mut stream = UnixStream::connect(&path).with_context(|| format!("Buddy is not running (no daemon at {})", path.display()))?;
    stream.set_read_timeout(Some(patience))?;
    for msg in [ClientMsg::Hello { role: "cli".into() }, ClientMsg::Command { name: name.into(), args }] {
        writeln!(stream, "{}", serde_json::to_string(&msg)?)?;
    }
    for line in BufReader::new(stream).lines() {
        if let ServerMsg::Reply { ok, data, error } = serde_json::from_str(&line?)? {
            return if ok { Ok(data) } else { Err(anyhow!(error.unwrap_or_else(|| "command failed".into()))) };
        }
    }
    bail!("the daemon closed the connection without answering")
}

pub fn run(name: &str, args: Value) -> Result<()> {
    let data = request(name, args)?;
    match name {
        "journal" => {
            for e in data.as_array().into_iter().flatten().rev() {
                println!("{}  {:<16} {}", e["ts"], e["kind"].as_str().unwrap_or("?"), e["data"]);
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
    println!("  doing:       {}{}", text(&st["activity"]), if st["hidden"] == true { " (hidden)" } else { "" });
    println!(
        "  experience:  {:.1} h awake over {} sessions, petted {} times",
        s["life"]["awake_secs"].as_f64().unwrap_or(0.0) / 3600.0,
        s["life"]["sessions"],
        s["life"]["pets"]
    );
    println!("  renderer:    {}", text(&s["renderer"]));
    println!("  privacy:     {}", if st["privacy"] == true { "ON — all observation suspended" } else { "off" });
    println!("  observing:   {}", text(&s["observing"]));
    println!("  memory:      {} (schema v{})", text(&s["database"]), s["schema_version"]);
    Ok(())
}

pub fn why() -> Result<()> {
    let w = request("why", Value::Null)?;
    let text = |v: &Value| v.as_str().unwrap_or("?").replace('_', " ");
    println!("He is doing: {} (for {:.0} s) — because: {}", text(&w["activity"]), w["for_secs"].as_f64().unwrap_or(0.0), text(&w["because"]));
    let others: Vec<String> = w["also_considered"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| format!("{} ({}, {:.2})", text(&o["activity"]), text(&o["reason"]), o["appeal"].as_f64().unwrap_or(0.0)))
        .collect();
    if !others.is_empty() {
        println!("He also considered: {}", others.join("; "));
    }
    for (title, key) in [("Feeling", "emotions"), ("Needs (0 satisfied, 1 urgent)", "needs"), ("Temperament", "personality")] {
        let mut items: Vec<(String, f64)> =
            w[key].as_object().into_iter().flatten().map(|(k, v)| (k.replace('_', " "), v.as_f64().unwrap_or(0.0))).collect();
        items.sort_by(|a, b| b.1.total_cmp(&a.1));
        println!("{title}: {}", items.iter().map(|(k, v)| format!("{k} {v:.2}")).collect::<Vec<_>>().join(", "));
    }
    let busy = match w["you"]["busy"].as_f64().unwrap_or(0.0) {
        b if b > 0.7 => "busy",
        b if b > 0.3 => "working on and off",
        _ => "not doing much at the keyboard",
    };
    println!("You: {}, {busy}", if w["you"]["away"] == true { "away" } else { "here" });
    if let Some(saw) = w["last_look_at_the_screen"].as_str() {
        println!("Last look at the screen: {saw}");
    }
    for habit in w["habits_noticed"].as_array().into_iter().flatten() {
        let apps: Vec<&str> = habit["apps"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        println!("A habit of yours he has noticed: {} ({} times on {} days)", apps.join(", then "), habit["times"], habit["days"]);
    }
    match w["favourite_spot"].as_f64() {
        Some(spot) => println!("Favourite spot: {:.0}% of the way across the screen", spot * 100.0),
        None => println!("Favourite spot: none yet"),
    }
    Ok(())
}

/// Runs offline; needs no daemon and touches no saved state.
pub fn simulate(args: &[String]) -> Result<()> {
    use buddy_core::simulate::{Scenario, User, run};
    let option = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1));
    let user = match option("--user").map(String::as_str) {
        None | Some("attentive") => User::Attentive,
        Some("absent") => User::Absent,
        Some("office") => User::OfficeHours,
        Some(other) => bail!("unknown user '{other}' (attentive, absent, office)"),
    };
    let scenario = Scenario {
        days: args.first().and_then(|d| d.parse().ok()).unwrap_or(30.0),
        seed: option("--seed").and_then(|s| s.parse().ok()).unwrap_or(1),
        user,
        start_age_days: option("--age").and_then(|s| s.parse().ok()).unwrap_or(400.0),
        adult_days: 365.0,
    };
    print!("{}", run(&scenario));
    Ok(())
}

pub fn senses() -> Result<()> {
    let s = request("senses", Value::Null)?;
    if s["privacy_mode"] == true {
        println!("Privacy mode is ON: nothing is being observed, whatever is switched on below.");
    }
    for sense in s["senses"].as_array().into_iter().flatten() {
        let allowed = if sense["allowed"] == true { "on " } else { "off" };
        println!("  {:<14} {}  {}", sense["source"].as_str().unwrap_or("?"), allowed, sense["what"].as_str().unwrap_or(""));
        if let Some(reporting) = sense["reporting"].as_str() {
            println!("  {:<14}      {}", "", reporting);
        }
    }
    Ok(())
}

pub fn look(file: Option<&str>) -> Result<()> {
    let args = match file {
        Some(path) => serde_json::json!({ "file": std::fs::canonicalize(path).with_context(|| format!("no such image: {path}"))? }),
        None => {
            eprintln!("Looking… (the first time, GNOME asks which screen to share)");
            Value::Null
        }
    };
    let seen = request_within("look", args, Duration::from_secs(240))?;
    println!("He sees: {}", seen["summary"].as_str().unwrap_or("?"));
    println!(
        "  (looked at by {} on this machine in {:.1} s; only this summary is kept, in memory)",
        seen["model"].as_str().unwrap_or("?"),
        seen["secs"].as_f64().unwrap_or(0.0)
    );
    Ok(())
}
