//! Named helper scripts: the safe half of Phase 7.
//!
//! A helper is a JSON manifest in the user's data directory. It declares a name,
//! arguments, an argv (never a shell string), an optional working directory and a
//! timeout. The owner creates the manifest; Buddy can list it, dry-run it, and ask
//! for permission before running it. No model ever supplies a raw shell command.
//!
//! Manifest shape:
//! {
//!   "name": "project-tests",
//!   "description": "Run the tests in a project",
//!   "arguments": ["path"],
//!   "command": ["/usr/bin/cargo", "test"],
//!   "cwd": "{path}",
//!   "timeout_secs": 300,
//!   "risk": "changes",
//!   "dry_run": ["/usr/bin/cargo", "test", "--no-run"]
//! }

use crate::skills::Permissions;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_TIMEOUT_SECS: u64 = 600;
const MAX_OUTPUT: usize = 4_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Helper {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub command: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub risk: String,
    #[serde(default)]
    pub dry_run: Option<Vec<String>>,
    /// Run under bubblewrap with no network and only the helper/cwd/approved folders.
    #[serde(default)]
    pub sandbox: bool,
    /// An optional argv that must succeed after the helper command.
    #[serde(default)]
    pub verify: Option<Vec<String>>,
    #[serde(skip)]
    pub source: PathBuf,
}

fn default_timeout() -> u64 {
    120
}

pub fn dir() -> PathBuf {
    buddy_core::paths::data_dir().join("helpers")
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 60 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn placeholders(value: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else { break };
        let name = &rest[..end];
        if !name.is_empty() && !found.iter().any(|item| item == name) {
            found.push(name.to_string());
        }
        rest = &rest[end + 1..];
    }
    found
}

fn resolve_program(source: &Path, program: &str) -> Result<PathBuf> {
    let path = Path::new(program);
    let joined = if path.is_absolute() { path.to_path_buf() } else { source.join(path) };
    let real = joined.canonicalize().with_context(|| format!("helper program '{}' was not found", joined.display()))?;
    if !real.is_file() {
        bail!("helper program '{}' is not a file", real.display());
    }
    Ok(real)
}

fn validate(helper: &Helper, source_dir: &Path) -> Result<()> {
    if !valid_name(&helper.name) {
        bail!("helper name '{}' must contain only letters, digits, '-' or '_'", helper.name);
    }
    if helper.description.trim().is_empty() {
        bail!("helper '{}' needs a description", helper.name);
    }
    if helper.command.is_empty() || helper.command[0].trim().is_empty() {
        bail!("helper '{}' needs a command", helper.name);
    }
    if helper.timeout_secs == 0 || helper.timeout_secs > MAX_TIMEOUT_SECS {
        bail!("helper '{}' timeout must be 1..={MAX_TIMEOUT_SECS} seconds", helper.name);
    }
    let unique: std::collections::BTreeSet<&String> = helper.arguments.iter().collect();
    if unique.len() != helper.arguments.len() || helper.arguments.iter().any(|arg| !valid_name(arg)) {
        bail!("helper '{}' has duplicate or invalid argument names", helper.name);
    }
    for word in helper
        .command
        .iter()
        .chain(helper.dry_run.iter().flatten())
        .chain(helper.verify.iter().flatten())
        .chain(helper.cwd.iter())
    {
        for name in placeholders(word) {
            if !helper.arguments.contains(&name) {
                bail!("helper '{}' uses undeclared argument '{{{name}}}'", helper.name);
            }
        }
    }
    for raw in helper.command.iter().chain(helper.verify.iter().flatten()) {
        if raw.trim().is_empty() {
            bail!("helper '{}' has an empty command argument", helper.name);
        }
    }
    let _ = resolve_program(source_dir, &helper.command[0])?;
    if let Some(verify) = &helper.verify {
        if verify.is_empty() {
            bail!("helper '{}' has an empty verify command", helper.name);
        }
        let _ = resolve_program(source_dir, &verify[0])?;
    }
    Ok(())
}

pub fn load_all(directory: &Path) -> Result<Vec<Helper>> {
    let mut helpers = Vec::new();
    if !directory.is_dir() {
        return Ok(helpers);
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let mut helper: Helper = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        let source = path.parent().unwrap_or(directory).to_path_buf();
        helper.source = path.clone();
        validate(&helper, &source)?;
        if helpers.iter().any(|other: &Helper| other.name == helper.name) {
            bail!("two helpers are named '{}'; names must be unique", helper.name);
        }
        helpers.push(helper);
    }
    helpers.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(helpers)
}

