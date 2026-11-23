use anyhow::{Context, Result, anyhow, bail};
use buddy_core::{ClientMsg, ServerMsg, paths};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::Command;
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
        _ => show_outcome(&data)?,
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
    println!("  pictures:    {}", s["pictures"].as_str().unwrap_or("?"));
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

pub fn say(words: &str) -> Result<()> {
    let r = request_within("say", serde_json::json!({ "text": words }), Duration::from_secs(150))?;
    if r.get("understood_as").is_none() {
        return show_outcome(&r);
    }
    let text = |v: &Value| v.as_str().unwrap_or("?").replace('_', " ");
    let meaning = &r["understood_as"];
    let detail = match &meaning["detail"] {
        Value::String(s) => format!(" \u{201c}{s}\u{201d}"),
        Value::Object(o) => format!(" \u{201c}{}\u{201d} = {}", o["cue"].as_str().unwrap_or("?"), o["trick"].as_str().unwrap_or("?")),
        _ => String::new(),
    };
    println!("Understood as: {}{detail} (by {})", text(&meaning["kind"]), text(&r["by"]));
    let kind = meaning["kind"].as_str().unwrap_or_default();
    if r["puzzled"] == true && kind == "cue" {
        println!("He knows that word only a little and did not do it this time. Praise him when he does, and he will learn it.");
    } else if r["puzzled"] == true {
        println!("He looks at you and tilts his head: he does not know what you want.");
    } else if kind == "cue" && r["did_as_asked"] == true {
        println!("He does it: {} — because: {}", text(&r["now"]), text(&r["because"]));
    } else {
        println!(
            "He is {} ({}).",
            match text(&r["now"]).as_str() {
                "stand" => "standing".to_string(),
                "sit" => "sitting".to_string(),
                "lie" => "lying down".to_string(),
                "sleep" => "asleep".to_string(),
                other => format!("busy with: {other}"),
            },
            text(&r["because"])
        );
    }
    Ok(())
}

pub fn knows() -> Result<()> {
    let known = request("knows", Value::Null)?;
    println!("{:<22} {:<10} {:>8}  {:<9} rewarded/corrected", "word", "means", "how well", "how");
    for k in known.as_array().into_iter().flatten() {
        let how = match k["source"].as_str() {
            Some("taught") => "told",
            Some("learned") => "learned",
            _ => "instinct",
        };
        println!(
            "{:<22} {:<10} {:>7.0}%  {:<9} {}/{}",
            k["cue"].as_str().unwrap_or("?"),
            k["trick"].as_str().unwrap_or("?"),
            100.0 * k["strength"].as_f64().unwrap_or(0.0),
            how,
            k["rewarded"],
            k["corrected"]
        );
    }
    Ok(())
}

pub fn memories() -> Result<()> {
    let all = request("memories", Value::Null)?;
    let list = all.as_array().cloned().unwrap_or_default();
    if list.is_empty() {
        println!("He has not been told to remember anything yet. Try: buddy say \"remember that I prefer working late\"");
    }
    for m in list {
        println!(
            "{:>3}. {}   ({}, {})",
            m["id"],
            m["text"].as_str().unwrap_or("?"),
            m["source"].as_str().unwrap_or("?"),
            m["language"].as_str().unwrap_or("?")
        );
    }
    Ok(())
}

/// A path as the daemon will see it, whatever directory the command was run from.
pub fn absolute(path: &str) -> String {
    let expanded = match path.strip_prefix("~/") {
        Some(rest) => std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest),
        None => std::path::PathBuf::from(path),
    };
    std::path::absolute(&expanded).unwrap_or(expanded).to_string_lossy().into_owned()
}

