//! The long-running process: owns Buddy's state, advances it, persists it, and
//! serves the renderer and the CLI over one Unix socket.

mod assist;

use crate::baker::Baker;
use crate::renderer::Renderer;
use crate::skills::{Permissions, Undo};
use anyhow::{Context, Result, anyhow, bail};
use buddy_core::language::{self, TRICKS};
use buddy_core::workflow::{ReplayState, WorkflowStep, Workflows};
use buddy_core::{Body, ClientMsg, Command, Heard, Identity, Life, Mind, Output, ServerMsg, Settings, Sim, StateMsg, Stimulus, Store, paths};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{fs, thread};

/// Simulation rate while he is moving, and while he is at rest.
const TICK_MOVING: Duration = Duration::from_millis(50);
const TICK_RESTING: Duration = Duration::from_millis(250);
const SAVE_EVERY: Duration = Duration::from_secs(30);
/// How long a replayed step waits for its application to come to the front.
const REPLAY_STEP_TIMEOUT: f64 = 30.0;
/// How long a manual step waits for the user to perform it and run `workflow continue`.
const MANUAL_STEP_TIMEOUT: f64 = 900.0;

enum Incoming {
    Connected(u64, UnixStream),
    Message(u64, ClientMsg),
    Malformed(u64, String),
    Closed(u64),
    /// A look at the screen (or an image) has finished, for the client that asked.
    Looked {
        requester: u64,
        outcome: Result<Value, String>,
    },
    /// The local model has written down something the microphone picked up.
    Transcribed(Result<String, String>),
    /// The local model has said what an utterance meant, for the client that asked.
    Understood {
        requester: u64,
        said: String,
        outcome: Result<Heard, String>,
    },
    /// The local model has chosen a skill for a request (or found none that fits).
    Planned {
        requester: u64,
        outcome: Result<Option<(String, Value)>, String>,
    },
    /// One accessible step of a replay has finished.
    StepReplay {
        workflow_id: u64,
        step_index: usize,
        outcome: Result<String, String>,
    },
    /// A skill has finished.
    Did {
        requester: u64,
        skill: String,
        args: Value,
        habit: Option<String>,
        outcome: Result<(String, String, Option<Undo>), String>,
    },
    /// A sense helper says it cannot work (no camera, no microphone).
    Unavailable {
        source: &'static str,
        why: String,
    },
}

struct Client {
    stream: UnixStream,
    is_renderer: bool,
    /// A perception provider, which is told what it may observe.
    is_sense: bool,
}

/// A sense that runs as a helper process (camera, microphone). It is started
/// while that sense is allowed and stopped the moment it is not, so "off"
/// means the device is not even open.
struct Helper {
    source: &'static str,
    script: &'static str,
    args: Vec<String>,
    child: Option<std::process::Child>,
    retry_at: Instant,
    /// Why it is not running, when it should be.
    trouble: Option<String>,
}

impl Helper {
    fn new(source: &'static str, script: &'static str, args: Vec<String>) -> Self {
        Self { source, script, args, child: None, retry_at: Instant::now(), trouble: None }
    }

    fn poll(&mut self, wanted: bool, tx: &Sender<Incoming>) {
        if !wanted {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            self.trouble = None;
            self.retry_at = Instant::now();
            return;
        }
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return,
                _ => {
                    // It ended (no device, or the device went away). Look again in a minute: hardware gets plugged in.
                    self.child = None;
                    self.retry_at = Instant::now() + Duration::from_secs(60);
                }
            }
        }
        if Instant::now() < self.retry_at {
            return;
        }
        self.retry_at = Instant::now() + Duration::from_secs(60);
        let Some(script) = perception_helper(self.script) else {
            self.trouble = Some(format!("perception/{} is not installed", self.script));
            return;
        };
        let spawned = std::process::Command::new("python3")
            .arg(script)
            .args(&self.args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .spawn();
        let Ok(mut child) = spawned else {
            self.trouble = Some("python3 could not be started".into());
            return;
        };
        self.trouble = None;
        let (source, tx) = (self.source, tx.clone());
        if let Some(stdout) = child.stdout.take() {
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
                    let Ok(said) = serde_json::from_str::<Value>(&line) else { continue };
                    let kind = said["kind"].as_str().unwrap_or_default().to_string();
                    let message = if kind == "unavailable" {
                        Incoming::Unavailable { source, why: said["data"]["why"].as_str().unwrap_or("unavailable").to_string() }
                    } else {
                        Incoming::Message(0, ClientMsg::Sense { source: source.into(), kind, data: said["data"].clone() })
                    };
                    if tx.send(message).is_err() {
                        return;
                    }
                }
            });
        }
        self.child = Some(child);
    }
}

