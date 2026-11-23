//! What Buddy can do on the computer, and what he is allowed to.
//!
//! Skills are ordinary local code. A model (or his own vocabulary) may choose a
//! skill and its argument, but only this module ever touches the system, and
//! only within what the user has granted: a policy per skill, a list of
//! folders he may look in and change, and a list of commands approved by hand.

use anyhow::{Context, Result, bail};
use buddy_core::language::normalise;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    /// He asks first, every time.
    Ask,
    /// He may go ahead.
    Allow,
    /// He must not.
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    /// Only reads or shows something.
    Looks,
    /// Changes something, in a way that is small or can be undone.
    Changes,
    /// Could do real damage or send data off the machine. Always needs a yes.
    Sensitive,
}

pub struct Skill {
    pub name: &'static str,
    pub what: &'static str,
    pub risk: Risk,
    /// The names of its arguments, the first being the main one.
    pub arguments: &'static [&'static str],
}

pub const SKILLS: [Skill; 9] = [
    Skill { name: "find_files", what: "Find files by name in the folders he may use.", risk: Risk::Looks, arguments: &["query"] },
    Skill { name: "read_file", what: "Read a text file in the folders he may use.", risk: Risk::Looks, arguments: &["path"] },
    Skill { name: "open_app", what: "Open an installed application.", risk: Risk::Changes, arguments: &["name"] },
    Skill { name: "open_file", what: "Open a file or folder with its usual application.", risk: Risk::Changes, arguments: &["path"] },
    Skill { name: "make_note", what: "Add a line to his notes file.", risk: Risk::Changes, arguments: &["text"] },
    Skill { name: "move_file", what: "Move or rename a file within the folders he may use.", risk: Risk::Changes, arguments: &["from", "to"] },
    Skill { name: "run_command", what: "Run a shell command.", risk: Risk::Sensitive, arguments: &["command"] },
    Skill { name: "replay_workflow", what: "Replay a saved workflow and check each step.", risk: Risk::Changes, arguments: &["workflow", "id"] },
    Skill {
        name: "run_helper",
        what: "Run a named helper script the owner registered. It always asks first.",
        risk: Risk::Sensitive,
        arguments: &["helper", "args"],
    },
];

pub fn skill(name: &str) -> Option<&'static Skill> {
    SKILLS.iter().find(|s| s.name == name)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Permissions {
    policies: BTreeMap<String, Policy>,
    /// Folders he may search, read and move files in.
    pub folders: Vec<PathBuf>,
    /// Shell commands approved once and for all, word for word.
    pub commands: Vec<String>,
}

impl Permissions {
    pub fn policy(&self, name: &str) -> Policy {
        let Some(skill) = skill(name) else { return Policy::Deny };
        match (self.policies.get(name), skill.risk) {
            // Nothing can make a sensitive skill run without a yes.
            (Some(Policy::Deny), _) => Policy::Deny,
            (_, Risk::Sensitive) => Policy::Ask,
            (Some(policy), _) => *policy,
            (None, Risk::Looks) => Policy::Allow,
            (None, Risk::Changes) => Policy::Ask,
        }
    }

    pub fn set_policy(&mut self, name: &str, policy: Policy) -> Result<()> {
        let skill = skill(name).with_context(|| format!("he has no skill called '{name}'"))?;
        if skill.risk == Risk::Sensitive && policy == Policy::Allow {
            bail!("'{name}' can send data away or change anything, so it always asks first; it cannot be set to allow");
        }
        self.policies.insert(name.to_string(), policy);
        Ok(())
    }

    /// The real path of `path`, if it lies inside a folder he may use.
    fn within(&self, path: &Path) -> Result<PathBuf> {
        let real = path.canonicalize().with_context(|| format!("there is no {}", path.display()))?;
        if self.folders.iter().filter_map(|f| f.canonicalize().ok()).any(|folder| real.starts_with(folder)) {
            Ok(real)
        } else {
            bail!("{} is outside the folders he may use (see: buddy folders)", path.display())
        }
    }
}

