//! Short routines the user works through, recorded as application steps and replayed
//! with a check that each application really came to the front.
//!
//! This is deliberately application-level. Inside-application keystrokes and clicks
//! need a permissioned accessibility adapter and are a separate milestone.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

const MIN_DWELL_SECS: f64 = 2.0;
const MIN_STEPS: usize = 2;
const MAX_STEPS: usize = 8;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    pub apps: Vec<String>,
    pub created_at: f64,
    #[serde(default)]
    pub last_run: Option<f64>,
    #[serde(default)]
    pub runs: u32,
    #[serde(default)]
    pub successes: u32,
    #[serde(default)]
    pub failures: u32,
    #[serde(default)]
    pub standing_yes: bool,
    #[serde(default)]
    pub never: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Workflows {
    pub items: Vec<Workflow>,
    next_id: u64,
}

impl Workflows {
    pub fn record(&mut self, name: &str, apps: &[String], now: f64) -> Result<Workflow> {
        let name = name.trim();
        if name.is_empty() {
            bail!("a workflow needs a name");
        }
        let mut clean: Vec<String> = Vec::new();
        for app in apps.iter().map(|app| app.trim()).filter(|app| !app.is_empty()) {
            if clean.last().map(String::as_str) != Some(app) {
                clean.push(app.to_string());
            }
        }
        if clean.len() < MIN_STEPS {
            bail!("a workflow needs at least {MIN_STEPS} application steps");
        }
        if clean.len() > MAX_STEPS {
            bail!("a workflow can have at most {MAX_STEPS} application steps");
        }
        self.next_id += 1;
        let workflow = Workflow {
            id: self.next_id,
            name: name.to_string(),
            apps: clean,
            created_at: now,
            last_run: None,
            runs: 0,
            successes: 0,
            failures: 0,
            standing_yes: false,
            never: false,
        };
        self.items.push(workflow.clone());
        Ok(workflow)
    }

    pub fn list(&self) -> &[Workflow] {
        &self.items
    }

    pub fn get(&self, id: u64) -> Option<&Workflow> {
        self.items.iter().find(|workflow| workflow.id == id)
    }

    pub fn find(&self, name: &str) -> Option<&Workflow> {
        self.items.iter().find(|workflow| workflow.name.eq_ignore_ascii_case(name))
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut Workflow> {
        self.items.iter_mut().find(|workflow| workflow.id == id)
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|workflow| workflow.id != id);
        self.items.len() != before
    }

    pub fn outcome(&mut self, id: u64, ok: bool, now: f64) {
        if let Some(workflow) = self.get_mut(id) {
            workflow.runs += 1;
            workflow.last_run = Some(now);
            if ok {
                workflow.successes += 1;
            } else {
                workflow.failures += 1;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recording {
    pub name: String,
    pub started_at: f64,
    apps: Vec<String>,
    pending: Option<(String, f64)>,
}

impl Recording {
    pub fn new(name: String, started_at: f64) -> Self {
        Self { name, started_at, apps: Vec::new(), pending: None }
    }

    pub fn observe(&mut self, app: &str, now: f64) {
        if let Some((previous, since)) = &self.pending {
            if previous == app {
                return;
            }
            if now - since >= MIN_DWELL_SECS {
                self.apps.push(previous.clone());
            }
        }
        self.pending = Some((app.to_string(), now));
    }

    pub fn finish(mut self, now: f64) -> Vec<String> {
        let _ = now;
        if let Some((pending, _)) = self.pending.take() {
            if self.apps.last() != Some(&pending) {
                self.apps.push(pending);
            }
        }
        self.apps
    }

    pub fn cancel(self) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayEvent {
    Waiting,
    StepOk,
    Done,
    TimedOut,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayState {
    pub apps: Vec<String>,
    pub index: usize,
    pub started: f64,
}

impl ReplayState {
    pub fn new(apps: Vec<String>, now: f64) -> Self {
        Self { apps, index: 0, started: now }
    }

    pub fn expected(&self) -> Option<&str> {
        self.apps.get(self.index).map(String::as_str)
    }

    pub fn check(&mut self, front: Option<&str>, now: f64, timeout: f64) -> ReplayEvent {
        if self.index >= self.apps.len() {
            return ReplayEvent::Done;
        }
        if now - self.started > timeout {
            return ReplayEvent::TimedOut;
        }
        if front == Some(self.apps[self.index].as_str()) {
            self.index += 1;
            self.started = now;
            if self.index >= self.apps.len() { ReplayEvent::Done } else { ReplayEvent::StepOk }
        } else {
            ReplayEvent::Waiting
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recording_keeps_the_apps_the_user_stayed_on() {
        let mut recording = Recording::new("morning".into(), 100.0);
        recording.observe("terminal", 100.0);
        recording.observe("editor", 101.0);
        recording.observe("browser", 130.0);
        assert_eq!(recording.finish(140.0), vec!["editor", "browser"]);
    }

    #[test]
    fn a_workflow_needs_a_name_and_at_least_two_apps() {
        let mut workflows = Workflows::default();
        assert!(workflows.record("", &["a".into(), "b".into()], 0.0).is_err());
        assert!(workflows.record("x", &["a".into()], 0.0).is_err());
        let saved = workflows.record("x", &["a".into(), "b".into()], 0.0).unwrap();
        assert_eq!(saved.id, 1);
        assert_eq!(workflows.list().len(), 1);
    }

    #[test]
    fn outcomes_are_counted_and_survive_json() {
        let mut workflows = Workflows::default();
        let id = workflows.record("x", &["a".into(), "b".into()], 0.0).unwrap().id;
        workflows.outcome(id, true, 10.0);
        workflows.outcome(id, false, 20.0);
        let restored: Workflows = serde_json::from_str(&serde_json::to_string(&workflows).unwrap()).unwrap();
        let saved = restored.get(id).unwrap();
        assert_eq!((saved.runs, saved.successes, saved.failures), (2, 1, 1));
        assert_eq!(saved.last_run, Some(20.0));
    }

    #[test]
    fn a_step_passes_only_when_the_expected_app_is_in_front() {
        let mut state = ReplayState::new(vec!["terminal".into(), "editor".into()], 0.0);
        assert_eq!(state.check(Some("browser"), 1.0, 20.0), ReplayEvent::Waiting);
        assert_eq!(state.check(Some("terminal"), 2.0, 20.0), ReplayEvent::StepOk);
        assert_eq!(state.expected(), Some("editor"));
        assert_eq!(state.check(Some("editor"), 3.0, 20.0), ReplayEvent::Done);
    }

    #[test]
    fn a_step_that_never_appears_times_out() {
        let mut state = ReplayState::new(vec!["terminal".into()], 0.0);
        assert_eq!(state.check(None, 5.0, 20.0), ReplayEvent::Waiting);
        assert_eq!(state.check(None, 21.0, 20.0), ReplayEvent::TimedOut);
    }
}