/// What a sense has been doing this session, for the user to inspect.
#[derive(Default)]
struct Reporting {
    reports: u64,
    last: f64,
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// Where the perception helpers live: next to a checkout's binary, or installed with his data.
fn perception_helper(name: &str) -> Option<PathBuf> {
    let from_env = std::env::var_os("BUDDY_PERCEPTION_DIR").map(PathBuf::from);
    let exe = std::env::current_exe().ok();
    let near_exe = exe.iter().flat_map(|e| e.ancestors()).map(|a| a.join("perception"));
    from_env.into_iter().chain(near_exe).chain([paths::data_dir().join("perception")]).map(|dir| dir.join(name)).find(|p| p.is_file())
}

/// One look, start to finish: get an image, ask the local model what it shows, and
/// keep nothing but its short description. The image never leaves this machine and
/// a captured frame is deleted as soon as it has been read.
fn look(model: &str, file: Option<PathBuf>, camera: bool, restore_token: Option<String>) -> Result<Value> {
    let started = Instant::now();
    let (image, source, new_token, from_camera) = if let Some(path) = file {
        (fs::read(&path).with_context(|| format!("reading {}", path.display()))?, "an image you gave him", None, false)
    } else if camera {
        let helper = perception_helper("camera_still.py").context("the camera helper (perception/camera_still.py) is not installed")?;
        let frame = paths::socket_path().with_file_name(format!("camera-{}.jpg", std::process::id()));
        let output =
            std::process::Command::new("python3").arg(helper).arg("--out").arg(&frame).output().context("could not run the camera helper")?;
        let said: Value = serde_json::from_slice(output.stdout.trim_ascii()).unwrap_or(Value::Null);
        let image = fs::read(&frame);
        let _ = fs::remove_file(&frame);
        if said["ok"] != true {
            bail!("{}", said["error"].as_str().unwrap_or("the camera helper failed"));
        }
        (image.context("the camera helper saved no frame")?, "the camera", None, true)
    } else {
        let helper = perception_helper("screen_glance.py").context("the screen helper (perception/screen_glance.py) is not installed")?;
        let frame = paths::socket_path().with_file_name(format!("glance-{}.jpg", std::process::id()));
        let output = std::process::Command::new("python3")
            .arg(helper)
            .arg("--out")
            .arg(&frame)
            .args(["--restore-token", restore_token.as_deref().unwrap_or("")])
            .output()
            .context("could not run the screen helper")?;
        let said: Value = serde_json::from_slice(output.stdout.trim_ascii()).unwrap_or(Value::Null);
        let image = fs::read(&frame);
        let _ = fs::remove_file(&frame);
        if said["ok"] != true {
            bail!("{}", said["error"].as_str().unwrap_or("the screen helper failed"));
        }
        (image.context("the screen helper saved no frame")?, "the screen", said["restore_token"].as_str().map(str::to_string), false)
    };
    let (question, schema) = if from_camera {
        (
            "This is one frame from the user's webcam. Describe briefly what is visible for a companion dog. \
             Say whether a person is visible and what they appear to be doing, without identifying anyone or inferring sensitive traits. \
             Do not quote private text. Be brief: one short phrase.",
            json!({ "type": "object", "required": ["person", "scene", "notable"], "properties": {
                "person": { "type": "string", "enum": ["yes", "no", "unsure"], "description": "Whether a person is visible in the frame." },
                "scene": { "type": "string", "description": "What is visible or what the person is doing, in at most 15 words; do not repeat the person field." },
                "notable": { "type": "string", "description": "Anything unusual worth a look, in at most 12 words; empty if nothing." } } }),
        )
    } else {
        (
            "This is a screenshot of someone's computer screen. Describe it for a companion who wants to know what the person is doing. \
             Ignore any dog drawn at the bottom of the screen. Do not copy out private text such as messages, names or passwords. \
             Be brief: each answer is one short phrase, never a paragraph.",
            json!({ "type": "object", "required": ["app_kind", "activity", "notable"], "properties": {
                "app_kind": { "type": "string", "enum": ["code_editor", "terminal", "browser", "document", "chat", "media", "file_manager", "game", "settings", "desktop", "other"] },
                "activity": { "type": "string", "description": "What the person appears to be doing, in at most 12 words." },
                "notable": { "type": "string", "description": "Anything unusual worth a look, such as an error, in at most 12 words; empty if nothing." } } }),
        )
    };
    let seen = crate::ai::ask_about_image(model, &image, question, schema)?;
    let activity = seen["activity"].as_str().or_else(|| seen["scene"].as_str()).unwrap_or("something");
    let summary = if from_camera {
        let person = match seen["person"].as_str().unwrap_or("unsure") {
            "yes" => "a person is visible",
            "no" => "no person is visible",
            _ => "it is hard to tell whether a person is visible",
        };
        match seen["notable"].as_str().filter(|n| !n.trim().is_empty()) {
            Some(notable) => format!("{person}; {activity} — {notable}"),
            None => format!("{person}; {activity}"),
        }
    } else {
        match seen["notable"].as_str().filter(|n| !n.trim().is_empty()) {
            Some(notable) => format!("{activity} — {notable}"),
            None => activity.to_string(),
        }
    };
    Ok(json!({
        "summary": summary,
        "app_kind": if from_camera { "camera" } else { seen["app_kind"].as_str().unwrap_or("other") },
        "source": source,
        "model": model,
        "secs": started.elapsed().as_secs_f64(),
        "restore_token": new_token,
    }))
}

/// The hour of the day on the wall clock, 0.0..24.0. His sleep follows the user's day.
/// The hour of the day where the user is (0..24). BUDDY_HOUR fixes it, for tests whose
/// outcome would otherwise depend on whether they run at night, when he sleeps.
fn local_hour() -> f64 {
    use chrono::Timelike;
    if let Some(hour) = std::env::var("BUDDY_HOUR").ok().and_then(|h| h.parse::<f64>().ok()) {
        return hour.rem_euclid(24.0);
    }
    let t = chrono::Local::now();
    f64::from(t.hour()) + f64::from(t.minute()) / 60.0
}

fn listen() -> Result<UnixListener> {
    let path = paths::socket_path();
    let dir = path.parent().context("socket path has no parent")?;
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    if UnixStream::connect(&path).is_ok() {
        bail!("Buddy is already running (socket {} is live)", path.display());
    }
    let _ = fs::remove_file(&path); // stale socket from an unclean exit
    UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))
}

fn helper_reply(output: std::process::Output, number: usize, what: &str) -> Result<String, String> {
    let text = String::from_utf8_lossy(&output.stdout);
    let reply: Value = serde_json::from_str(text.trim()).map_err(|error| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        format!("step {number} sent no valid answer ({error}): {} {}", text.trim(), stderr.trim())
    })?;
    if reply["ok"] != true || reply["verified"] != true {
        return Err(format!("step {number} ({what}) failed: {}", reply["why"].as_str().unwrap_or("no semantic change was verified")));
    }
    Ok(format!("step {number}: {what}"))
}

/// Run one accessible step. Text travels through stdin so it is not visible in the
/// process list. Nothing here sends keys, mouse coordinates or clipboard data.
fn run_action_step(step: &WorkflowStep, number: usize) -> Result<String, String> {
    match step {
        WorkflowStep::Activate { app, frame, role, name } => {
            let script = perception_helper("a11y_act.py").ok_or("perception/a11y_act.py is not installed")?;
            let output = std::process::Command::new("python3")
                .arg(&script)
                .args(["--app", app, "--frame", frame, "--role", role, "--name", name])
                .output()
                .map_err(|error| format!("step {number} could not start: {error}"))?;
            helper_reply(output, number, &format!("activated {role} '{name}'"))
        }
        WorkflowStep::TypeText { app, frame, role, name, text } => {
            let script = perception_helper("a11y_text.py").ok_or("perception/a11y_text.py is not installed")?;
            let mut child = std::process::Command::new("python3")
                .arg(&script)
                .args(["--app", app, "--frame", frame, "--role", role, "--name", name])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|error| format!("step {number} could not start: {error}"))?;
            let mut stdin = child.stdin.take().ok_or_else(|| format!("step {number}: the text helper has no input"))?;
            stdin.write_all(text.as_bytes()).map_err(|error| format!("step {number}: the text could not be passed: {error}"))?;
            drop(stdin);
            let output = child.wait_with_output().map_err(|error| format!("step {number} could not finish: {error}"))?;
            helper_reply(output, number, &format!("entered text in {role} '{}'", name.trim()))
        }
        WorkflowStep::OpenApp { .. } | WorkflowStep::Manual { .. } => Err(format!("step {number} is not an accessible action")),
    }
}

fn accept_loop(listener: UnixListener, tx: Sender<Incoming>) {
    for (id, stream) in (1u64..).zip(listener.incoming().flatten()) {
        let Ok(reader) = stream.try_clone() else {
            continue;
        };
        if tx.send(Incoming::Connected(id, stream)).is_err() {
            return;
        }
        let tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(reader).lines() {
                let Ok(line) = line else { break };
                let msg = match serde_json::from_str::<ClientMsg>(&line) {
                    Ok(m) => Incoming::Message(id, m),
                    Err(e) => Incoming::Malformed(id, e.to_string()),
                };
                if tx.send(msg).is_err() {
                    return;
                }
            }
            let _ = tx.send(Incoming::Closed(id));
        });
    }
}

struct Replay {
    requester: u64,
    workflow_id: u64,
    args: Value,
    state: ReplayState,
    /// An accessible helper is running for the current step.
    busy: bool,
    /// The current step is Manual and waits for `workflow_continue`.
    manual: bool,
    /// The step index for which we already tried to open the expected application.
    opened_for: Option<usize>,
}

struct Daemon {
    store: Store,
    sim: Sim,
    clients: HashMap<u64, Client>,
    renderer: Option<Renderer>,
    /// Draws his pictures with a real coat while the user is away (none with --no-renderer).
    baker: Option<Baker>,
    reporting: HashMap<String, Reporting>,
    tx: Sender<Incoming>,
    helpers: [Helper; 2],
    looking: bool,
    /// A one-shot camera look has paused the presence helper so the device is free.
    camera_paused: bool,
    /// When he was last spoken to by name; for a little while after, he takes what he hears as meant for him.
    addressed_at: f64,
    /// What he may do on the computer, what is waiting for a yes, and the way back from the last change.
    permissions: Permissions,
    waiting: Vec<assist::Waiting>,
    current_app: Option<String>,
    replay: Option<Replay>,
    next_id: u64,
    last_undo: Option<(String, Undo)>,
    last_result: Option<String>,
    quit: bool,
}

