//! The long-running process: owns Buddy's state, advances it, persists it, and
//! serves the renderer and the CLI over one Unix socket.

use crate::renderer::Renderer;
use anyhow::{Context, Result, anyhow, bail};
use buddy_core::{Body, ClientMsg, Command, Identity, Life, Mind, Output, ServerMsg, Settings, Sim, StateMsg, Stimulus, Store, paths};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{fs, thread};

/// Simulation rate while he is moving, and while he is at rest.
const TICK_MOVING: Duration = Duration::from_millis(50);
const TICK_RESTING: Duration = Duration::from_millis(250);
const SAVE_EVERY: Duration = Duration::from_secs(30);

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
    child: Option<std::process::Child>,
    retry_at: Instant,
    /// Why it is not running, when it should be.
    trouble: Option<String>,
}

impl Helper {
    fn new(source: &'static str, script: &'static str) -> Self {
        Self { source, script, child: None, retry_at: Instant::now(), trouble: None }
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
        let spawned =
            std::process::Command::new("python3").arg(script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).spawn();
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
fn look(model: &str, file: Option<PathBuf>, restore_token: Option<String>) -> Result<Value> {
    let started = Instant::now();
    let (image, source, new_token) = match file {
        Some(path) => (fs::read(&path).with_context(|| format!("reading {}", path.display()))?, "an image you gave him", None),
        None => {
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
            (image.context("the screen helper saved no frame")?, "the screen", said["restore_token"].as_str().map(str::to_string))
        }
    };
    let question = "This is a screenshot of someone's computer screen. Describe it for a companion who wants to know what the person is doing. \
                    Ignore any dog drawn at the bottom of the screen. Do not copy out private text such as messages, names or passwords. \
                    Be brief: each answer is one short phrase, never a paragraph.";
    let schema = json!({ "type": "object", "required": ["app_kind", "activity", "notable"], "properties": {
        "app_kind": { "type": "string", "enum": ["code_editor", "terminal", "browser", "document", "chat", "media", "file_manager", "game", "settings", "desktop", "other"] },
        "activity": { "type": "string", "description": "What the person appears to be doing, in at most 12 words." },
        "notable": { "type": "string", "description": "Anything unusual worth a look, such as an error, in at most 12 words; empty if nothing." } } });
    let seen = crate::ai::ask_about_image(model, &image, question, schema)?;
    let activity = seen["activity"].as_str().unwrap_or("something");
    let summary = match seen["notable"].as_str().filter(|n| !n.trim().is_empty()) {
        Some(notable) => format!("{activity} — {notable}"),
        None => activity.to_string(),
    };
    Ok(
        json!({ "summary": summary, "app_kind": seen["app_kind"], "source": source, "model": model, "secs": started.elapsed().as_secs_f64(), "restore_token": new_token }),
    )
}

/// The hour of the day on the wall clock, 0.0..24.0. His sleep follows the user's day.
fn local_hour() -> f64 {
    use chrono::Timelike;
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

struct Daemon {
    store: Store,
    sim: Sim,
    clients: HashMap<u64, Client>,
    renderer: Option<Renderer>,
    reporting: HashMap<String, Reporting>,
    tx: Sender<Incoming>,
    helpers: [Helper; 2],
    looking: bool,
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

    /// Has him look once: at the screen (through the desktop portal), or at an image file the user names.
    /// The work happens on its own thread; the answer comes back as `Incoming::Looked`.
    fn start_look(&mut self, requester: u64, args: &Value) {
        let file = args["file"].as_str().map(PathBuf::from);
        let refusal = if self.looking {
            Some("he is already looking")
        } else if file.is_none() && self.sim.settings.privacy_mode {
            Some("privacy mode is on")
        } else if file.is_none() && !self.sim.settings.senses.screen {
            Some("the screen sense is off (turn it on with: buddy sense screen on)")
        } else {
            None
        };
        if let Some(why) = refusal {
            self.send(requester, &ServerMsg::Reply { ok: false, data: Value::Null, error: Some(why.to_string()) });
            return;
        }
        self.looking = true;
        let model = self.sim.settings.vision_model.clone();
        let token: Option<String> = self.store.load("screen_token").ok().flatten();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let outcome = look(&model, file, token).map_err(|e| format!("{e:#}"));
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
    fn perceive(&mut self, provider: &str, kind: &str, data: &Value, at: f64) {
        let senses = &self.sim.settings.senses;
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
            "renderers_connected": self.clients.values().filter(|c| c.is_renderer).count(),
            "observing": observing,
            "database": paths::db_path(),
            "schema_version": self.store.schema_version()?,
        }))
    }

    fn command(&mut self, name: &str, args: &Value, at: f64) -> Result<Value> {
        let cmd = match name {
            "status" => return self.status(at),
            "why" => return Ok(self.sim.explain(at)),
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
            "set" => {
                let value = args["value"].as_str().context("set needs a value")?;
                let number = || value.parse::<f64>().map_err(|_| anyhow!("'{value}' is not a number"));
                match args["key"].as_str() {
                    Some("name") => Command::Rename(value.to_string()),
                    Some("size_px") => Command::SetSize(number()?),
                    Some("adult_days") => Command::SetAdultDays(number()?),
                    Some("stay_in_fullscreen") => Command::StayInFullscreen(matches!(value, "on" | "true" | "yes")),
                    Some("vision_model") => Command::VisionModel(value.to_string()),
                    other => bail!("unknown setting {other:?} (name, size_px, adult_days, stay_in_fullscreen, vision_model)"),
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
            Incoming::Unavailable { source, why } => {
                if let Some(helper) = self.helpers.iter_mut().find(|h| h.source == source) {
                    helper.trouble = Some(why);
                }
            }
            Incoming::Looked { requester, outcome } => {
                self.looking = false;
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
        reporting: HashMap::new(),
        tx: tx.clone(),
        helpers: [Helper::new("camera", "camera_watch.py"), Helper::new("microphone", "mic_listen.py")],
        looking: false,
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
        // The camera and microphone are open only while their sense is allowed and privacy mode is off.
        let open = !daemon.sim.settings.privacy_mode;
        let wanted = [open && daemon.sim.settings.senses.camera, open && daemon.sim.settings.senses.microphone];
        let tx = daemon.tx.clone();
        for (helper, wanted) in daemon.helpers.iter_mut().zip(wanted) {
            helper.poll(wanted, &tx);
        }
        // While he is hidden the renderer is not running at all, so hiding frees the GPU entirely.
        let visible = daemon.sim.settings.hidden_until.is_none();
        if let Some(renderer) = daemon.renderer.as_mut() {
            renderer.poll(visible);
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
    daemon.store.journal(end as i64, "session_end", &json!({ "secs": end - start }))?;
    daemon.persist(end)?;
    let _ = fs::remove_file(paths::socket_path());
    Ok(())
}