/// What came of something he was asked (or offered) to do.
fn show_outcome(r: &Value) -> Result<()> {
    if let Some(id) = r["waiting_for_yes"].as_u64() {
        println!("He would like to {}.", r["would"].as_str().unwrap_or("?"));
        println!("He is sitting and waiting for your answer:   buddy approve {id}   or   buddy deny {id}");
        println!("(or just say so: buddy say yes / buddy say \"نه ممنون\")");
    } else if let Some(did) = r["did"].as_str() {
        println!("Done: {did}.");
        if let Some(detail) = r["detail"].as_str().filter(|d| !d.is_empty()) {
            println!("{detail}");
        }
        if r["more"] == true {
            println!("… (the rest: buddy result)");
        }
        if r["can_be_undone"] == true {
            println!("(take it back with: buddy undo)");
        }
    } else if let Some(declined) = r["declined"].as_str() {
        println!("All right: he will not {declined}{}.", if r["for_good"] == true { ", and will not ask again" } else { "" });
    } else if let Some(undid) = r["undid"].as_str() {
        println!("Taken back: {undid} ({}).", r["how"].as_str().unwrap_or(""));
    } else if r.get("folders").is_some() {
        let folders: Vec<&str> = r["folders"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        println!(
            "Folders he may use: {}",
            if folders.is_empty() { "none yet (buddy folders add ~/Documents)".to_string() } else { folders.join(", ") }
        );
        for command in r["commands"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            println!("Command approved for good: {command}");
        }
    } else if r.is_null() {
        println!("ok");
    } else {
        println!("{}", serde_json::to_string_pretty(r)?);
    }
    Ok(())
}

/// Commands whose answer may take as long as the work does (a search, a command).
pub fn act(name: &str, args: Value) -> Result<()> {
    show_outcome(&request_within(name, args, Duration::from_secs(16 * 60))?)
}

pub fn result() -> Result<()> {
    println!("{}", request("result", Value::Null)?.as_str().unwrap_or_default());
    Ok(())
}

pub fn listing(what: &str) -> Result<()> {
    let list = request(what, Value::Null)?;
    let rows = list.as_array().cloned().unwrap_or_default();
    if what == "waiting" && rows.is_empty() {
        println!("Nothing is waiting for an answer.");
    }
    for row in rows {
        if what == "waiting" {
            println!(
                "{:>3}. {} ({}, {:.0} s ago)",
                row["id"],
                row["would"].as_str().unwrap_or("?"),
                if row["his_idea"] == true { "his idea" } else { "you asked" },
                row["for_secs"].as_f64().unwrap_or(0.0)
            );
        } else {
            println!(
                "{:<12} {:<6} {:<26} {}",
                row["name"].as_str().unwrap_or("?"),
                row["policy"].as_str().unwrap_or("?"),
                row["risk"].as_str().unwrap_or("?"),
                row["what"].as_str().unwrap_or("")
            );
        }
    }
    Ok(())
}

/// How the managed runtime is doing, without needing the daemon.
pub fn ai_status() -> Result<()> {
    let manifest = crate::ai_runtime::Runtime::path();
    let endpoint = crate::ai::endpoint();
    println!("Buddy's local AI runtime");
    match crate::ai_runtime::Runtime::load_from(&manifest) {
        Ok(runtime) => {
            println!("  runtime:   Ollama {}", if runtime.ollama_version.is_empty() { "(unknown version)" } else { &runtime.ollama_version });
            println!("  endpoint:  {endpoint}");
            println!("  service:   {}", runtime.service);
            println!("  models:    {}", runtime.models_dir.display());
            if let Some(model) = &runtime.model {
                println!("  selected:  {model}");
                if let Ok(capabilities) = crate::ai::capabilities(model) {
                    println!("  supports:  {}", capabilities.join(", "));
                }
            }
        }
        Err(why) if manifest.exists() => {
            println!("  runtime:   manifest is invalid");
            println!("  manifest:  {} — {why:#}", manifest.display());
            println!("  endpoint:  {endpoint} (BUDDY_OLLAMA_URL can override it)");
        }
        Err(_) => {
            println!("  runtime:   not installed");
            println!("  manifest:  {}", manifest.display());
            println!("  endpoint:  {endpoint} (BUDDY_OLLAMA_URL can override it)");
        }
    }
    match crate::ai::version() {
        Ok(version) => println!("  health:    running, Ollama {version}"),
        Err(why) => println!("  health:    not available — {why:#}"),
    }
    match crate::ai::models() {
        Ok(models) if models.is_empty() => println!("  installed models: none"),
        Ok(models) => println!("  installed models: {}", models.join(", ")),
        Err(_) => {}
    }
    Ok(())
}

/// A plain-language check of the runtime's pieces: manifest, health, service, store,
/// the separately installed Ollama, disk and the selected model.
pub fn ai_doctor() -> Result<()> {
    let manifest = crate::ai_runtime::Runtime::path();
    let endpoint = crate::ai::endpoint();
    let runtime = crate::ai_runtime::Runtime::load_from(&manifest);
    println!("Buddy AI doctor");
    match &runtime {
        Ok(runtime) => {
            println!("  manifest:  ok ({})", manifest.display());
            println!("  endpoint:  {endpoint}");
            println!("  service:   {}", runtime.service);
            println!("  models:    {}", runtime.models_dir.display());
            if let Some(digest) = &runtime.model_digest {
                println!("  digest:    {digest}");
            }
            match std::fs::metadata(&runtime.models_dir) {
                Ok(meta) if meta.is_dir() => println!("  store:     the models directory exists"),
                Ok(_) => println!("  store:     {} is not a directory", runtime.models_dir.display()),
                Err(_) => println!("  store:     the models directory is missing ({})", runtime.models_dir.display()),
            }
            let state = Command::new("systemctl")
                .args(["--user", "is-active", &runtime.service])
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .unwrap_or_else(|_| "unknown".into());
            println!("  unit:      {} ({state})", runtime.service);
            if let Some(free) = disk_free(&runtime.models_dir) {
                println!("  disk:      {} free on the models volume", human_bytes(free));
            }
            match crate::ai::models() {
                Ok(models) => {
                    if let Some(model) = &runtime.model {
                        if models.iter().any(|m| m == model) {
                            println!("  model:     {model} is installed");
                        } else {
                            println!("  model:     {model} is selected but not installed (run: buddy ai repair)");
                        }
                    } else {
                        println!("  model:     no model selected (run: buddy ai setup)");
                    }
                }
                Err(why) => println!("  model:     cannot list models — {why:#}"),
            }
        }
        Err(why) if manifest.exists() => {
            println!("  manifest:  invalid ({} — {why:#})", manifest.display());
            println!("  endpoint:  {endpoint}");
            println!("  next:      repair or remove the manifest, then run scripts/install.sh");
        }
        Err(_) => {
            println!("  manifest:  missing ({})", manifest.display());
            println!("  endpoint:  {endpoint}");
            println!("  next:      run scripts/install.sh, or set BUDDY_OLLAMA_URL to use another runtime");
        }
    }
    match crate::ai::version() {
        Ok(version) => println!("  health:    running, Ollama {version} at {endpoint}"),
        Err(why) => println!("  health:    not available — {why:#}"),
    }
    let system = SocketAddr::from(([127, 0, 0, 1], 11434));
    if TcpStream::connect_timeout(&system, Duration::from_millis(300)).is_ok() {
        println!("  system:    an Ollama service is also listening on 127.0.0.1:11434 (left untouched)");
    } else {
        println!("  system:    nothing is listening on 127.0.0.1:11434");
    }
    Ok(())
}

fn disk_free(path: &Path) -> Option<u64> {
    let out = Command::new("df").arg("-Pk").arg(path).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let line = text.lines().nth(1)?;
    line.split_whitespace().nth(3)?.parse::<u64>().ok().map(|kb| kb * 1024)
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// One line for a stored workflow: id, name, steps and the record.
pub fn workflow_line(value: &Value) -> String {
    let apps: Vec<&str> = value["apps"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let steps = value["steps"].as_array().map_or(0, Vec::len);
    let what = if apps.is_empty() && steps > 0 { format!("{steps} inside-app step(s)") } else { apps.join(" > ") };
    format!(
        "{}  {}  {}  ({} runs, {} ok, {} failed)",
        value["id"].as_u64().unwrap_or(0),
        value["name"].as_str().unwrap_or("?"),
        what,
        value["runs"].as_u64().unwrap_or(0),
        value["successes"].as_u64().unwrap_or(0),
        value["failures"].as_u64().unwrap_or(0)
    )
}

fn workflow_key(key: &str) -> Value {
    match key.parse::<u64>() {
        Ok(id) => serde_json::json!({ "id": id }),
        Err(_) => serde_json::json!({ "name": key }),
    }
}

pub fn workflows() -> Result<()> {
    let data = request("workflows", Value::Null)?;
    let workflows = data["workflows"].as_array().cloned().unwrap_or_default();
    if workflows.is_empty() {
        println!("No workflows yet. Record one with: buddy workflow record NAME");
    }
    for workflow in workflows {
        println!("{}", workflow_line(&workflow));
    }
    Ok(())
}

pub fn workflow_show(key: &str) -> Result<()> {
    let workflow = request("workflow", workflow_key(key))?;
    println!("{}", serde_json::to_string_pretty(&workflow)?);
    Ok(())
}

pub fn workflow_record(action: &str, name: Option<&str>) -> Result<()> {
    let args = match action {
        "start" => serde_json::json!({ "action": "start", "name": name.unwrap_or_default() }),
        other => serde_json::json!({ "action": other }),
    };
    let data = request("workflow_record", args)?;
    match action {
        "start" => println!(
            "Recording workflow \u{201c}{}\u{201d}. Switch applications, then: buddy workflow stop",
            data["recording"].as_str().unwrap_or(name.unwrap_or("?"))
        ),
        "stop" => println!("Saved: {}", workflow_line(&data)),
        "cancel" => println!("Recording cancelled."),
        _ => println!("{}", serde_json::to_string_pretty(&data)?),
    }
    Ok(())
}

pub fn workflow_capture(action: &str, name: Option<&str>) -> Result<()> {
    let args = match action {
        "start" => serde_json::json!({ "action": "start", "name": name.unwrap_or_default() }),
        other => serde_json::json!({ "action": other }),
    };
    let data = request("workflow_capture", args)?;
    match action {
        "start" => {
            println!("Capturing inside-application steps. Focus a button or menu item, then:");
            println!("  buddy workflow capture-step");
            println!("Stop with: buddy workflow stop");
        }
        "stop" => println!("Saved: {}", workflow_line(&data)),
        "cancel" => println!("Capture cancelled."),
        _ => println!("{}", serde_json::to_string_pretty(&data)?),
    }
    Ok(())
}

pub fn workflow_capture_step() -> Result<()> {
    let data = request("workflow_capture_step", Value::Null)?;
    let step = &data["captured"];
    println!(
        "Captured: {} '{}' in {} ({} steps)",
        step["role"].as_str().unwrap_or("?"),
        step["name"].as_str().unwrap_or("?"),
        step["frame"].as_str().unwrap_or("?"),
        data["steps"].as_u64().unwrap_or(0)
    );
    Ok(())
}

pub fn workflow_delete(key: &str) -> Result<()> {
    let data = request("workflow_delete", workflow_key(key))?;
    if data["deleted"] == true {
        println!("Workflow deleted.");
    } else {
        println!("No such workflow.");
    }
    Ok(())
}

/// Turn the microphone on only when the selected audio model can actually hear.
pub fn voice(on: bool) -> Result<()> {
    if !on {
        request("sense", serde_json::json!({ "source": "microphone", "on": false }))?;
        println!("Microphone off.");
        return Ok(());
    }
    let providers = request("providers", Value::Null)?;
    let local = &providers["local"];
    if local["available"] != true {
        bail!("the local model runtime is not available: {}", local["why"].as_str().unwrap_or("?"));
    }
    let model = local["understands_audio_with"].as_str().unwrap_or("?");
    let capabilities: Vec<&str> = local["capabilities"]["audio"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    if !capabilities.iter().any(|capability| *capability == "audio") {
        bail!("the audio model '{model}' has no audio input; choose one with: buddy ai model NAME");
    }
    request("sense", serde_json::json!({ "source": "microphone", "on": true }))?;
    println!("Listening through {model}. Say his name, or speak to him within twenty seconds of his name.");
    println!("(If privacy mode is on, nothing is heard. Turn it off with: buddy privacy off)");
    Ok(())
}

pub fn providers() -> Result<()> {
    let p = request_within("providers", Value::Null, Duration::from_secs(20))?;
    let local = &p["local"];
    if local["available"] == true {
        let models: Vec<&str> = local["models"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if models.is_empty() {
            println!("Local models (Ollama): available — no models installed");
        } else {
            println!("Local models (Ollama): available — {}", models.join(", "));
        }
        println!("  runtime: {} at {}", local["version"].as_str().unwrap_or("unknown version"), local["endpoint"].as_str().unwrap_or("?"));
        println!(
            "  language: {}    images: {}    audio: {}",
            local["understands_language_with"].as_str().unwrap_or("?"),
            local["looks_at_images_with"].as_str().unwrap_or("?"),
            local["understands_audio_with"].as_str().unwrap_or("?")
        );
        for (label, key) in [("language", "language"), ("images", "vision"), ("audio", "audio")] {
            let capabilities: Vec<&str> = local["capabilities"][key].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            if !capabilities.is_empty() {
                println!("  {label} supports: {}", capabilities.join(", "));
            }
        }
    } else {
        println!("Local models (Ollama): not available at {} — {}", local["endpoint"].as_str().unwrap_or("?"), local["why"].as_str().unwrap_or("?"));
    }
    println!("Order: {}.", p["order"].as_str().unwrap_or("?"));
    println!("Without it, he still lives, obeys the words he knows, and does what his own vocabulary can work out.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_workflow_line_names_the_steps_and_the_record() {
        let value = serde_json::json!({
            "id": 1, "name": "morning", "apps": ["terminal", "editor"],
            "runs": 2, "successes": 1, "failures": 1
        });
        assert_eq!(workflow_line(&value), "1  morning  terminal > editor  (2 runs, 1 ok, 1 failed)");
    }
}
