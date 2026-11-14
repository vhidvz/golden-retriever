//! Claude Code as an optional helper for harder tasks.
//!
//! Buddy only ever runs the user's own installed `claude` binary, logged in by
//! the user, in a clean environment. He never reads its credentials. An API
//! key in the environment is removed before starting it, so work is not
//! silently billed somewhere the user did not choose. Everything here is
//! optional: when Claude Code is missing, logged out, over its usage limit or
//! offline, the caller is told which, and the rest of Buddy carries on.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(15 * 60);

fn claude() -> Command {
    let mut cmd = Command::new("claude");
    // Only what it needs to find its login and run: no inherited API keys or tokens.
    cmd.env_clear();
    for keep in ["HOME", "PATH", "USER", "LANG", "XDG_CONFIG_HOME", "XDG_RUNTIME_DIR"] {
        if let Some(value) = std::env::var_os(keep) {
            cmd.env(keep, value);
        }
    }
    cmd.stdin(Stdio::null());
    cmd
}

/// Whether Claude Code can be used right now, and if not, why not, in words for the user.
pub fn status() -> Value {
    let output = match claude().args(["auth", "status", "--json"]).stderr(Stdio::null()).output() {
        Ok(output) => output,
        Err(_) => return serde_json::json!({ "available": false, "why": "Claude Code is not installed" }),
    };
    let said: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    if said["loggedIn"] != true {
        return serde_json::json!({ "available": false, "why": "Claude Code is not logged in (run: claude auth login)" });
    }
    serde_json::json!({
        "available": true,
        "signed_in_with": said["authMethod"],
        "plan": said["subscriptionType"],
        "login_expires": said["loginExpiresAt"],
    })
}

pub struct Answer {
    pub text: String,
    /// The usage-limit state Claude Code reported, if it did.
    pub limit: Value,
    pub secs: f64,
}

/// Reads Claude Code's line-per-event output: the final result, and the usage-limit notice.
fn digest(lines: impl Iterator<Item = String>) -> (Option<Value>, Value) {
    let (mut result, mut limit) = (None, Value::Null);
    for line in lines {
        let Ok(event) = serde_json::from_str::<Value>(&line) else { continue };
        match event["type"].as_str() {
            Some("rate_limit_event") => limit = event["rate_limit_info"].clone(),
            Some("result") => result = Some(event),
            _ => {}
        }
    }
    (result, limit)
}

/// Turns Claude Code's own failure text into what the user needs to know.
fn explain(failure: &str, limit: &Value) -> String {
    let lower = failure.to_lowercase();
    if lower.contains("not logged in") || lower.contains("oauth") || lower.contains("authenticat") {
        "Claude Code is not logged in any more (run: claude auth login)".into()
    } else if lower.contains("limit") || limit["status"].as_str().is_some_and(|s| s != "allowed") {
        match limit["resetsAt"].as_i64() {
            Some(at) => format!("the Claude usage limit is reached; it resets at Unix time {at} (date -d @{at})"),
            None => "the Claude usage limit is reached".into(),
        }
    } else if lower.contains("connect") || lower.contains("timed out") || lower.contains("econn") || lower.contains("socket") {
        "Claude could not be reached (no network?)".into()
    } else {
        format!("Claude Code failed: {}", failure.trim())
    }
}

/// Hands a task to Claude Code in `folder`. With `may_edit` it can read and change
/// files there; without, it can only read. It can never run commands.
pub fn ask(task: &str, folder: &Path, may_edit: bool) -> Result<Answer> {
    let started = Instant::now();
    let tools = if may_edit { "Read,Grep,Glob,Edit,Write" } else { "Read,Grep,Glob" };
    let mut cmd = claude();
    cmd.arg("-p").arg(task).args([
        "--output-format",
        "stream-json",
        "--verbose",
        "--setting-sources",
        "",
        "--strict-mcp-config",
        "--no-session-persistence",
    ]);
    cmd.args(["--tools", tools, "--allowedTools", tools]);
    if may_edit {
        cmd.args(["--permission-mode", "acceptEdits"]);
    }
    let mut child = cmd.current_dir(folder).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().context("Claude Code is not installed")?;
    let stdout = child.stdout.take().context("no output from Claude Code")?;
    let reader = std::thread::spawn(move || digest(BufReader::new(stdout).lines().map_while(|l| l.ok())));
    while child.try_wait()?.is_none() {
        if started.elapsed() > PATIENCE {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Claude Code was still working after fifteen minutes, so he stopped it");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let (result, limit) = reader.join().unwrap_or((None, Value::Null));
    let Some(result) = result else {
        let mut complaint = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut complaint);
        }
        bail!("{}", explain(&complaint, &limit));
    };
    let text = result["result"].as_str().unwrap_or_default().to_string();
    // Claude Code reports some failures as a "successful" result flagged is_error.
    if result["is_error"] == true {
        bail!("{}", explain(&text, &limit));
    }
    Ok(Answer { text, limit, secs: started.elapsed().as_secs_f64() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lines(events: &[Value]) -> impl Iterator<Item = String> + '_ {
        events.iter().map(|e| e.to_string())
    }

    #[test]
    fn the_result_and_the_usage_limit_are_read_from_the_event_stream() {
        let events = [
            json!({ "type": "system", "subtype": "init" }),
            json!({ "type": "rate_limit_event", "rate_limit_info": { "status": "allowed", "resetsAt": 1794490800 } }),
            json!({ "type": "assistant" }),
            json!({ "type": "result", "subtype": "success", "is_error": false, "result": "woof" }),
        ];
        let (result, limit) = digest(lines(&events).chain(["not json at all".to_string()]));
        assert_eq!(result.unwrap()["result"], "woof");
        assert_eq!(limit["resetsAt"], 1794490800);
    }

    #[test]
    fn failures_are_explained_in_terms_the_user_can_act_on() {
        let none = Value::Null;
        assert!(explain("Not logged in · Please run /login", &none).contains("claude auth login"));
        assert!(explain("Failed to authenticate: OAuth session expired", &none).contains("claude auth login"));
        assert!(explain("API Error: Connection refused (ECONNREFUSED)", &none).contains("could not be reached"));
        let limited = json!({ "status": "rejected", "resetsAt": 1794490800 });
        assert!(explain("You've hit your session limit", &limited).contains("date -d @1794490800"));
        assert!(explain("something odd", &none).contains("something odd"));
    }

    #[test]
    fn no_api_key_reaches_claude_code() {
        // SAFETY: tests in this module do not read these variables concurrently.
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-not-a-real-key") };
        let cmd = claude();
        let passed: Vec<String> = cmd.get_envs().filter_map(|(k, v)| v.map(|_| k.to_string_lossy().into_owned())).collect();
        assert!(!passed.iter().any(|k| k.contains("ANTHROPIC") || k.contains("CLAUDE")), "{passed:?}");
        assert!(passed.contains(&"HOME".to_string()));
        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };
    }
}