/// How to take back something he did.
#[derive(Clone, Debug, PartialEq)]
pub enum Undo {
    MoveBack { from: PathBuf, to: PathBuf },
    DropNote { file: PathBuf, line: String },
}

#[derive(Debug)]
pub struct Done {
    /// One line for the user.
    pub summary: String,
    /// Anything longer: the files found, the text read, a command's output.
    pub detail: String,
    pub undo: Option<Undo>,
}

fn text<'a>(args: &'a Value, name: &str) -> Result<&'a str> {
    args[name].as_str().map(str::trim).filter(|s| !s.is_empty()).with_context(|| format!("it needs a {name}"))
}

/// A sentence saying what running this would do, for the user to say yes or no to.
pub fn describe(name: &str, args: &Value) -> String {
    let arg = |n: &str| args[n].as_str().unwrap_or("?").to_string();
    match name {
        "find_files" => format!("look for files named like \u{201c}{}\u{201d}", arg("query")),
        "read_file" => format!("read {}", arg("path")),
        "open_app" => format!("open the application \u{201c}{}\u{201d}", arg("name")),
        "open_file" => format!("open {}", arg("path")),
        "make_note" => format!("note down \u{201c}{}\u{201d}", arg("text")),
        "move_file" => format!("move {} to {}", arg("from"), arg("to")),
        "run_command" => format!("run the command: {}", arg("command")),
        "replay_workflow" => format!("replay the workflow \u{201c}{}\u{201d}", arg("workflow")),
        "run_helper" => format!("run the helper \u{201c}{}\u{201d}", arg("helper")),
        other => format!("do '{other}'"),
    }
}

/// Carries out a skill. Policy is the caller's business; scope is checked here.
pub fn run(name: &str, args: &Value, permissions: &Permissions, notes: &Path) -> Result<Done> {
    match name {
        "find_files" => find_files(text(args, "query")?, permissions),
        "read_file" => {
            let path = permissions.within(Path::new(text(args, "path")?))?;
            let bytes = fs::read(&path)?;
            if bytes.len() > 20_000 {
                bail!("{} is too long for him to read ({} KB)", path.display(), bytes.len() / 1000);
            }
            let content = String::from_utf8(bytes).ok().context("that is not a text file")?;
            Ok(Done { summary: format!("read {} ({} lines)", path.display(), content.lines().count()), detail: content, undo: None })
        }
        "open_app" => open_app(text(args, "name")?),
        "open_file" => {
            let path = permissions.within(Path::new(text(args, "path")?))?;
            Command::new("xdg-open")
                .arg(&path)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .context("could not open it")?;
            Ok(Done { summary: format!("opened {}", path.display()), detail: String::new(), undo: None })
        }
        "make_note" => {
            let line = format!("- {} {}", timestamp(), text(args, "text")?.replace('\n', " "));
            let mut all = fs::read_to_string(notes).unwrap_or_default();
            all.push_str(&line);
            all.push('\n');
            if let Some(dir) = notes.parent() {
                fs::create_dir_all(dir)?;
            }
            fs::write(notes, all)?;
            Ok(Done {
                summary: format!("noted in {}", notes.display()),
                detail: line.clone(),
                undo: Some(Undo::DropNote { file: notes.to_path_buf(), line }),
            })
        }
        "move_file" => {
            let from = permissions.within(Path::new(text(args, "from")?))?;
            let to = PathBuf::from(text(args, "to")?);
            let to = if to.is_dir() { to.join(from.file_name().context("nothing to move")?) } else { to };
            let into = permissions.within(to.parent().context("nowhere to move it to")?)?;
            let to = into.join(to.file_name().context("it needs a name")?);
            if to.exists() {
                bail!("{} already exists; he will not overwrite it", to.display());
            }
            fs::rename(&from, &to)?;
            Ok(Done {
                summary: format!("moved {} to {}", from.display(), to.display()),
                detail: String::new(),
                undo: Some(Undo::MoveBack { from: to, to: from }),
            })
        }
        "run_command" => run_command(text(args, "command")?),
        "run_helper" => bail!("helper execution is handled by the daemon"),
        other => bail!("he has no skill called '{other}'"),
    }
}

