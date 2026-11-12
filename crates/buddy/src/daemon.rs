//! The long-running process: owns Buddy's state, advances it, persists it, and
//! serves the renderer and the CLI over one Unix socket.

use crate::renderer::Renderer;
use anyhow::{Context, Result, anyhow, bail};
use buddy_core::{
    Body, ClientMsg, Command, Identity, JournalEntry, Life, ServerMsg, Settings, Sim, StateMsg,
    Store, paths,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
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
}

struct Client {
    stream: UnixStream,
    is_renderer: bool,
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64())
}

fn listen() -> Result<UnixListener> {
    let path = paths::socket_path();
    let dir = path.parent().context("socket path has no parent")?;
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    if UnixStream::connect(&path).is_ok() {
        bail!(
            "Buddy is already running (socket {} is live)",
            path.display()
        );
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
    quit: bool,
}

impl Daemon {
    fn record(&self, entries: Vec<JournalEntry>, at: f64) {
        for e in entries {
            if let Err(err) = self.store.journal(at as i64, e.kind, &e.data) {
                eprintln!("buddy: journal write failed: {err:#}");
            }
        }
    }

    fn persist(&self, at: f64) -> Result<()> {
        let t = at as i64;
        self.store.save("identity", &self.sim.identity, t)?;
        self.store.save("life", &self.sim.life, t)?;
        self.store.save("settings", &self.sim.settings, t)?;
        self.store.save("body", &self.sim.body(), t)?;
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

    fn broadcast(&mut self, state: &StateMsg) {
        let msg = ServerMsg::State(state.clone());
        let renderers: Vec<u64> = self
            .clients
            .iter()
            .filter(|(_, c)| c.is_renderer)
            .map(|(id, _)| *id)
            .collect();
        for id in renderers {
            self.send(id, &msg);
        }
    }

    fn status(&mut self, at: f64) -> Result<Value> {
        let renderer = match self.renderer.as_mut() {
            Some(r) => r.status(),
            None => "disabled (--no-renderer)",
        };
        let observing = if self.sim.settings.privacy_mode {
            "nothing (privacy mode)"
        } else {
            "nothing — camera, microphone and screen perception are not built yet"
        };
        Ok(json!({
            "state": self.sim.snapshot(at),
            "life": self.sim.life,
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
            "export" => return self.store.export(at as i64),
            "journal" => {
                return Ok(Value::Array(
                    self.store
                        .recent_journal(args["limit"].as_u64().unwrap_or(20) as usize)?,
                ));
            }
            "quit" => {
                self.quit = true;
                return Ok(Value::Null);
            }
            "hide" => Command::Hide {
                minutes: args["minutes"].as_f64(),
            },
            "show" => Command::Show,
            "sleep" => Command::Sleep,
            "wake" => Command::Wake,
            "privacy" => Command::Privacy(
                args["on"]
                    .as_bool()
                    .context("privacy needs {\"on\": true|false}")?,
            ),
            "pet" => Command::Pet,
            "drag" => Command::DragTo {
                x_frac: args["x_frac"].as_f64().context("drag needs x_frac")?,
            },
            "drag_end" => Command::DragEnd,
            "set" => {
                let value = args["value"].as_str().context("set needs a value")?;
                let number = || {
                    value
                        .parse::<f64>()
                        .map_err(|_| anyhow!("'{value}' is not a number"))
                };
                match args["key"].as_str() {
                    Some("name") => Command::Rename(value.to_string()),
                    Some("size_px") => Command::SetSize(number()?),
                    Some("adult_days") => Command::SetAdultDays(number()?),
                    other => bail!("unknown setting {other:?} (name, size_px, adult_days)"),
                }
            }
            other => bail!("unknown command '{other}'"),
        };
        // Dragging arrives many times a second; everything else is worth saving at once.
        let save_now = !matches!(cmd, Command::DragTo { .. });
        let entries = self.sim.apply(cmd, at);
        self.record(entries, at);
        if save_now {
            self.persist(at)?;
        }
        Ok(Value::Null)
    }

    fn handle(&mut self, incoming: Incoming, at: f64) {
        match incoming {
            Incoming::Connected(id, stream) => {
                self.clients.insert(
                    id,
                    Client {
                        stream,
                        is_renderer: false,
                    },
                );
            }
            Incoming::Closed(id) => {
                self.clients.remove(&id);
            }
            Incoming::Malformed(id, error) => self.send(
                id,
                &ServerMsg::Reply {
                    ok: false,
                    data: Value::Null,
                    error: Some(error),
                },
            ),
            Incoming::Message(id, msg) => match msg {
                ClientMsg::Hello { role } => {
                    if let Some(client) = self.clients.get_mut(&id) {
                        client.is_renderer = role == "renderer";
                    }
                    let state = ServerMsg::State(self.sim.snapshot(at));
                    if role == "renderer" {
                        self.send(id, &state);
                    }
                }
                ClientMsg::Screen { walk_px } => {
                    self.sim.apply(Command::SetScreen { walk_px }, at);
                }
                ClientMsg::Command { name, args } => {
                    let reply = match self.command(&name, &args, at) {
                        Ok(data) => ServerMsg::Reply {
                            ok: true,
                            data,
                            error: None,
                        },
                        Err(e) => ServerMsg::Reply {
                            ok: false,
                            data: Value::Null,
                            error: Some(format!("{e:#}")),
                        },
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
        None => (
            Identity {
                name: "Buddy".into(),
                born_at: start as i64,
            },
            true,
        ),
    };
    let mut life: Life = store.load("life")?.unwrap_or_default();
    let settings: Settings = store.load("settings")?.unwrap_or_default();
    let body: Body = store.load("body")?.unwrap_or_default();
    if newborn {
        store.journal(start as i64, "born", &json!({ "name": identity.name }))?;
    }
    if let Some(note) = &store.recovery_note {
        eprintln!("buddy: {note}");
        store.journal(start as i64, "recovered", &json!({ "note": note }))?;
    }
    let away_secs = if life.last_seen > 0 {
        start as i64 - life.last_seen
    } else {
        0
    };
    life.sessions += 1;
    store.journal(
        start as i64,
        "session_start",
        &json!({ "away_secs": away_secs, "session": life.sessions }),
    )?;

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64);
    let mut daemon = Daemon {
        store,
        sim: Sim::new(identity, life, settings, body, seed),
        clients: HashMap::new(),
        renderer: (!no_renderer).then(Renderer::new),
        quit: false,
    };
    daemon.persist(start)?;

    let terminate = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(signal, Arc::clone(&terminate))?;
    }
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || accept_loop(listener, tx));

    let mut last_state: Option<StateMsg> = None;
    let mut last_push = Instant::now();
    let mut last_save = Instant::now();
    while !daemon.quit && !terminate.load(Ordering::Relaxed) {
        let wait = if daemon.sim.is_moving() {
            TICK_MOVING
        } else {
            TICK_RESTING
        };
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
        let entries = daemon.sim.tick(at);
        daemon.record(entries, at);

        let state = daemon.sim.snapshot(at);
        if last_state.as_ref() != Some(&state)
            && (daemon.sim.is_moving() || last_push.elapsed() >= TICK_RESTING)
        {
            daemon.broadcast(&state);
            last_state = Some(state);
            last_push = Instant::now();
        }
        if last_save.elapsed() >= SAVE_EVERY {
            if let Err(e) = daemon.persist(at) {
                eprintln!("buddy: save failed: {e:#}");
            }
            last_save = Instant::now();
        }
        // While he is hidden the renderer is not running at all, so hiding frees the GPU entirely.
        let visible = daemon.sim.settings.hidden_until.is_none();
        if let Some(renderer) = daemon.renderer.as_mut() {
            renderer.poll(visible);
        }
    }

    let end = now();
    if let Some(renderer) = daemon.renderer.as_mut() {
        renderer.stop();
    }
    daemon
        .store
        .journal(end as i64, "session_end", &json!({ "secs": end - start }))?;
    daemon.persist(end)?;
    let _ = fs::remove_file(paths::socket_path());
    Ok(())
}
