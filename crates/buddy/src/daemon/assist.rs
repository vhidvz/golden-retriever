//! Doing things on the computer for the user: working out what was asked,
//! getting a yes where one is needed, carrying it out, and remembering how to
//! take it back. Also his own offers of help, which go through the same gate.

use super::{Daemon, Incoming};
use crate::skills::{self, Policy, Risk, Undo};
use anyhow::{Context, Result, bail};
use buddy_core::{Answer, Heard, ServerMsg, paths};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread;

/// Seconds an offer of his own waits for an answer, and a request the user made.
const OFFER_LAPSES: f64 = 90.0;
const REQUEST_LAPSES: f64 = 600.0;

/// Something he would like to do and is waiting for a yes to.
pub(super) struct Waiting {
    pub id: u64,
    pub skill: String,
    pub args: Value,
    /// Set when this was his own idea, from one of the user's habits.
    pub habit: Option<String>,
    pub since: f64,
}

fn notes() -> PathBuf {
    paths::data_dir().join("notes.md")
}

/// Turn the JSON `args` object (or a JSON string) into helper values. Values are
/// strings and are passed as separate argv entries, never through a shell.
pub(super) fn helper_values(args: &Value) -> BTreeMap<String, String> {
    let source = match &args["args"] {
        Value::String(text) => serde_json::from_str(text).ok(),
        other => Some(other.clone()),
    };
    source
        .and_then(|value| value.as_object().cloned())
        .map(|object| object.into_iter().filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string()))).collect())
        .unwrap_or_default()
}

fn reply(ok: bool, data: Value, error: Option<String>) -> ServerMsg {
    ServerMsg::Reply { ok, data, error }
}

impl Daemon {
    /// A request for something on the computer. If his own vocabulary already worked
    /// out what and with what, that is used; otherwise the local model chooses a skill.
    pub(super) fn task(&mut self, requester: u64, request: &str, skill: Option<String>, argument: Option<String>, at: f64) {
        if let (Some(skill), Some(argument)) = (skill, argument) {
            let main = skills::skill(&skill).map_or("query", |s| s.arguments[0]);
            self.propose(requester, &skill, json!({ main: argument }), None, "his own vocabulary", at);
            return;
        }
        let (model, request, tx) = (self.sim.settings.language_model.clone(), request.to_string(), self.tx.clone());
        thread::spawn(move || {
            let outcome = crate::ai::plan(&model, &request).map_err(|e| format!("{e:#}"));
            let _ = tx.send(Incoming::Planned { requester, outcome });
        });
    }

    /// He knows what he would do. Whether he may is the user's to say: by policy, or by answering now.
    pub(super) fn propose(&mut self, requester: u64, skill: &str, args: Value, habit: Option<String>, by: &str, at: f64) {
        let would = skills::describe(skill, &args);
        let approved_before = skill == "run_command" && args["command"].as_str().is_some_and(|c| self.permissions.commands.iter().any(|ok| ok == c));
        match self.permissions.policy(skill) {
            Policy::Deny => {
                let outputs = self.sim.task_done(false, at);
                self.absorb(outputs, at);
                self.send(
                    requester,
                    &reply(false, Value::Null, Some(format!("you have told him not to {would} (change it with: buddy skill {skill} ask)"))),
                );
            }
            Policy::Allow => self.carry_out(requester, skill, args, habit, at),
            Policy::Ask if approved_before => self.carry_out(requester, skill, args, habit, at),
            Policy::Ask => {
                self.next_id += 1;
                let id = self.next_id;
                if habit.is_none() {
                    // (When the idea was his own he is already doing his best to catch your eye.)
                    let outputs = self.sim.await_answer(at);
                    self.absorb(outputs, at);
                }
                self.waiting.push(Waiting { id, skill: skill.to_string(), args, habit, since: at });
                self.send(requester, &reply(true, json!({ "waiting_for_yes": id, "would": would, "worked_out_by": by }), None));
            }
        }
    }

    /// Runs a skill on a thread of its own; the outcome comes back as `Incoming::Did`.
    pub(super) fn carry_out(&mut self, requester: u64, skill: &str, args: Value, habit: Option<String>, at: f64) {
        if skill == "run_helper" {
            let helper_name = args["helper"].as_str().unwrap_or_default().to_string();
            let values = helper_values(&args);
            let permissions = self.permissions.clone();
            let tx = self.tx.clone();
            let args_for_reply = args.clone();
            thread::spawn(move || {
                let outcome = crate::helpers::find(&crate::helpers::dir(), &helper_name)
                    .and_then(|helper| crate::helpers::run(&helper, &values, &permissions))
                    .map(|(summary, detail)| (summary, detail, None))
                    .map_err(|error| format!("{error:#}"));
                let _ = tx.send(Incoming::Did { requester, skill: "run_helper".into(), args: args_for_reply, habit: None, outcome });
            });
            return;
        }
        if skill == "replay_workflow" {
            self.start_replay(requester, args, at);
            return;
        }
        let (permissions, tx, skill) = (self.permissions.clone(), self.tx.clone(), skill.to_string());
        thread::spawn(move || {
            let outcome = skills::run(&skill, &args, &permissions, &notes()).map(|d| (d.summary, d.detail, d.undo));
            let _ = tx.send(Incoming::Did { requester, skill, args, habit, outcome: outcome.map_err(|e| format!("{e:#}")) });
        });
    }