impl Daemon {
    /// Deals with what a tick or command produced: journal it, log it, or pass it to the renderers.
    fn absorb(&mut self, outputs: Vec<Output>, at: f64) {
        for output in outputs {
            let stored = match output {
                Output::Journal(entry) => self.store.journal(at as i64, entry.kind, &entry.data),
                Output::Behaved(record) => self.store.log_behaviour(&record),
                Output::Look { at, secs } => {
                    self.broadcast(&ServerMsg::Event { name: "look".into(), args: json!({ "at": at, "secs": secs }) });
                    Ok(())
                }
                Output::Offer { habit, app, standing } => {
                    self.offer(habit, app, standing, at);
                    Ok(())
                }
                Output::Remember(text) => self.store.remember(at as i64, &text, language::language(&text), "told").map(|_| ()),
                Output::Vocalize { kind, intensity } => {
                    self.broadcast(&ServerMsg::Event { name: "vocalize".into(), args: json!({ "kind": kind, "intensity": intensity }) });
                    Ok(())
                }
            };
            if let Err(err) = stored {
                eprintln!("buddy: could not record: {err:#}");
            }
        }
    }

    fn persist(&self, at: f64) -> Result<()> {
        let t = at as i64;
        self.store.save("identity", &self.sim.identity, t)?;
        self.store.save("life", &self.sim.life, t)?;
        self.store.save("settings", &self.sim.settings, t)?;
        self.store.save("body", &self.sim.body(), t)?;
        self.store.save("mind", &self.sim.mind(), t)?;
        self.store.save("permissions", &self.permissions, t)?;
        self.store.backup_daily(t)?;
        Ok(())
    }

    fn send(&mut self, id: u64, msg: &ServerMsg) {
        let Some(client) = self.clients.get_mut(&id) else {
            return;
        };
        let line = serde_json::to_string(msg).unwrap_or_default();
        if writeln!(client.stream, "{line}").is_err() {
            self.clients.remove(&id);
        }
    }

    fn broadcast(&mut self, msg: &ServerMsg) {
        let renderers: Vec<u64> = self.clients.iter().filter(|(_, c)| c.is_renderer).map(|(id, _)| *id).collect();
        for id in renderers {
            self.send(id, msg);
        }
    }

    /// The name he answers to. The configured name is primary; the English and Persian
    /// forms of "Buddy" are aliases only while that is actually his name.
    fn names(&self) -> Vec<String> {
        let configured = self.sim.identity.name.clone();
        let normal = language::normalise(&configured);
        let mut names = vec![configured];
        if normal == "buddy" || normal == "بادی" {
            for alias in ["Buddy", "بادی"] {
                if !names.iter().any(|name| language::normalise(name) == language::normalise(alias)) {
                    names.push(alias.to_string());
                }
            }
        }
        names
    }

