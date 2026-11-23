//! Short routines the user works through, recorded as application steps and replayed
//! with a check that each application really came to the front.
//!
//! This is deliberately application-level. Inside-application keystrokes and clicks
//! need a permissioned accessibility adapter and are a separate milestone.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

const MIN_DWELL_SECS: f64 = 2.0;
const MAX_STEPS: usize = 8;
const MIN_APP_STEPS: usize = 2;
const MAX_TEXT_CHARS: usize = 2_000;
const MAX_NOTE_CHARS: usize = 240;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowStep {
    /// Bring an application to the front, or wait for it to arrive there.
    OpenApp { app: String },
    /// Invoke one accessible action, located semantically.
    Activate { app: String, frame: String, role: String, name: String },
    /// Replace the contents of one accessible editable text field.
    TypeText { app: String, frame: String, role: String, name: String, text: String },
    /// An application that exposes no usable accessibility tree: bring it forward
    /// and wait while the user performs this step. It is never blind-clicked.
    Manual { app: String, note: String },
}

impl WorkflowStep {
    pub fn app(&self) -> &str {
        match self {
            Self::OpenApp { app } | Self::Activate { app, .. } | Self::TypeText { app, .. } | Self::Manual { app, .. } => app,
        }
    }

    pub fn needs_accessibility(&self) -> bool {
        matches!(self, Self::Activate { .. } | Self::TypeText { .. })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Workflow {
    pub id: u64,
    pub name: String,
    /// Application-level steps, kept for display and for workflows recorded before steps existed.
    pub apps: Vec<String>,
    /// The typed steps, including accessible actions inside applications.
    #[serde(default)]
    pub steps: Vec<WorkflowStep>,
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
        let apps: Vec<String> = apps.iter().map(|app| app.trim().to_string()).filter(|app| !app.is_empty()).collect();
        if apps.len() < MIN_APP_STEPS {
            bail!("a workflow needs at least {MIN_APP_STEPS} application steps");
        }
        let steps: Vec<WorkflowStep> = apps.into_iter().map(|app| WorkflowStep::OpenApp { app }).collect();
        self.record_steps(name, steps, now)
    }

    pub fn record_steps(&mut self, name: &str, steps: Vec<WorkflowStep>, now: f64) -> Result<Workflow> {
        let name = name.trim();
        if name.is_empty() {
            bail!("a workflow needs a name");
        }
        let mut clean: Vec<WorkflowStep> = Vec::new();
        for step in steps {
            match &step {
                WorkflowStep::OpenApp { app } if app.trim().is_empty() => continue,
                WorkflowStep::Activate { app, frame, role, name }
                    if app.trim().is_empty() || frame.trim().is_empty() || role.trim().is_empty() || name.trim().is_empty() =>
                {
                    continue;
                }
                WorkflowStep::TypeText { app, frame, role, text, .. }
                    if app.trim().is_empty()
                        || frame.trim().is_empty()
                        || role.trim().is_empty()
                        || text.is_empty()
                        || role == "password text"
                        || text.chars().count() > MAX_TEXT_CHARS =>
                {
                    continue;
                }
                WorkflowStep::Manual { app, note } if app.trim().is_empty() || note.trim().is_empty() || note.chars().count() > MAX_NOTE_CHARS => {
                    continue;
                }
                WorkflowStep::OpenApp { app } if clean.last() == Some(&WorkflowStep::OpenApp { app: app.clone() }) => continue,
                _ => clean.push(step),
            }
        }
        if clean.is_empty() {
            bail!("a workflow needs at least one step");
        }
        if clean.len() > MAX_STEPS {
            bail!("a workflow can have at most {MAX_STEPS} steps");
        }
        let apps: Vec<String> = clean
            .iter()
            .filter_map(|step| match step {
                WorkflowStep::OpenApp { app } => Some(app.trim().to_string()),
                _ => None,
            })
            .collect();
        self.next_id += 1;
        let workflow = Workflow {
            id: self.next_id,
            name: name.to_string(),
            apps,
            steps: clean,
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

    pub fn has_actions(workflow: &Workflow) -> bool {
        workflow.steps.iter().any(|step| !matches!(step, WorkflowStep::OpenApp { .. }))
    }

    pub fn needs_accessibility(workflow: &Workflow) -> bool {
        workflow.steps.iter().any(WorkflowStep::needs_accessibility)
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
pub struct ActionRecording {
    pub name: String,
    pub started_at: f64,
    steps: Vec<WorkflowStep>,
}

impl ActionRecording {
    pub fn new(name: String, started_at: f64) -> Self {
        Self { name, started_at, steps: Vec::new() }
    }

    pub fn push(&mut self, step: WorkflowStep) {
        if self.steps.len() < MAX_STEPS {
            self.steps.push(step);
        }
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn last_app(&self) -> Option<&str> {
        self.steps.last().map(WorkflowStep::app)
    }

    pub fn finish(self) -> Vec<WorkflowStep> {
        self.steps
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

#[derive(Clone, Debug, PartialEq)]
pub struct ReplayState {
    pub steps: Vec<WorkflowStep>,
    pub index: usize,
    pub started: f64,
}

impl ReplayState {
    pub fn new(steps: Vec<WorkflowStep>, now: f64) -> Self {
        Self { steps, index: 0, started: now }
    }

    pub fn step(&self) -> Option<&WorkflowStep> {
        self.steps.get(self.index)
    }

    pub fn expected_app(&self) -> Option<&str> {
        self.step().map(WorkflowStep::app)
    }

    pub fn advance(&mut self, now: f64) {
        self.index += 1;
        self.started = now;
    }

    pub fn done(&self) -> bool {
        self.index >= self.steps.len()
    }

    pub fn timed_out(&self, now: f64, timeout: f64) -> bool {
        !self.done() && now - self.started > timeout
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
    fn an_action_workflow_can_have_one_step() {
        let mut workflows = Workflows::default();
        let step = WorkflowStep::Activate {
            app: "local.buddy.A11yProbe".into(),
            frame: "Buddy A11y Probe".into(),
            role: "button".into(),
            name: "Press me".into(),
        };
        let saved = workflows.record_steps("press", vec![step.clone()], 0.0).unwrap();
        assert_eq!(saved.steps, vec![step]);
        assert!(Workflows::has_actions(&saved));
        assert_eq!(saved.apps.len(), 0);
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
    fn text_entry_is_kept_as_a_typed_step_and_password_fields_are_refused() {
        let mut workflows = Workflows::default();
        let step = WorkflowStep::TypeText {
            app: "local.buddy.A11yProbe".into(),
            frame: "Buddy A11y Probe".into(),
            role: "text".into(),
            name: "".into(),
            text: "hello from Buddy".into(),
        };
        let saved = workflows.record_steps("type", vec![step.clone()], 0.0).unwrap();
        assert_eq!(saved.steps, vec![step]);
        assert!(Workflows::needs_accessibility(&saved));
        let restored: Workflows = serde_json::from_str(&serde_json::to_string(&workflows).unwrap()).unwrap();
        assert_eq!(restored.list()[0].steps, saved.steps);

        let password = WorkflowStep::TypeText {
            app: "local.buddy.A11yProbe".into(),
            frame: "Buddy A11y Probe".into(),
            role: "password text".into(),
            name: "Password".into(),
            text: "secret".into(),
        };
        assert!(workflows.record_steps("password", vec![password], 0.0).is_err());
    }

    #[test]
    fn a_manual_step_is_for_an_app_without_a_tree() {
        let mut workflows = Workflows::default();
        let step = WorkflowStep::Manual { app: "telegram-desktop".into(), note: "send the prepared file".into() };
        let saved = workflows.record_steps("telegram", vec![step.clone()], 0.0).unwrap();
        assert_eq!(saved.steps, vec![step]);
        assert!(!Workflows::needs_accessibility(&saved));
        assert!(Workflows::has_actions(&saved));
        assert!(workflows.record_steps("empty", vec![WorkflowStep::Manual { app: "x".into(), note: "".into() }], 0.0).is_err());
    }

    #[test]
    fn application_and_inside_application_steps_keep_their_order() {
        let mut workflows = Workflows::default();
        let steps = vec![
            WorkflowStep::OpenApp { app: "editor".into() },
            WorkflowStep::Activate { app: "editor".into(), frame: "Editor".into(), role: "button".into(), name: "New".into() },
            WorkflowStep::OpenApp { app: "browser".into() },
            WorkflowStep::TypeText {
                app: "browser".into(),
                frame: "Browser".into(),
                role: "text".into(),
                name: "Address".into(),
                text: "https://example.test".into(),
            },
        ];
        let saved = workflows.record_steps("mixed", steps.clone(), 0.0).unwrap();
        assert_eq!(saved.steps, steps);
        assert_eq!(saved.apps, vec!["editor", "browser"]);
        assert!(Workflows::needs_accessibility(&saved));
    }

    #[test]
    fn a_replay_walks_the_step_sequence() {
        let steps = vec![
            WorkflowStep::OpenApp { app: "terminal".into() },
            WorkflowStep::Activate { app: "terminal".into(), frame: "Terminal".into(), role: "button".into(), name: "Run".into() },
            WorkflowStep::Manual { app: "chat".into(), note: "send it".into() },
        ];
        let mut state = ReplayState::new(steps, 0.0);
        assert_eq!(state.expected_app(), Some("terminal"));
        assert!(!state.done());
        state.advance(1.0);
        assert!(matches!(state.step(), Some(WorkflowStep::Activate { .. })));
        state.advance(2.0);
        assert_eq!(state.expected_app(), Some("chat"));
        assert!(!state.timed_out(3.0, 20.0));
        state.advance(3.0);
        assert!(state.done());
        assert!(!state.timed_out(100.0, 20.0));
    }

    #[test]
    fn a_step_that_never_appears_times_out() {
        let state = ReplayState::new(vec![WorkflowStep::OpenApp { app: "terminal".into() }], 0.0);
        assert!(!state.timed_out(5.0, 20.0));
        assert!(state.timed_out(21.0, 20.0));
    }
}