pub fn undo(what: &Undo) -> Result<String> {
    match what {
        Undo::MoveBack { from, to } => {
            if to.exists() {
                bail!("{} is in the way; nothing was changed", to.display());
            }
            fs::rename(from, to)?;
            Ok(format!("moved {} back to {}", from.display(), to.display()))
        }
        Undo::DropNote { file, line } => {
            let all = fs::read_to_string(file)?;
            let mut lines: Vec<&str> = all.lines().collect();
            let at = lines.iter().rposition(|l| l == line).context("that note is no longer there")?;
            lines.remove(at);
            fs::write(file, lines.iter().map(|l| format!("{l}\n")).collect::<String>())?;
            Ok("took the note back out".into())
        }
    }
}

fn timestamp() -> String {
    Command::new("date")
        .arg("+%Y-%m-%d %H:%M")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Files whose names contain every word of the query, newest first.
fn find_files(query: &str, permissions: &Permissions) -> Result<Done> {
    if permissions.folders.is_empty() {
        bail!("he has not been given any folders to look in (give him one with: buddy folders add ~/Documents)");
    }
    let words: Vec<String> = normalise(query).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect();
    let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
    let started = Instant::now();
    let mut stack: Vec<(PathBuf, usize)> = permissions.folders.iter().map(|f| (f.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        // A bounded search: he answers in a couple of seconds or with what he has.
        if started.elapsed() > Duration::from_secs(4) {
            break;
        }
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else { continue };
            if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target" | "__pycache__") {
                continue;
            }
            if kind.is_dir() && depth < 6 {
                stack.push((entry.path(), depth + 1));
            }
            let plain = normalise(&name);
            if words.iter().all(|w| plain.contains(w.as_str())) {
                found.push((entry.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH), entry.path()));
            }
        }
    }
    found.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    found.truncate(20);
    let listing: Vec<String> = found.iter().map(|(_, p)| p.display().to_string()).collect();
    let summary = match listing.len() {
        0 => format!("found nothing named like \u{201c}{query}\u{201d}"),
        n => format!("found {n} file{} named like \u{201c}{query}\u{201d}", if n == 1 { "" } else { "s" }),
    };
    Ok(Done { summary, detail: listing.join("\n"), undo: None })
}

/// Common Persian names for kinds of application, mapped to words found in launchers.
const PERSIAN_APP_WORDS: [(&str, &str); 8] = [
    ("ماشین حساب", "calculator"),
    ("ماشینحساب", "calculator"),
    ("مرورگر", "browser"),
    ("ترمینال", "terminal"),
    ("فایل ها", "files"),
    ("پرونده ها", "files"),
    ("تنظیمات", "settings"),
    ("ویرایشگر متن", "text editor"),
];

/// Every installed application that has a launcher: (launcher file, name, searchable text).
fn launchers() -> Vec<(PathBuf, String, String)> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let dirs = [
        PathBuf::from("/usr/share/applications"),
        home.join(".local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];
    let mut all = Vec::new();
    for entry in dirs.iter().flat_map(|d| fs::read_dir(d).into_iter().flatten().flatten()) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
            continue;
        }
        let Ok(content) = fs::read_to_string(&path) else { continue };
        let field = |key: &str| content.lines().find_map(|l| l.strip_prefix(key)).map(str::to_string);
        if field("NoDisplay=").as_deref() == Some("true") || field("Exec=").is_none() {
            continue;
        }
        let Some(name) = field("Name=") else { continue };
        let searchable = normalise(&format!(
            "{name} {} {} {}",
            field("GenericName=").unwrap_or_default(),
            field("Keywords=").unwrap_or_default(),
            field("Name[fa]=").unwrap_or_default()
        ));
        all.push((path, name, searchable));
    }
    all
}