    /// A skill finished. Record what was done (not its output), keep the way back, and tell whoever asked.
    pub(super) fn did(
        &mut self,
        requester: u64,
        skill: &str,
        args: &Value,
        habit: Option<String>,
        outcome: Result<(String, String, Option<Undo>), String>,
        at: f64,
    ) {
        let would = skills::describe(skill, args);
        let (worked, summary) = match &outcome {
            Ok((summary, _, _)) => (true, summary.clone()),
            Err(why) => (false, why.clone()),
        };
        let undoable = matches!(&outcome, Ok((_, _, Some(_))));
        let _ = self.store.journal(
            at as i64,
            "did",
            &json!({ "skill": skill, "what": would, "worked": worked, "outcome": summary, "his_idea": habit.is_some(), "can_be_undone": undoable }),
        );
        let outputs = self.sim.task_done(worked, at);
        self.absorb(outputs, at);
        match outcome {
            Ok((summary, detail, undo)) => {
                if let Some(undo) = undo {
                    self.last_undo = Some((summary.clone(), undo));
                }
                self.last_result = Some(format!("{summary}\n{detail}").trim().to_string());
                let shown: String = detail.chars().take(1500).collect();
                self.send(
                    requester,
                    &reply(true, json!({ "did": summary, "detail": shown, "more": detail.chars().count() > 1500, "can_be_undone": undoable }), None),
                );
            }
            Err(why) => self.send(requester, &reply(false, Value::Null, Some(why))),
        }
    }

    /// The user's answer to whatever is waiting: the one named, or the most recent.
    /// `lasting` makes a yes standing ("always") or a no final ("don't help me with this").
    pub(super) fn answer(&mut self, requester: u64, id: Option<u64>, yes: bool, lasting: bool, at: f64) {
        let position = match id {
            Some(id) => self.waiting.iter().position(|w| w.id == id),
            None => self.waiting.len().checked_sub(1),
        };
        let Some(position) = position else {
            self.send(requester, &reply(false, Value::Null, Some("nothing is waiting for an answer".into())));
            return;
        };
        let waiting = self.waiting.remove(position);
        if let Some(habit) = &waiting.habit {
            let answer = match (yes, lasting) {
                (true, true) => Answer::Always,
                (true, false) => Answer::Yes,
                (false, true) => Answer::Never,
                (false, false) => Answer::No,
            };
            let outputs = self.sim.offer_answered(Some(habit), answer, at);
            self.absorb(outputs, at);
        } else if lasting {
            // A standing answer about a kind of request. It can widen what he may do only as far as
            // the skill's own limits allow: sensitive skills keep asking, except a command approved word for word.
            let risk = skills::skill(&waiting.skill).map(|s| s.risk);
            if !yes {
                let _ = self.permissions.set_policy(&waiting.skill, Policy::Deny);
            } else if waiting.skill == "run_command" {
                self.permissions.commands.extend(waiting.args["command"].as_str().map(str::to_string));
            } else if risk != Some(Risk::Sensitive) {
                let _ = self.permissions.set_policy(&waiting.skill, Policy::Allow);
            }
        }
        let _ = self.persist(at);
        if yes {
            self.carry_out(requester, &waiting.skill, waiting.args, waiting.habit, at);
        } else {
            let _ =
                self.store.journal(at as i64, "declined", &json!({ "what": skills::describe(&waiting.skill, &waiting.args), "for_good": lasting }));
            self.send(requester, &reply(true, json!({ "declined": skills::describe(&waiting.skill, &waiting.args), "for_good": lasting }), None));
        }
    }

    /// If something is waiting, a yes or a no said to him is the answer to it. True if it was.
    pub(super) fn heard_as_answer(&mut self, requester: u64, heard: &Heard, at: f64) -> bool {
        if self.waiting.is_empty() {
            return false;
        }
        let (yes, lasting) = match heard {
            Heard::Yes | Heard::Approval => (true, false),
            Heard::Always => (true, true),
            // "Don't help me with this" settles it for good when the idea was his; a request you made yourself is just dropped.
            Heard::NoHelp => (false, self.waiting.last().is_some_and(|w| w.habit.is_some())),
            Heard::NotNow | Heard::Correction | Heard::Stop => (false, false),
            _ => return false,
        };
        self.answer(requester, None, yes, lasting, at);
        true
    }