    /// Something was said to him (typed, for now). His own vocabulary and what he has been
    /// taught are tried first; only what they cannot place goes to the local model.
    fn said(&mut self, requester: u64, text: &str, at: f64) {
        let taught = self.sim.training.known_cues(&TRICKS);
        let names = self.names();
        let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let heard = language::understand(text, &name_refs, &taught);
        // When he is waiting for a yes or a no, that is what a yes or a no means.
        if self.heard_as_answer(requester, &heard, at) {
            return;
        }
        if heard != Heard::Unknown || text.split_whitespace().count() < 2 {
            self.heard(requester, text, heard, "his own vocabulary", at);
            return;
        }
        let (model, said, tx) = (self.sim.settings.language_model.clone(), text.to_string(), self.tx.clone());
        thread::spawn(move || {
            let outcome = crate::ai::interpret(&model, &said, &TRICKS, &taught).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Incoming::Understood { requester, said, outcome });
        });
    }

    fn heard(&mut self, requester: u64, said: &str, heard: Heard, by: &str, at: f64) {
        if self.heard_as_answer(requester, &heard, at) {
            return;
        }
        if let Heard::Task { request, skill, argument } = &heard {
            // He gives you his attention, and the request goes to be worked out and, if allowed, done.
            let outputs = self.sim.hear(heard.clone(), at);
            self.absorb(outputs, at);
            let _ = self.store.journal(at as i64, "spoken_to", &json!({ "language": language::language(said), "understood_as": "task", "by": by }));
            self.task(requester, request, skill.clone(), argument.clone(), at);
            return;
        }
        let meaning = serde_json::to_value(&heard).unwrap_or(Value::Null);
        let outputs = self.sim.hear(heard, at);
        self.absorb(outputs, at);
        // The journal keeps what kind of thing was said, not the words: talk is not memory unless he is told to remember it.
        let _ =
            self.store.journal(at as i64, "spoken_to", &json!({ "language": language::language(said), "understood_as": meaning["kind"], "by": by }));
        let _ = self.persist(at);
        let why = self.sim.explain(at);
        let reply = json!({ "understood_as": meaning, "by": by, "now": why["activity"], "because": why["because"],
            "puzzled": self.sim.snapshot(at).confusion > 0.5, "did_as_asked": why["doing_as_asked"] });
        self.send(requester, &ServerMsg::Reply { ok: true, data: reply, error: None });
    }

    /// Has him look once: at the screen (through the desktop portal), or at an image file the user names.
    /// The work happens on its own thread; the answer comes back as `Incoming::Looked`.
    fn start_look(&mut self, requester: u64, args: &Value) {
        let file = args["file"].as_str().map(PathBuf::from);
        let camera = args["camera"].as_bool().unwrap_or(false);
        let refusal = if self.looking {
            Some("he is already looking")
        } else if file.is_some() && camera {
            Some("choose either an image file or the camera, not both")
        } else if camera && self.sim.settings.privacy_mode {
            Some("privacy mode is on")
        } else if camera && !self.sim.settings.senses.camera {
            Some("the camera sense is off (turn it on with: buddy sense camera on)")
        } else if file.is_none() && !camera && self.sim.settings.privacy_mode {
            Some("privacy mode is on")
        } else if file.is_none() && !camera && !self.sim.settings.senses.screen {
            Some("the screen sense is off (turn it on with: buddy sense screen on)")
        } else {
            None
        };
        if let Some(why) = refusal {
            self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some(why.to_string()) });
            return;
        }
        if camera {
            // The presence helper and the one-shot capture must not fight over the device.
            self.camera_paused = true;
            let tx = self.tx.clone();
            if let Some(helper) = self.helpers.iter_mut().find(|h| h.source == "camera") {
                helper.poll(false, &tx);
            }
        }
        self.looking = true;
        let model = self.sim.settings.vision_model.clone();
        let token: Option<String> = self.store.load("screen_token").ok().flatten();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let outcome = look(&model, file, camera, token).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Incoming::Looked { requester, outcome });
        });
    }

    /// What perception providers may observe right now.
    fn policy(&self) -> ServerMsg {
        let senses = &self.sim.settings.senses;
        let open = !self.sim.settings.privacy_mode;
        ServerMsg::Policy { desktop: open && senses.desktop, window_titles: open && senses.desktop && senses.window_titles }
    }

    fn announce_policy(&mut self) {
        let policy = self.policy();
        let providers: Vec<u64> = self.clients.iter().filter(|(_, c)| c.is_sense).map(|(id, _)| *id).collect();
        for id in providers {
            self.send(id, &policy);
        }
    }

    /// Something a provider observed. Dropped here, before anything is done with it,
    /// if that sense is switched off or privacy mode is on.
    fn start_replay(&mut self, requester: u64, mut args: Value, at: f64) {
        if self.replay.is_some() {
            let _ =
                self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some("a workflow is already being replayed".into()) });
            return;
        }
        let workflow = if let Some(id) = args["id"].as_u64() {
            self.sim.workflows().iter().find(|workflow| workflow.id == id).cloned()
        } else if let Some(name) = args["workflow"].as_str() {
            self.sim.workflows().iter().find(|workflow| workflow.name.eq_ignore_ascii_case(name)).cloned()
        } else {
            None
        };
        let Some(workflow) = workflow else {
            let _ = self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some("no such workflow".into()) });
            return;
        };
        if workflow.never {
            let _ = self.send(
                requester,
                &ServerMsg::Reply { ok: false, data: Value::Null, error: Some("you told him not to help with this workflow".into()) },
            );
            return;
        }
        let steps = if workflow.steps.is_empty() {
            workflow.apps.iter().cloned().map(|app| WorkflowStep::OpenApp { app }).collect()
        } else {
            workflow.steps.clone()
        };
        if steps.is_empty() {
            let _ = self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some("the workflow has no steps".into()) });
            return;
        }
        if Workflows::needs_accessibility(&workflow) && (self.sim.settings.privacy_mode || !self.sim.settings.senses.accessibility) {
            let _ = self.send(
                requester,
                &ServerMsg::Reply {
                    ok: false,
                    data: Value::Null,
                    error: Some("turn on the accessibility sense first: buddy sense accessibility on".into()),
                },
            );
            return;
        }
        if args["workflow"].as_str().is_none() {
            args["workflow"] = json!(workflow.name);
        }
        let state = ReplayState::new(steps, at);
        self.replay = Some(Replay { requester, workflow_id: workflow.id, args, state, busy: false, manual: false, opened_for: None });
        self.advance_replay(at);
    }

    fn open_replay_app(&mut self, at: f64) {
        let Some(app) = self.replay.as_ref().and_then(|replay| replay.state.expected_app()).map(str::to_string) else {
            self.fail_replay("the workflow has no next application".into(), at);
            return;
        };
        if self.current_app.as_deref() == Some(app.as_str()) {
            self.advance_replay(at);
            return;
        }
        let notes = paths::data_dir().join("notes.md");
        if let Err(why) = crate::skills::run("open_app", &json!({ "name": app }), &self.permissions, &notes) {
            self.fail_replay(format!("could not open '{app}': {why:#}"), at);
        }
    }

    fn advance_replay(&mut self, at: f64) {
        let Some(mut replay) = self.replay.take() else { return };
        if replay.busy {
            self.replay = Some(replay);
            return;
        }
        if replay.manual {
            if at - replay.state.started > MANUAL_STEP_TIMEOUT {
                self.replay = Some(replay);
                self.fail_replay(format!("the manual step timed out after {MANUAL_STEP_TIMEOUT:.0} s"), at);
            } else {
                self.replay = Some(replay);
            }
            return;
        }
        if replay.state.timed_out(at, REPLAY_STEP_TIMEOUT) {
            let step = replay.state.index + 1;
            let app = replay.state.expected_app().unwrap_or("?").to_string();
            self.replay = Some(replay);
            self.fail_replay(format!("step {step} in '{app}' did not come to the front within {REPLAY_STEP_TIMEOUT:.0} s"), at);
            return;
        }
        let Some(step) = replay.state.step().cloned() else {
            let _ = self.sim.workflow_outcome(replay.workflow_id, true, at);
            let summary = format!("replayed the workflow ({} steps)", replay.state.steps.len());
            self.did(replay.requester, "replay_workflow", &replay.args, None, Ok((summary, String::new(), None)), at);
            return;
        };
        let expected = step.app().to_string();
        if self.current_app.as_deref() != Some(expected.as_str()) {
            if replay.opened_for != Some(replay.state.index) {
                replay.opened_for = Some(replay.state.index);
                self.replay = Some(replay);
                self.open_replay_app(at);
            } else {
                self.replay = Some(replay);
            }
            return;
        }
        replay.opened_for = None;
        match step {
            WorkflowStep::OpenApp { .. } => {
                replay.state.advance(at);
                self.replay = Some(replay);
                self.advance_replay(at);
            }
            WorkflowStep::Activate { .. } | WorkflowStep::TypeText { .. } => {
                if self.sim.settings.privacy_mode || !self.sim.settings.senses.accessibility {
                    self.replay = Some(replay);
                    self.fail_replay("the accessibility sense was turned off during the replay".into(), at);
                    return;
                }
                let index = replay.state.index;
                let workflow_id = replay.workflow_id;
                replay.busy = true;
                self.replay = Some(replay);
                let tx = self.tx.clone();
                thread::spawn(move || {
                    let outcome = run_action_step(&step, index + 1);
                    let _ = tx.send(Incoming::StepReplay { workflow_id, step_index: index, outcome });
                });
            }
            WorkflowStep::Manual { app, note } => {
                let step_number = replay.state.index + 1;
                let total = replay.state.steps.len();
                let workflow_name = self.sim.workflows().iter().find(|w| w.id == replay.workflow_id).map(|w| w.name.clone()).unwrap_or_default();
                replay.manual = true;
                let requester = std::mem::replace(&mut replay.requester, 0);
                self.replay = Some(replay);
                if requester != 0 {
                    let _ = self.send(
                        requester,
                        &ServerMsg::Reply {
                            ok: true,
                            data: json!({
                                "waiting_for_manual": {
                                    "workflow": workflow_name,
                                    "step": step_number,
                                    "total": total,
                                    "app": app,
                                    "note": note,
                                }
                            }),
                            error: None,
                        },
                    );
                }
            }
        }
    }

    fn continue_replay(&mut self, requester: u64, at: f64) {
        let Some(mut replay) = self.replay.take() else {
            let _ = self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some("no workflow is being replayed".into()) });
            return;
        };
        if !replay.manual {
            self.replay = Some(replay);
            let _ = self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some("no manual step is waiting".into()) });
            return;
        }
        let Some(step) = replay.state.step().cloned() else {
            self.replay = Some(replay);
            let _ = self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some("the workflow has no manual step".into()) });
            return;
        };
        let expected = step.app().to_string();
        if self.current_app.as_deref() != Some(expected.as_str()) {
            self.replay = Some(replay);
            let _ = self.send(
                requester,
                &ServerMsg::Reply { ok: false, data: Value::Null, error: Some(format!("focus '{expected}' before continuing the workflow")) },
            );
            return;
        }
        replay.manual = false;
        replay.requester = requester;
        replay.opened_for = None;
        replay.state.advance(at);
        self.replay = Some(replay);
        self.advance_replay(at);
    }

    fn fail_replay(&mut self, why: String, at: f64) {
        let Some(replay) = self.replay.take() else { return };
        let _ = self.sim.workflow_outcome(replay.workflow_id, false, at);
        self.did(replay.requester, "replay_workflow", &replay.args, None, Err(why), at);
    }

    fn workflow_capture_step(&mut self) -> Result<Value> {
        if self.sim.settings.privacy_mode || !self.sim.settings.senses.accessibility {
            bail!("turn on the accessibility sense first: buddy sense accessibility on");
        }
        let script = perception_helper("a11y_focus.py").context("perception/a11y_focus.py is not installed")?;
        let output = std::process::Command::new("python3").arg(&script).output().context("the accessibility helper could not start")?;
        let text = String::from_utf8_lossy(&output.stdout);
        let reply: Value = serde_json::from_str(text.trim()).with_context(|| format!("the accessibility helper said: {}", text.trim()))?;
        if reply["ok"] != true {
            bail!(
                "{} (if this application exposes no accessibility tree, use: buddy workflow manual-step \"what to do\" --app APP)",
                reply["why"].as_str().unwrap_or("the focused control could not be read")
            );
        }
        let step: WorkflowStep = serde_json::from_value(reply["step"].clone()).context("the accessibility helper sent an invalid step")?;
        self.sim.capture_action_step(step.clone())?;
        Ok(json!({ "captured": step, "steps": self.sim.action_recording_len() }))
    }

    fn workflow_type_step(&mut self, text: &str) -> Result<Value> {
        if self.sim.settings.privacy_mode || !self.sim.settings.senses.accessibility {
            bail!("turn on the accessibility sense first: buddy sense accessibility on");
        }
        if text.is_empty() {
            bail!("the text to enter is empty");
        }
        if text.chars().count() > 2_000 {
            bail!("the text to enter is longer than 2000 characters");
        }
        let script = perception_helper("a11y_focus.py").context("perception/a11y_focus.py is not installed")?;
        let output = std::process::Command::new("python3").arg(&script).arg("--text").output().context("the accessibility helper could not start")?;
        let raw = String::from_utf8_lossy(&output.stdout);
        let reply: Value = serde_json::from_str(raw.trim()).with_context(|| format!("the accessibility helper said: {}", raw.trim()))?;
        if reply["ok"] != true {
            bail!(
                "{} (if this application exposes no accessibility tree, use: buddy workflow manual-step \"what to do\" --app APP)",
                reply["why"].as_str().unwrap_or("the focused text field could not be read")
            );
        }
        let app = reply["step"]["app"].as_str().context("the text step has no application")?.to_string();
        let frame = reply["step"]["frame"].as_str().context("the text step has no frame")?.to_string();
        let role = reply["step"]["role"].as_str().context("the text step has no role")?.to_string();
        let name = reply["step"]["name"].as_str().unwrap_or_default().to_string();
        if role == "password text" {
            bail!("password fields are not allowed");
        }
        let step = WorkflowStep::TypeText { app, frame, role, name, text: text.to_string() };
        self.sim.capture_action_step(step.clone())?;
        Ok(json!({ "captured": step, "steps": self.sim.action_recording_len() }))
    }

    fn workflow_manual_step(&mut self, args: &Value) -> Result<Value> {
        if self.sim.settings.privacy_mode {
            bail!("turn privacy mode off before recording a manual step");
        }
        let note = args["note"].as_str().context("manual-step needs a note describing what to do")?.trim().to_string();
        if note.is_empty() {
            bail!("the manual step needs a note");
        }
        if note.chars().count() > 240 {
            bail!("the note is longer than 240 characters");
        }
        let app = args["app"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| self.current_app.clone())
            .context("focus the target application, or pass --app with its application id")?;
        let step = WorkflowStep::Manual { app, note };
        self.sim.capture_action_step(step.clone())?;
        Ok(json!({ "captured": step, "steps": self.sim.action_recording_len() }))
    }

    fn perceive(&mut self, provider: &str, kind: &str, data: &Value, at: f64) {
        let senses = &self.sim.settings.senses;
        // A stretch of speech from the microphone: have it written down locally, then delete the recording.
        if kind == "utterance" {
            let Some(path) = data["path"].as_str().map(PathBuf::from) else { return };
            if self.sim.settings.privacy_mode || !senses.microphone {
                let _ = fs::remove_file(path);
                return;
            }
            self.noted("microphone", at);
            let (model, tx) = (self.sim.settings.audio_model.clone(), self.tx.clone());
            thread::spawn(move || {
                let outcome =
                    fs::read(&path).map_err(anyhow::Error::from).and_then(|wav| crate::ai::transcribe(&model, &wav)).map_err(|e| format!("{e:#}"));
                let _ = fs::remove_file(&path);
                let _ = tx.send(Incoming::Transcribed(outcome));
            });
            return;
        }
        if self.sim.settings.privacy_mode {
            return;
        }
        // The pointer is not something he reacts to as an event; the renderer just follows it with his eyes.
        if kind == "pointer" {
            if senses.desktop {
                self.broadcast(&ServerMsg::Event { name: "pointer".into(), args: data.clone() });
                if std::env::var_os("BUDDY_TRACE_SENSES").is_some() {
                    eprintln!("sense {provider} pointer {data}");
                }
                self.noted("desktop", at);
            }
            return;
        }
        let Some(stimulus) = Stimulus::parse(kind, data) else { return };
        if let Stimulus::Focus { app } = &stimulus {
            self.current_app = Some(app.clone());
        }
        let source = stimulus.source();
        if senses.get(source) != Some(true) {
            return;
        }
        if std::env::var_os("BUDDY_TRACE_SENSES").is_some() {
            eprintln!("sense {provider} {kind} {data}");
        }
        if self.noted(source, at) {
            let _ = self.store.journal(at as i64, "sense_active", &json!({ "source": source, "provider": provider }));
        }
        let outputs = self.sim.sense(stimulus, at);
        self.absorb(outputs, at);
    }

    /// Counts a report from a sense; true if it is the first this session.
    fn noted(&mut self, source: &str, at: f64) -> bool {
        let entry = self.reporting.entry(source.to_string()).or_default();
        entry.reports += 1;
        entry.last = at;
        entry.reports == 1
    }

    fn senses(&self, at: f64) -> Value {
        let describe = |source: &str| match source {
            "system" => "how long since you touched the computer, screen lock, system load",
            "desktop" => "which application is in front, fullscreen, pointer position, that a notification arrived",
            "window_titles" => "the titles of windows",
            "screen" => "what is drawn on the screen, looked at locally",
            "camera" => "whether someone is in front of the camera",
            "microphone" => "sounds and speech",
            "accessibility" => "the focused control's accessibility information, for guided workflow capture",
            _ => "",
        };
        let list: Vec<Value> = buddy_core::perception::SOURCES
            .iter()
            .map(|source| {
                let trouble =
                    self.helpers.iter().find(|h| h.source == *source).and_then(|h| h.trouble.as_ref()).map(|why| format!("not available: {why}"));
                let reporting = trouble.or_else(|| {
                    self.reporting.get(*source).map(|r| format!("{} reports this session, the last {:.0} s ago", r.reports, at - r.last))
                });
                json!({ "source": source, "allowed": self.sim.settings.senses.get(source), "what": describe(source), "reporting": reporting })
            })
            .collect();
        json!({ "privacy_mode": self.sim.settings.privacy_mode, "senses": list })
    }

    fn status(&mut self, at: f64) -> Result<Value> {
        let renderer = match self.renderer.as_mut() {
            Some(r) => r.status(),
            None => "disabled (--no-renderer)",
        };
        let active: Vec<&str> = self.reporting.iter().filter(|(_, r)| at - r.last < 60.0).map(|(source, _)| source.as_str()).collect();
        let observing = if self.sim.settings.privacy_mode {
            "nothing (privacy mode)".to_string()
        } else if active.is_empty() {
            "nothing is reporting".to_string()
        } else {
            active.join(", ")
        };
        Ok(json!({
            "state": self.sim.snapshot(at),
            "life": self.sim.life,
            "feeling": self.sim.explain(at)["emotions"],
            "needs": self.sim.needs,
            "settings": self.sim.settings,
            "renderer": renderer,
            "pictures": self.baker.as_ref().map_or("not drawn (--no-renderer)", |b| b.note()),
            "renderers_connected": self.clients.values().filter(|c| c.is_renderer).count(),
            "observing": observing,
            "database": paths::db_path(),
            "schema_version": self.store.schema_version()?,
        }))
    }

    fn command(&mut self, name: &str, args: &Value, at: f64) -> Result<Value> {
        if let Some(answered) = self.assist_command(name, args, at) {
            return answered;
        }
        let cmd = match name {
            "status" => return self.status(at),
            "why" => return Ok(self.sim.explain(at)),
            "snapshot" => {
                // Only his own window is saved, by the renderer itself: nothing else on the screen.
                let path = args["path"].as_str().context("snapshot needs a file name")?.to_string();
                if !self.clients.values().any(|c| c.is_renderer) {
                    bail!("his window is not open (he is hidden, or the renderer is not running)");
                }
                self.broadcast(&ServerMsg::Event { name: "snapshot".into(), args: json!({ "path": path }) });
                return Ok(json!({ "did": format!("saved what his window shows to {path}, and what he was doing to {path}.txt") }));
            }
            "knows" => {
                let known: Vec<Value> = self
                    .sim
                    .training
                    .known(&TRICKS)
                    .into_iter()
                    .map(|(cue, trick, p)| json!({ "cue": cue, "trick": trick, "strength": p.strength, "source": p.source, "rewarded": p.rewarded, "corrected": p.corrected }))
                    .collect();
                return Ok(json!(known));
            }
            "memories" => return Ok(json!(self.store.memories()?)),
            "forget" => {
                let id = args["id"].as_i64().context("forget needs the number of a memory (see: buddy memories)")?;
                if !self.store.forget_memory(id)? {
                    bail!("there is no memory number {id}");
                }
                return Ok(Value::Null);
            }
            "unlearn" => {
                let cue = language::normalise(args["cue"].as_str().context("unlearn needs a word")?);
                if !self.sim.training.forget(&cue) {
                    bail!("he has learned nothing about \u{201c}{cue}\u{201d}");
                }
                self.persist(at)?;
                return Ok(Value::Null);
            }
            "teach" => {
                let (cue, trick) = (args["cue"].as_str().context("teach needs a word")?, args["trick"].as_str().context("teach needs a trick")?);
                if !TRICKS.contains(&trick) {
                    bail!("he has no trick called '{trick}' (he has: {})", TRICKS.join(", "));
                }
                let outputs = self.sim.hear(Heard::Teach { cue: cue.to_string(), trick: trick.to_string() }, at);
                self.absorb(outputs, at);
                self.persist(at)?;
                return Ok(Value::Null);
            }
            "treat" => {
                let outputs = self.sim.hear(Heard::Treat, at);
                self.absorb(outputs, at);
                return Ok(Value::Null);
            }
            "senses" => return Ok(self.senses(at)),
            "export" => return self.store.export(at as i64),
            "journal" => {
                return Ok(Value::Array(self.store.recent_journal(args["limit"].as_u64().unwrap_or(20) as usize)?));
            }
            "quit" => {
                self.quit = true;
                return Ok(Value::Null);
            }
            "hide" => Command::Hide { minutes: args["minutes"].as_f64() },
            "show" => Command::Show,
            "sleep" => Command::Sleep,
            "wake" => Command::Wake,
            "privacy" => Command::Privacy(args["on"].as_bool().context("privacy needs {\"on\": true|false}")?),
            "pet" => Command::Pet,
            "sense" => {
                let source = args["source"].as_str().context("sense needs a source")?;
                if !buddy_core::perception::SOURCES.contains(&source) {
                    bail!("unknown sense '{source}' ({})", buddy_core::perception::SOURCES.join(", "));
                }
                Command::Sense { source: source.to_string(), on: args["on"].as_bool().context("sense needs {\"on\": true|false}")? }
            }
            "praise" => Command::Praise,
            "feed" => Command::Feed,
            "water" => Command::Water,
            "quiet" => Command::Quiet(args["on"].as_bool().context("quiet needs {\"on\": true|false}")?),
            "drag" => Command::DragTo { x_frac: args["x_frac"].as_f64().context("drag needs x_frac")? },
            "drag_end" => Command::DragEnd,
            "workflows" => return Ok(json!({ "workflows": self.sim.workflows() })),
            "workflow" => {
                let found = if let Some(id) = args["id"].as_u64() {
                    self.sim.workflows().iter().find(|workflow| workflow.id == id)
                } else if let Some(name) = args["name"].as_str() {
                    self.sim.workflows().iter().find(|workflow| workflow.name.eq_ignore_ascii_case(name))
                } else {
                    bail!("workflow needs an id or a name")
                };
                return found.cloned().map(|workflow| json!(workflow)).context("no such workflow");
            }
            "workflow_record" => {
                let action = args["action"].as_str().context("workflow_record needs an action")?;
                match action {
                    "start" => {
                        let name = args["name"].as_str().context("workflow_record start needs a name")?;
                        self.sim.start_workflow_recording(name.to_string(), at)?;
                        let _ = self.store.journal(at as i64, "workflow_recording", &json!({ "name": name }));
                        return Ok(json!({ "recording": name }));
                    }
                    "stop" => {
                        let workflow = if self.sim.has_action_recording() {
                            self.sim.stop_action_recording(at)?
                        } else {
                            self.sim.stop_workflow_recording(at)?
                        };
                        self.persist(at)?;
                        let _ = self.store.journal(
                            at as i64,
                            "workflow_recorded",
                            &json!({ "id": workflow.id, "name": workflow.name, "apps": workflow.apps }),
                        );
                        return Ok(json!(workflow));
                    }
                    "cancel" => {
                        let cancelled = self.sim.cancel_workflow_recording() | self.sim.cancel_action_recording();
                        if cancelled {
                            let _ = self.store.journal(at as i64, "workflow_cancelled", &Value::Null);
                        }
                        return Ok(json!({ "cancelled": cancelled }));
                    }
                    other => bail!("workflow_record action is start, stop or cancel, not {other:?}"),
                }
            }
            "workflow_capture" => {
                let action = args["action"].as_str().context("workflow_capture needs an action")?;
                match action {
                    "start" => {
                        if self.sim.settings.privacy_mode {
                            bail!("turn privacy mode off before capturing a workflow");
                        }
                        let name = args["name"].as_str().context("workflow_capture start needs a name")?;
                        self.sim.start_action_recording(name.to_string(), at)?;
                        let _ = self.store.journal(at as i64, "workflow_capture", &json!({ "name": name }));
                        return Ok(json!({ "capturing": name }));
                    }
                    "stop" => {
                        let workflow = self.sim.stop_action_recording(at)?;
                        self.persist(at)?;
                        let _ = self.store.journal(
                            at as i64,
                            "workflow_recorded",
                            &json!({ "id": workflow.id, "name": workflow.name, "steps": workflow.steps }),
                        );
                        return Ok(json!(workflow));
                    }
                    "cancel" => return Ok(json!({ "cancelled": self.sim.cancel_action_recording() })),
                    other => bail!("workflow_capture action is start, stop or cancel, not {other:?}"),
                }
            }
            "workflow_capture_step" => return self.workflow_capture_step(),
            "workflow_abort" => {
                let replay = self.replay.take();
                let replayed = replay.is_some();
                if let Some(replay) = replay {
                    let _ = self.sim.workflow_outcome(replay.workflow_id, false, at);
                }
                let recorded = self.sim.cancel_workflow_recording() | self.sim.cancel_action_recording();
                if replayed || recorded {
                    let _ = self.store.journal(at as i64, "workflow_aborted", &json!({ "replay": replayed, "recording": recorded }));
                    return Ok(json!({ "aborted": replayed, "cancelled_recording": recorded }));
                }
                bail!("nothing is being replayed or recorded");
            }
            "workflow_type_step" => {
                let text = args["text"].as_str().context("workflow_type_step needs text")?;
                return self.workflow_type_step(text);
            }
            "workflow_manual_step" => return self.workflow_manual_step(args),
            "workflow_delete" => {
                let id = if let Some(id) = args["id"].as_u64() {
                    id
                } else if let Some(name) = args["name"].as_str() {
                    self.sim
                        .workflows()
                        .iter()
                        .find(|workflow| workflow.name.eq_ignore_ascii_case(name))
                        .map(|workflow| workflow.id)
                        .context("no such workflow")?
                } else {
                    bail!("workflow_delete needs an id or a name")
                };
                let deleted = self.sim.delete_workflow(id);
                if deleted {
                    self.persist(at)?;
                    let _ = self.store.journal(at as i64, "workflow_deleted", &json!({ "id": id }));
                }
                return Ok(json!({ "deleted": deleted }));
            }
            "set" => {
                let value = args["value"].as_str().context("set needs a value")?;
                let number = || value.parse::<f64>().map_err(|_| anyhow!("'{value}' is not a number"));
                match args["key"].as_str() {
                    Some("name") => {
                        let name = value.trim();
                        if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
                            bail!("a name needs 1–60 printable characters");
                        }
                        Command::Rename(name.to_string())
                    }
                    Some("size_px") => Command::SetSize(number()?),
                    Some("adult_days") => Command::SetAdultDays(number()?),
                    Some("stay_in_fullscreen") => Command::StayInFullscreen(matches!(value, "on" | "true" | "yes")),
                    Some("vision_model" | "language_model" | "audio_model") if crate::ai::is_cloud_model(value) => {
                        bail!("cloud models are not used: Buddy runs a local model")
                    }
                    Some("vision_model") => Command::VisionModel(value.to_string()),
                    Some("language_model") => Command::LanguageModel(value.to_string()),
                    Some("audio_model") => Command::AudioModel(value.to_string()),
                    Some("pictures") => {
                        if !matches!(value, "away" | "always" | "off") {
                            bail!("pictures can be: away (draw only while you are away), always, off");
                        }
                        self.sim.settings.pictures = value.to_string();
                        self.persist(at)?;
                        return Ok(Value::Null);
                    }
                    other => bail!(
                        "unknown setting {other:?} (name, size_px, adult_days, stay_in_fullscreen, vision_model, language_model, audio_model, pictures)"
                    ),
                }
            }
            other => bail!("unknown command '{other}'"),
        };
        // Dragging arrives many times a second; everything else is worth saving at once.
        let save_now = !matches!(cmd, Command::DragTo { .. });
        let changes_policy = matches!(cmd, Command::Sense { .. } | Command::Privacy(_));
        let outputs = self.sim.apply(cmd, at);
        self.absorb(outputs, at);
        if save_now {
            self.persist(at)?;
        }
        if changes_policy {
            self.announce_policy();
        }
        Ok(Value::Null)
    }

    fn handle(&mut self, incoming: Incoming, at: f64) {
        match incoming {
            Incoming::Connected(id, stream) => {
                self.clients.insert(id, Client { stream, is_renderer: false, is_sense: false });
            }
            Incoming::Closed(id) => {
                self.clients.remove(&id);
            }
            Incoming::Transcribed(outcome) => {
                let Ok(words) = outcome else { return };
                if words.is_empty() {
                    return;
                }
                // Talk in the room is not for him unless his name is in it, or he was spoken to moments ago.
                let names = self.names();
                let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
                let named = language::addresses(&words, &name_refs);
                let addressed = named || at - self.addressed_at < 20.0;
                if std::env::var_os("BUDDY_TRACE_SENSES").is_some() {
                    eprintln!("heard speech ({} words), {}", words.split_whitespace().count(), if addressed { "for him" } else { "not for him" });
                }
                if addressed {
                    self.addressed_at = at;
                    self.said(0, &words, at);
                }
            }
            Incoming::Understood { requester, said, outcome } => {
                // If the model could not help, he is left as puzzled as he would have been without it.
                let (heard, by) = match outcome {
                    Ok(heard) => (heard, "the local language model"),
                    Err(_) => (Heard::Unknown, "nothing: his vocabulary did not cover it and no local model answered"),
                };
                self.heard(requester, &said, heard, by, at);
            }
            Incoming::Planned { requester, outcome } => match outcome {
                Ok(Some((skill, args))) => self.propose(requester, &skill, args, None, "the local language model", at),
                other => {
                    let outputs = self.sim.task_done(false, at);
                    self.absorb(outputs, at);
                    let why = match other {
                        Err(why) => format!("he could not work out what to do: {why}"),
                        _ => "none of the things he can do fits that".to_string(),
                    };
                    self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some(why) });
                }
            },
            Incoming::StepReplay { workflow_id, step_index, outcome } => {
                let Some(mut replay) = self.replay.take() else { return };
                if replay.workflow_id != workflow_id || replay.state.index != step_index {
                    // A late answer from a step that has already been abandoned.
                    self.replay = Some(replay);
                    return;
                }
                replay.busy = false;
                self.replay = Some(replay);
                match outcome {
                    Ok(_) => {
                        let Some(mut replay) = self.replay.take() else { return };
                        replay.state.advance(at);
                        self.replay = Some(replay);
                        self.advance_replay(at);
                    }
                    Err(why) => self.fail_replay(why, at),
                }
            }
            Incoming::Did { requester, skill, args, habit, outcome } => self.did(requester, &skill, &args, habit, outcome, at),
            Incoming::Unavailable { source, why } => {
                if let Some(helper) = self.helpers.iter_mut().find(|h| h.source == source) {
                    helper.trouble = Some(why);
                }
            }
            Incoming::Looked { requester, outcome } => {
                self.looking = false;
                self.camera_paused = false;
                let reply = match outcome {
                    Ok(seen) => {
                        if let Some(token) = seen["restore_token"].as_str().filter(|t| !t.is_empty()) {
                            let _ = self.store.save("screen_token", &token, at as i64);
                        }
                        // The journal records that he looked and roughly at what; the description itself is not stored.
                        let _ = self.store.journal(at as i64, "looked", &json!({ "at": seen["source"], "model": seen["model"], "kind": seen["app_kind"], "kept": "a one-line summary, in memory only" }));
                        self.noted("screen", at);
                        let outputs = self.sim.sense(Stimulus::Glimpse { summary: seen["summary"].as_str().unwrap_or_default().to_string() }, at);
                        self.absorb(outputs, at);
                        ServerMsg::Reply { ok: true, data: seen, error: None }
                    }
                    Err(error) => ServerMsg::Reply { ok: false, data: Value::Null, error: Some(error) },
                };
                self.send(requester, &reply);
            }
            Incoming::Malformed(id, error) => self.send(id, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some(error) }),
            Incoming::Message(id, msg) => match msg {
                ClientMsg::Hello { role } => {
                    if let Some(client) = self.clients.get_mut(&id) {
                        client.is_renderer = role == "renderer";
                        client.is_sense = role == "sense";
                    }
                    if role == "sense" {
                        let policy = self.policy();
                        self.send(id, &policy);
                    }
                    let state = ServerMsg::State(self.sim.snapshot(at));
                    if role == "renderer" {
                        self.send(id, &state);
                    }
                }
                ClientMsg::Screen { walk_px } => {
                    self.sim.apply(Command::SetScreen { walk_px }, at);
                }
                ClientMsg::Sense { source, kind, data } => self.perceive(&source, &kind, &data, at),
                ClientMsg::Command { name, args } if name == "look" => self.start_look(id, &args),
                ClientMsg::Command { name, args } if name == "approve" || name == "deny" => {
                    self.answer(id, args["id"].as_u64(), name == "approve", args["lasting"] == true, at)
                }
                ClientMsg::Command { name, args } if name == "replay_workflow" => self.propose(id, "replay_workflow", args, None, "you", at),
                ClientMsg::Command { name, args: _ } if name == "workflow_continue" => self.continue_replay(id, at),
                ClientMsg::Command { name, args } if name == "say" => self.said(id, args["text"].as_str().unwrap_or_default(), at),
                ClientMsg::Command { name, args } => {
                    let reply = match self.command(&name, &args, at) {
                        Ok(data) => ServerMsg::Reply { ok: true, data, error: None },
                        Err(e) => ServerMsg::Reply { ok: false, data: Value::Null, error: Some(format!("{e:#}")) },
                    };
                    // The renderer fires commands without waiting for answers; only the CLI reads replies.
                    if self.clients.get(&id).is_some_and(|c| !c.is_renderer) {
                        self.send(id, &reply);
                    }
                }
            },
        }
    }
}