pub fn find(directory: &Path, name: &str) -> Result<Helper> {
    load_all(directory)?.into_iter().find(|helper| helper.name == name).with_context(|| format!("there is no helper called '{name}' (see: buddy helpers)"))
}

fn values_for(helper: &Helper, values: &BTreeMap<String, String>) -> Result<()> {
    for argument in &helper.arguments {
        if values.get(argument).is_none_or(|value| value.is_empty()) {
            bail!("helper '{}' needs --arg {argument}=VALUE", helper.name);
        }
    }
    for key in values.keys() {
        if !helper.arguments.contains(key) {
            bail!("helper '{}' has no argument '{key}'", helper.name);
        }
    }
    Ok(())
}

fn substitute(value: &str, values: &BTreeMap<String, String>) -> String {
    let mut result = value.to_string();
    for (name, value) in values {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

fn display(argv: &[String]) -> String {
    argv.iter().map(|arg| format!("{arg:?}")).collect::<Vec<_>>().join(" ")
}

fn argv_for(
    helper: &Helper,
    raw: &[String],
    values: &BTreeMap<String, String>,
    permissions: &Permissions,
) -> Result<(PathBuf, Vec<String>, PathBuf)> {
    values_for(helper, values)?;
    if raw.is_empty() {
        bail!("helper '{}' has an empty command", helper.name);
    }
    let source = helper.source.parent().unwrap_or(Path::new("."));
    let program = resolve_program(source, &raw[0])?;
    let mut argv: Vec<String> = raw.iter().map(|word| substitute(word, values)).collect();
    argv[0] = program.to_string_lossy().into_owned();
    let cwd = match &helper.cwd {
        Some(cwd) => {
            let path = PathBuf::from(substitute(cwd, values));
            let path = if path.is_absolute() { path } else { source.join(path) };
            let real = path.canonicalize().with_context(|| format!("helper cwd '{}' does not exist", path.display()))?;
            let allowed = real.starts_with(source) || permissions.folders.iter().filter_map(|f| f.canonicalize().ok()).any(|folder| real.starts_with(folder));
            if !allowed {
                bail!("helper cwd '{}' is outside the helper folder and his approved folders", real.display());
            }
            real
        }
        None => source.to_path_buf(),
    };
    Ok((program, argv, cwd))
}

fn sandboxed(program: &Path, argv: &[String], cwd: &Path, helper_source: &Path, permissions: &Permissions) -> Result<Command> {
    let bwrap = which("bwrap").context("this helper asks for a sandbox, but bubblewrap (bwrap) is not installed")?;
    let mut command = Command::new(bwrap);
    for arg in [
        "--die-with-parent",
        "--new-session",
        "--unshare-all",
        "--proc", "/proc",
        "--dev", "/dev",
        "--tmpfs", "/tmp",
    ] {
        command.arg(arg);
    }
    for path in ["/usr", "/bin", "/lib", "/lib64", "/etc"] {
        if Path::new(path).exists() {
            command.args(["--ro-bind", path, path]);
        }
    }
    let source = helper_source;
    command.args(["--ro-bind", &source.to_string_lossy(), &source.to_string_lossy()]);
    if cwd != source {
        command.args(["--bind", &cwd.to_string_lossy(), &cwd.to_string_lossy()]);
    } else {
        command.args(["--bind", &cwd.to_string_lossy(), &cwd.to_string_lossy()]);
    }
    for folder in permissions.folders.iter().filter_map(|f| f.canonicalize().ok()) {
        if folder != cwd && folder != source && !cwd.starts_with(&folder) {
            command.args(["--ro-bind", &folder.to_string_lossy(), &folder.to_string_lossy()]);
        }
    }
    command
        .args(["--chdir", &cwd.to_string_lossy()])
        .args(["--clearenv", "--setenv", "PATH", "/usr/bin:/bin", "--setenv", "HOME", "/tmp", "--setenv", "TMPDIR", "/tmp"])
        .arg("--")
        .arg(program)
        .args(&argv[1..]);
    Ok(command)
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|dir| dir.join(name)).find(|candidate| candidate.is_file())
    })
}