/// The launcher that best fits a name the user gave, if any does.
pub fn find_app(wanted_id: &str) -> Option<(PathBuf, String)> {
    let mut wanted = normalise(wanted_id);
    for (persian, english) in PERSIAN_APP_WORDS {
        if wanted.contains(&normalise(persian)) {
            wanted = english.to_string();
        }
    }
    // The desktop sense names applications by their launcher id ("org.gnome.Nautilus"); that settles it at once.
    let all = launchers();
    let id = wanted_id.trim().trim_end_matches(".desktop").to_lowercase();
    if let Some((path, name, _)) = all.iter().find(|(path, _, _)| path.file_stem().is_some_and(|stem| stem.to_string_lossy().to_lowercase() == id)) {
        return Some((path.clone(), name.clone()));
    }
    let words: Vec<&str> = wanted.split(' ').filter(|w| !matches!(*w, "the" | "app" | "application" | "برنامه")).collect();
    if words.is_empty() {
        return None;
    }
    all.into_iter()
        .filter(|(_, _, searchable)| words.iter().all(|w| searchable.split(' ').any(|s| s.starts_with(w))))
        // An exact name beats a longer one that merely contains the word.
        .min_by_key(|(_, name, _)| (normalise(name) != wanted, name.len()))
        .map(|(path, name, _)| (path, name))
}

fn open_app(wanted: &str) -> Result<Done> {
    let (launcher, name) = find_app(wanted).with_context(|| format!("he found no installed application called \u{201c}{wanted}\u{201d}"))?;
    Command::new("gio")
        .arg("launch")
        .arg(&launcher)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("could not start it")?;
    Ok(Done { summary: format!("opened {name}"), detail: String::new(), undo: None })
}

fn run_command(command: &str) -> Result<Done> {
    let mut child = Command::new("sh").arg("-c").arg(command).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            let _ = child.wait();
            bail!("the command was still running after a minute, so he stopped it");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    let mut said = String::from_utf8_lossy(&output.stdout).into_owned();
    said.push_str(&String::from_utf8_lossy(&output.stderr));
    if said.len() > 4000 {
        said.truncate(said.char_indices().take_while(|(i, _)| *i < 4000).last().map_or(0, |(i, c)| i + c.len_utf8()));
        said.push_str("\n… (cut short)");
    }
    if !output.status.success() {
        bail!("the command failed ({}): {}", output.status, said.trim());
    }
    Ok(Done { summary: "the command ran".into(), detail: said, undo: None })
}