pub fn run(no_renderer: bool) -> Result<()> {
    let listener = listen()?;
    let store = Store::open(&paths::db_path())?;
    let start = now();

    let (identity, newborn) = match store.load::<Identity>("identity")? {
        Some(identity) => (identity, false),
        None => (Identity { name: "Buddy".into(), born_at: start as i64 }, true),
    };
    let mut life: Life = store.load("life")?.unwrap_or_default();
    let settings: Settings = store.load("settings")?.unwrap_or_default();
    let body: Body = store.load("body")?.unwrap_or_default();
    let mind: Mind = store.load("mind")?.unwrap_or_default();
    let permissions: Permissions = store.load("permissions")?.unwrap_or_default();
    let remembered = store.recent_behaviour(400)?;
    if newborn {
        store.journal(start as i64, "born", &json!({ "name": identity.name }))?;
    }
    if let Some(note) = &store.recovery_note {
        eprintln!("buddy: {note}");
        store.journal(start as i64, "recovered", &json!({ "note": note }))?;
    }
    let away_secs = if life.last_seen > 0 { start as i64 - life.last_seen } else { 0 };
    life.sessions += 1;
    store.journal(start as i64, "session_start", &json!({ "away_secs": away_secs, "session": life.sessions }))?;

    let (tx, rx) = mpsc::channel();
    let seed = SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
    let mut daemon = Daemon {
        store,
        sim: Sim::new(identity, life, settings, body, mind, seed),
        clients: HashMap::new(),
        renderer: (!no_renderer).then(Renderer::new),
        baker: (!no_renderer).then(Baker::new),
        reporting: HashMap::new(),
        tx: tx.clone(),
        helpers: [
            Helper::new("camera", "camera_watch.py", Vec::new()),
            // Speech is written, briefly, next to the socket: a directory only this user can read.
            Helper::new(
                "microphone",
                "mic_listen.py",
                vec!["--utterances".into(), paths::socket_path().with_file_name("").to_string_lossy().into_owned()],
            ),
        ],
        looking: false,
        camera_paused: false,
        addressed_at: f64::NEG_INFINITY,
        permissions,
        waiting: Vec::new(),
        current_app: None,
        replay: None,
        next_id: 0,
        last_undo: None,
        last_result: None,
        quit: false,
    };
    daemon.sim.remember(remembered.iter().map(|r| (r.ts + r.secs, r.activity, r.variant)));
    daemon.sim.returned_after(away_secs as f64);
    daemon.persist(start)?;

    let terminate = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&terminate))?;
    }
    let system_tx = tx.clone();
    thread::spawn(move || accept_loop(listener, tx));
    // The system sense has no client connection of its own; its reports come in as client 0.
    thread::spawn(move || {
        crate::system::watch(|kind, data| {
            system_tx.send(Incoming::Message(0, ClientMsg::Sense { source: "system".into(), kind: kind.into(), data })).is_ok()
        })
    });

    let mut last_state: Option<StateMsg> = None;
    let mut last_push = Instant::now();
    let mut last_save = Instant::now();
    while !daemon.quit && !terminate.load(Ordering::Relaxed) {
        let wait = if daemon.sim.is_moving() { TICK_MOVING } else { TICK_RESTING };
        match rx.recv_timeout(wait) {
            Ok(first) => {
                daemon.handle(first, now());
                while let Ok(more) = rx.try_recv() {
                    daemon.handle(more, now());
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let at = now();
        let outputs = daemon.sim.tick(at, local_hour());
        daemon.absorb(outputs, at);
        daemon.advance_replay(at);

        let state = daemon.sim.snapshot(at);
        if last_state.as_ref() != Some(&state) && (daemon.sim.is_moving() || last_push.elapsed() >= TICK_RESTING) {
            daemon.broadcast(&ServerMsg::State(state.clone()));
            last_state = Some(state);
            last_push = Instant::now();
        }
        if last_save.elapsed() >= SAVE_EVERY {
            if let Err(e) = daemon.persist(at) {
                eprintln!("buddy: save failed: {e:#}");
            }
            last_save = Instant::now();
        }
        daemon.let_lapse(at);
        // The camera and microphone are open only while their sense is allowed and privacy mode is off.
        let open = !daemon.sim.settings.privacy_mode;
        let wanted = [open && daemon.sim.settings.senses.camera && !daemon.camera_paused, open && daemon.sim.settings.senses.microphone];
        let tx = daemon.tx.clone();
        for (helper, wanted) in daemon.helpers.iter_mut().zip(wanted) {
            helper.poll(wanted, &tx);
        }
        // While he is hidden the renderer is not running at all, so hiding frees the GPU entirely.
        let visible = daemon.sim.settings.hidden_until.is_none();
        if let Some(renderer) = daemon.renderer.as_mut() {
            renderer.poll(visible);
        }
        if let Some(baker) = daemon.baker.as_mut() {
            let state = daemon.sim.snapshot(at);
            baker.poll(state.growth, daemon.sim.settings.size_px, !state.user_here, &daemon.sim.settings.pictures);
        }
    }

    let end = now();
    let tx = daemon.tx.clone();
    for helper in &mut daemon.helpers {
        helper.poll(false, &tx);
    }
    if let Some(renderer) = daemon.renderer.as_mut() {
        renderer.stop();
    }
    if let Some(baker) = daemon.baker.as_mut() {
        baker.stop();
    }
    daemon.store.journal(end as i64, "session_end", &json!({ "secs": end - start }))?;
    daemon.persist(end)?;
    let _ = fs::remove_file(paths::socket_path());
    Ok(())
}