    /// His own idea: one of the user's habits has begun, and he could do the next step.
    pub(super) fn offer(&mut self, habit: String, app: String, standing: bool, at: f64) {
        let args = json!({ "name": app });
        if standing && self.permissions.policy("open_app") != Policy::Deny {
            // "Always do that" was said about this habit: he goes ahead.
            self.carry_out(0, "open_app", args, Some(habit), at);
        } else {
            self.next_id += 1;
            self.waiting.push(Waiting { id: self.next_id, skill: "open_app".into(), args, habit: Some(habit), since: at });
        }
    }

    /// Offers and requests do not wait for ever. An offer nobody answered is simply let go.
    pub(super) fn let_lapse(&mut self, at: f64) {
        let lapsed: Vec<Option<String>> = self
            .waiting
            .iter()
            .filter(|w| at - w.since > if w.habit.is_some() { OFFER_LAPSES } else { REQUEST_LAPSES })
            .map(|w| w.habit.clone())
            .collect();
        if lapsed.is_empty() {
            return;
        }
        self.waiting.retain(|w| at - w.since <= if w.habit.is_some() { OFFER_LAPSES } else { REQUEST_LAPSES });
        for habit in lapsed.into_iter().flatten() {
            let outputs = self.sim.offer_answered(Some(&habit), Answer::Ignored, at);
            self.absorb(outputs, at);
        }
    }

    /// The commands about skills that answer at once. `None` means "not one of mine".
    pub(super) fn assist_command(&mut self, name: &str, args: &Value, at: f64) -> Option<Result<Value>> {
        Some(match name {
            "waiting" => Ok(json!(self
                .waiting
                .iter()
                .map(|w| json!({ "id": w.id, "would": skills::describe(&w.skill, &w.args), "his_idea": w.habit.is_some(), "for_secs": at - w.since }))
                .collect::<Vec<_>>())),
            "skills" => Ok(json!(skills::SKILLS
                .iter()
                .map(|s| json!({ "name": s.name, "what": s.what, "policy": self.permissions.policy(s.name),
                    "risk": match s.risk { Risk::Looks => "only looks", Risk::Changes => "changes things", Risk::Sensitive => "sensitive: always asks" } }))
                .collect::<Vec<_>>())),
            "skill" => (|| {
                let policy = match args["policy"].as_str() {
                    Some("ask") => Policy::Ask,
                    Some("allow") => Policy::Allow,
                    Some("deny") => Policy::Deny,
                    _ => bail!("the policy is one of: ask, allow, deny"),
                };
                self.permissions.set_policy(args["name"].as_str().context("which skill?")?, policy)?;
                self.persist(at)?;
                Ok(Value::Null)
            })(),
            "folders" => (|| {
                if let Some(add) = args["add"].as_str() {
                    let folder = PathBuf::from(add).canonicalize().with_context(|| format!("there is no folder {add}"))?;
                    if !folder.is_dir() {
                        bail!("{} is not a folder", folder.display());
                    }
                    if !self.permissions.folders.contains(&folder) {
                        self.permissions.folders.push(folder);
                    }
                } else if let Some(remove) = args["remove"].as_str() {
                    let folder = PathBuf::from(remove).canonicalize().unwrap_or_else(|_| PathBuf::from(remove));
                    self.permissions.folders.retain(|f| *f != folder);
                }
                self.persist(at)?;
                Ok(json!({ "folders": self.permissions.folders, "commands": self.permissions.commands }))
            })(),
            "undo" => (|| {
                let (what, undo) = self.last_undo.take().context("there is nothing he can take back")?;
                let how = skills::undo(&undo)?;
                self.store.journal(at as i64, "undid", &json!({ "what": what, "how": how }))?;
                Ok(json!({ "undid": what, "how": how }))
            })(),
            "result" => self.last_result.clone().map(|r| json!(r)).context("he has not done anything for you yet"),
            "providers" => {
                let endpoint = crate::ai::endpoint();
                let version = crate::ai::version().ok();
                let language = self.sim.settings.language_model.clone();
                let vision = self.sim.settings.vision_model.clone();
                let audio = self.sim.settings.audio_model.clone();
                let caps = |model: &str| crate::ai::capabilities(model).unwrap_or_default();
                let local = match crate::ai::models() {
                    Ok(models) => json!({
                        "available": true,
                        "endpoint": endpoint,
                        "version": version,
                        "models": models,
                        "understands_language_with": language,
                        "looks_at_images_with": vision,
                        "understands_audio_with": audio,
                        "capabilities": {
                            "language": caps(&language),
                            "vision": caps(&vision),
                            "audio": caps(&audio),
                        },
                    }),
                    Err(why) => json!({ "available": false, "endpoint": endpoint, "version": version, "why": format!("{why:#}") }),
                };
                Ok(json!({ "local": local, "order": "his own vocabulary first, then the local model" }))
            }
            _ => return None,
        })
    }
}