fn execute(helper: &Helper, raw: &[String], values: &BTreeMap<String, String>, permissions: &Permissions, label: &str) -> Result<String> {
    let (program, argv, cwd) = argv_for(helper, raw, values, permissions)?;
    let source = helper.source.parent().unwrap_or(Path::new("."));
    let mut command = if helper.sandbox {
        sandboxed(&program, &argv, &cwd, source, permissions)?
    } else {
        let mut command = Command::new(&program);
        command.args(&argv[1..]).current_dir(&cwd);
        command
    };
    let started = Instant::now();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("could not start helper '{}' ({label})", helper.name))?;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(helper.timeout_secs) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("helper '{}' {label} was still running after {} s and was stopped", helper.name, helper.timeout_secs);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    let mut detail = String::from_utf8_lossy(&output.stdout).into_owned();
    detail.push_str(&String::from_utf8_lossy(&output.stderr));
    if detail.len() > MAX_OUTPUT {
        detail.truncate(detail.char_indices().take_while(|(i, _)| *i < MAX_OUTPUT).last().map_or(0, |(i, c)| i + c.len_utf8()));
        detail.push_str("\n… (cut short)");
    }
    if !output.status.success() {
        bail!("helper '{}' {label} failed ({}): {}", helper.name, output.status, detail.trim());
    }
    Ok(detail)
}

pub fn dry_run(helper: &Helper, values: &BTreeMap<String, String>, permissions: &Permissions) -> Result<Value> {
    let raw = helper.dry_run.as_ref().unwrap_or(&helper.command);
    let (_program, argv, cwd) = argv_for(helper, raw, values, permissions)?;
    Ok(serde_json::json!({
        "helper": helper.name,
        "command": display(&argv),
        "cwd": cwd,
        "risk": helper.risk,
        "sandbox": helper.sandbox,
        "verify": helper.verify,
        "description": helper.description,
    }))
}

pub fn run(helper: &Helper, values: &BTreeMap<String, String>, permissions: &Permissions) -> Result<(String, String)> {
    let mut detail = execute(helper, &helper.command, values, permissions, "command")?;
    if let Some(verify) = &helper.verify {
        let verify_detail = execute(helper, verify, values, permissions, "verification")?;
        if !verify_detail.trim().is_empty() {
            if !detail.is_empty() {
                detail.push('\n');
            }
            detail.push_str(verify_detail.trim());
        }
    }
    Ok((format!("helper '{}' ran", helper.name), detail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sandbox(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("buddy-helper-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn manifest(name: &str, command: Vec<&str>) -> Helper {
        Helper {
            name: name.into(),
            description: "test helper".into(),
            arguments: vec!["word".into()],
            command: command.into_iter().map(str::to_string).collect(),
            cwd: None,
            timeout_secs: 5,
            risk: "looks".into(),
            dry_run: None,
            sandbox: false,
            verify: None,
            source: PathBuf::new(),
        }
    }

    #[test]
    fn manifests_render_and_dry_run_without_a_shell() {
        let dir = sandbox("render");
        let helper = manifest("echo", vec!["/bin/echo", "hello", "{word}"]);
        let mut values = BTreeMap::new();
        values.insert("word".into(), "world; not a command".into());
        let dry = dry_run(&helper, &values, &Permissions::default()).unwrap();
        assert!(dry["command"].as_str().unwrap().contains("not a command"));
        let (summary, detail) = run(&helper, &values, &Permissions::default()).unwrap();
        assert_eq!(summary, "helper 'echo' ran");
        assert_eq!(detail.trim(), "hello world; not a command");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn bad_names_and_undeclared_arguments_are_refused() {
        let dir = sandbox("validate");
        let mut bad = manifest("bad name", vec!["/bin/echo", "{missing}"]);
        assert!(validate(&bad, &dir).is_err());
        bad.name = "good".into();
        assert!(validate(&bad, &dir).is_err());
        bad.arguments = vec!["missing".into()];
        bad.command = vec!["/bin/echo".into(), "{missing}".into()];
        assert!(validate(&bad, &dir).is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sandbox_and_verify_run_without_a_shell() {
        if which("bwrap").is_none() {
            return;
        }
        let dir = sandbox("sandbox");
        let marker = dir.join("made.txt");
        let mut helper = manifest("make", vec!["/bin/touch", "{path}"]);
        helper.arguments = vec!["path".into()];
        helper.sandbox = true;
        helper.verify = Some(vec!["/usr/bin/test".into(), "-f".into(), "{path}".into()]);
        helper.source = dir.join("helper.json");
        let mut values = BTreeMap::new();
        values.insert("path".into(), marker.to_string_lossy().into_owned());
        let (summary, _) = run(&helper, &values, &Permissions::default()).unwrap();
        assert_eq!(summary, "helper 'make' ran");
        assert!(marker.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_helper_needs_all_declared_arguments() {
        let helper = manifest("echo", vec!["/bin/echo", "{word}"]);
        let error = dry_run(&helper, &BTreeMap::new(), &Permissions::default()).unwrap_err();
        assert!(format!("{error:#}").contains("--arg word=VALUE"));
    }
}