/// The skills as tool definitions for a model to choose from.
pub fn as_tools() -> Value {
    Value::Array(
        SKILLS
            .iter()
            // The model gets helper tools only after a helper catalogue is attached.
            .filter(|s| s.name != "run_helper")
            .map(|s| {
                let properties: serde_json::Map<String, Value> = s.arguments.iter().map(|a| (a.to_string(), json!({ "type": "string" }))).collect();
                json!({ "type": "function", "function": { "name": s.name, "description": s.what,
                    "parameters": { "type": "object", "properties": properties, "required": [s.arguments[0]] } } })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox(name: &str) -> (PathBuf, Permissions) {
        let dir = std::env::temp_dir().join(format!("buddy-skills-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("allowed/reports")).unwrap();
        fs::create_dir_all(dir.join("private")).unwrap();
        fs::write(dir.join("allowed/reports/tax report 2025.txt"), "numbers").unwrap();
        fs::write(dir.join("allowed/گزارش مالیات.txt"), "ارقام").unwrap();
        fs::write(dir.join("private/secret.txt"), "do not read").unwrap();
        let permissions = Permissions { folders: vec![dir.join("allowed")], ..Permissions::default() };
        (dir, permissions)
    }

    #[test]
    fn he_finds_and_reads_only_inside_the_folders_he_was_given() {
        let (dir, permissions) = sandbox("scope");
        let notes = dir.join("notes.md");
        let found = run("find_files", &json!({ "query": "Tax 2025" }), &permissions, &notes).unwrap();
        assert!(found.detail.ends_with("tax report 2025.txt"), "{}", found.detail);
        let persian = run("find_files", &json!({ "query": "گزارش ماليات" }), &permissions, &notes).unwrap();
        assert!(persian.detail.contains("گزارش مالیات"), "spelling variants of Persian letters still match: {}", persian.detail);
        assert!(run("find_files", &json!({ "query": "secret" }), &permissions, &notes).unwrap().detail.is_empty());

        let read = run("read_file", &json!({ "path": dir.join("allowed/reports/tax report 2025.txt") }), &permissions, &notes).unwrap();
        assert_eq!(read.detail, "numbers");
        let refused = run("read_file", &json!({ "path": dir.join("private/secret.txt") }), &permissions, &notes).unwrap_err();
        assert!(refused.to_string().contains("outside the folders"), "{refused}");
        // A path that climbs out of an allowed folder is still outside.
        assert!(run("read_file", &json!({ "path": dir.join("allowed/../private/secret.txt") }), &permissions, &notes).is_err());
        assert!(run("find_files", &json!({ "query": "x" }), &Permissions::default(), &notes).is_err(), "with no folders he looks nowhere");
    }

    #[test]
    fn what_he_changes_can_be_taken_back() {
        let (dir, permissions) = sandbox("undo");
        let notes = dir.join("notes.md");
        fs::write(&notes, "- an older note\n").unwrap();
        let noted = run("make_note", &json!({ "text": "the build is broken" }), &permissions, &notes).unwrap();
        assert!(fs::read_to_string(&notes).unwrap().contains("the build is broken"));
        undo(&noted.undo.unwrap()).unwrap();
        assert_eq!(fs::read_to_string(&notes).unwrap(), "- an older note\n");

        let from = dir.join("allowed/reports/tax report 2025.txt");
        let moved = run("move_file", &json!({ "from": from, "to": dir.join("allowed") }), &permissions, &notes).unwrap();
        assert!(dir.join("allowed/tax report 2025.txt").exists() && !from.exists());
        undo(&moved.undo.unwrap()).unwrap();
        assert!(from.exists());
        // He will not move things out of his folders, nor over an existing file.
        assert!(run("move_file", &json!({ "from": from, "to": dir.join("private") }), &permissions, &notes).is_err());
        fs::write(dir.join("allowed/taken.txt"), "x").unwrap();
        assert!(run("move_file", &json!({ "from": from, "to": dir.join("allowed/taken.txt") }), &permissions, &notes).is_err());
    }

    #[test]
    fn the_skill_list_is_the_eight_local_skills() {
        assert_eq!(SKILLS.len(), 9);
        assert!(skill("ask_claude").is_none(), "Claude Code was removed; it is not a skill");
        assert!(skill("run_command").is_some());
        assert!(skill("replay_workflow").is_some());
    }

    #[test]
    fn sensitive_skills_always_ask_and_a_deny_is_final() {
        let mut permissions = Permissions::default();
        assert_eq!(permissions.policy("find_files"), Policy::Allow);
        assert_eq!(permissions.policy("open_app"), Policy::Ask);
        assert_eq!(permissions.policy("run_command"), Policy::Ask);
        assert_eq!(permissions.policy("no_such_skill"), Policy::Deny);
        assert!(permissions.set_policy("run_command", Policy::Allow).is_err());
        permissions.set_policy("open_app", Policy::Allow).unwrap();
        permissions.set_policy("run_command", Policy::Deny).unwrap();
        assert_eq!((permissions.policy("open_app"), permissions.policy("run_command")), (Policy::Allow, Policy::Deny));
        let restored: Permissions = serde_json::from_str(&serde_json::to_string(&permissions).unwrap()).unwrap();
        assert_eq!(restored, permissions);
    }

    #[test]
    fn commands_report_their_output_and_their_failures() {
        let ran = run("run_command", &json!({ "command": "printf hello" }), &Permissions::default(), Path::new("/nonexistent")).unwrap();
        assert_eq!(ran.detail, "hello");
        let failed =
            run("run_command", &json!({ "command": "echo oops >&2; exit 3" }), &Permissions::default(), Path::new("/nonexistent")).unwrap_err();
        assert!(failed.to_string().contains("oops"), "{failed}");
    }
}
