//! Advances Buddy through time: his needs and mood drift, he decides what to
//! do (see `behavior`), carries it out, and remembers it.

use crate::behavior::{self, Considered, Context, Decision, Memory, Record};
use crate::language::{self, Heard, TRICKS};
use crate::model::{self, Activity, Body, Identity, Life, Settings};
use crate::mood::Mood;
use crate::needs::Needs;
use crate::patterns::Patterns;
use crate::perception::Stimulus;
use crate::personality::{Personality, drift};
use crate::proto::StateMsg;
use crate::rng::Rng;
use crate::training::{Response, Training};
use crate::workflow::{ActionRecording, Recording, Workflow, WorkflowStep, Workflows};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;

/// A tick longer than this is treated as time Buddy did not live through
/// (suspend, a stalled process, a clock change): he ages, but gains no experience
/// and nothing is simulated across the gap.
pub const MAX_STEP_SECS: f64 = 5.0;

/// Height of a newborn relative to the adult.
const NEWBORN_SCALE: f64 = 0.45;
/// Loud sounds allowed in any ten minutes.
const LOUD_SOUNDS_PER_WINDOW: usize = 6;
const SOUND_WINDOW_SECS: f64 = 600.0;
const REST_SPOT_BINS: usize = 12;
/// With no input for this long, he takes it that you have gone.
pub const AWAY_AFTER_SECS: f64 = 600.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Hide for a number of minutes, or until shown again.
    Hide {
        minutes: Option<f64>,
    },
    Show,
    Sleep,
    Wake,
    Privacy(bool),
    Quiet(bool),
    Pet,
    Praise,
    Feed,
    Water,
    DragTo {
        x_frac: f64,
    },
    DragEnd,
    SetScreen {
        walk_px: f64,
    },
    SetAdultDays(f64),
    SetSize(f64),
    Rename(String),
    /// Turn one sense on or off.
    Sense {
        source: String,
        on: bool,
    },
    StayInFullscreen(bool),
    VisionModel(String),
    LanguageModel(String),
    AudioModel(String),
}

/// Something worth remembering; the daemon writes these to the journal.
#[derive(Clone, Debug, PartialEq)]
pub struct JournalEntry {
    pub kind: &'static str,
    pub data: Value,
}

/// What a tick or command produced for the outside world.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Journal(JournalEntry),
    /// He finished (or was interrupted in) an activity.
    Behaved(Record),
    /// Something he was told to keep, in the user's own words.
    Remember(String),
    /// He makes a sound.
    Vocalize {
        kind: &'static str,
        intensity: f64,
    },
    /// He has seen the start of one of the user's habits and would like to do the next step:
    /// bring `app` to the front. `standing` means the user has said to just do it.
    Offer {
        habit: String,
        app: String,
        standing: bool,
    },
    /// He looks at the person in front of the screen for about this long.
    Look {
        /// "you", "screen", "notification" or "sound".
        at: &'static str,
        secs: f64,
    },
}

fn journal(kind: &'static str, data: Value) -> Output {
    Output::Journal(JournalEntry { kind, data })
}

/// Where he likes to rest, learned from where he settles and where he is petted.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Habits {
    rest_spots: [f64; REST_SPOT_BINS],
}

impl Habits {
    fn note(&mut self, x_frac: f64, weight: f64) {
        for w in &mut self.rest_spots {
            *w *= 0.995; // old habits fade unless kept up
        }
        let bin = ((x_frac * REST_SPOT_BINS as f64) as usize).min(REST_SPOT_BINS - 1);
        self.rest_spots[bin] += weight;
    }

    /// The centre of his preferred spot, once a preference has formed.
    pub fn favourite(&self) -> Option<f64> {
        let (bin, weight) = self.rest_spots.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
        (*weight > 4.0).then_some((bin as f64 + 0.5) / REST_SPOT_BINS as f64)
    }
}

/// The inner state that persists between sessions.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Mind {
    /// `None` until he is born; then fixed apart from slow drift.
    pub personality: Option<Personality>,
    pub needs: Needs,
    pub mood: Mood,
    pub habits: Habits,
    pub food_out: bool,
    pub water_out: bool,
    /// Recurring sequences of applications he has noticed.
    pub patterns: Patterns,
    /// Application routines recorded explicitly by the user.
    pub workflows: Workflows,
    /// What words mean to him.
    pub training: Training,
}

/// How the user answered an offer of help.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    Yes,
    /// Yes, and no need to ask next time.
    Always,
    No,
    /// No, and do not offer this again.
    Never,
    /// The offer lapsed unanswered.
    Ignored,
}

/// The trick an activity counts as, when he does it of his own accord and is
/// rewarded for it just after hearing a new word.
fn trick_for(activity: Activity) -> Option<&'static str> {
    Some(match activity {
        Activity::Sit => "sit",
        Activity::Lie => "down",
        Activity::Stand => "stand",
        Activity::Sleep => "sleep",
        Activity::TailChase => "spin",
        Activity::Roll => "roll_over",
        Activity::PlayBow => "bow",
        Activity::Shake => "shake",
        Activity::Stretch => "stretch",
        _ => return None,
    })
}

/// 0 by day, 1 in the middle of the night, with a small after-lunch dip.
pub fn night_pressure(hour: f64) -> f64 {
    let h = hour.rem_euclid(24.0);
    match h {
        h if !(6.0..23.5).contains(&h) => 1.0,
        h if h >= 22.0 => (h - 22.0) / 1.5,
        h if h < 8.0 => (8.0 - h) / 2.0,
        h if (13.0..15.0).contains(&h) => 0.25,
        _ => 0.0,
    }
}

pub struct Sim {
    pub identity: Identity,
    pub life: Life,
    pub settings: Settings,
    pub personality: Personality,
    pub needs: Needs,
    pub mood: Mood,
    habits: Habits,
    food_out: bool,
    water_out: bool,
    memory: Memory,

    activity: Activity,
    variant: u8,
    intensity: f64,
    started: f64,
    ends: f64,
    planned: f64,
    seed: u32,
    cause: String,
    considered: Vec<Considered>,

    facing: i8,
    target_px: Option<f64>,
    /// A walk can have several legs, with a pause to look around between them.
    legs_left: u8,
    pause_until: f64,
    x_px: f64,
    v: f64,
    walk_px: f64,
    dragging: bool,

    last_tick: Option<f64>,
    hour: f64,
    greet_urge: f64,
    attended: f64,
    last_asked: f64,
    /// Grows each time a bid for attention goes unanswered, so he asks less and less.
    ask_patience: f64,
    loud_sounds: VecDeque<f64>,
    last_glance: f64,

    // What his senses currently tell him.
    patterns: Patterns,
    workflows: Workflows,
    recording: Option<Recording>,
    action_recording: Option<ActionRecording>,
    user_away: bool,
    away_since: f64,
    busy: f64,
    idle_secs: f64,
    locked: bool,
    fullscreen: bool,
    overview: bool,
    load: f64,
    last_startle: f64,
    glimpse: Option<(f64, String)>,

    pub training: Training,
    /// Doing something because he was asked, not because he felt like it.
    performing: bool,
    /// After "come": sit down in front of you on arriving.
    sit_on_arrival: bool,
    inhibited: Option<(Activity, f64)>,
    hushed_until: f64,
    /// When he last offered to help with anything, so that he does not pester.
    offered_at: f64,
    rng: Rng,
    out: Vec<Output>,
}

impl Sim {
    pub fn new(identity: Identity, life: Life, settings: Settings, body: Body, mind: Mind, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let walk_px = 1000.0;
        Self {
            identity,
            life,
            settings,
            personality: mind.personality.unwrap_or_else(|| Personality::born(&mut rng)),
            needs: mind.needs,
            mood: mind.mood,
            habits: mind.habits,
            food_out: mind.food_out,
            water_out: mind.water_out,
            memory: Memory::default(),
            activity: Activity::Stand,
            variant: 0,
            intensity: 0.5,
            started: 0.0,
            ends: 0.0,
            planned: 0.0,
            seed: 0,
            cause: "just arrived".into(),
            considered: Vec::new(),
            facing: if body.facing < 0 { -1 } else { 1 },
            target_px: None,
            legs_left: 0,
            pause_until: 0.0,
            x_px: body.x_frac.clamp(0.0, 1.0) * walk_px,
            v: 0.0,
            walk_px,
            dragging: false,
            last_tick: None,
            hour: 12.0,
            greet_urge: 0.0,
            attended: 0.0,
            last_asked: f64::NAN,
            ask_patience: 1.0,
            loud_sounds: VecDeque::new(),
            last_glance: 0.0,
            patterns: mind.patterns,
            workflows: mind.workflows,
            recording: None,
            action_recording: None,
            user_away: false,
            away_since: 0.0,
            busy: 0.0,
            idle_secs: 0.0,
            locked: false,
            fullscreen: false,
            overview: false,
            load: 0.0,
            last_startle: f64::NEG_INFINITY,
            glimpse: None,
            training: mind.training,
            performing: false,
            sit_on_arrival: false,
            inhibited: None,
            hushed_until: 0.0,
            offered_at: f64::NEG_INFINITY,
            rng,
            out: Vec::new(),
        }
    }

    /// Start recording the applications the user works through.
    pub fn start_workflow_recording(&mut self, name: String, now: f64) -> Result<()> {
        if self.recording.is_some() || self.action_recording.is_some() {
            bail!("a workflow is already being recorded");
        }
        let name = name.trim();
        if name.is_empty() {
            bail!("a workflow needs a name");
        }
        self.recording = Some(Recording::new(name.to_string(), now));
        Ok(())
    }

    /// Stop recording and save the routine. Fewer than two applications is an error.
    pub fn stop_workflow_recording(&mut self, now: f64) -> Result<Workflow> {
        let recording = self.recording.take().ok_or_else(|| anyhow::anyhow!("no workflow is being recorded"))?;
        let name = recording.name.clone();
        let apps = recording.finish(now);
        self.workflows.record(&name, &apps, now)
    }

    pub fn cancel_workflow_recording(&mut self) -> bool {
        self.recording.take().is_some()
    }

    /// Start recording inside-application accessible actions.
    pub fn start_action_recording(&mut self, name: String, now: f64) -> Result<()> {
        if self.recording.is_some() || self.action_recording.is_some() {
            bail!("a workflow is already being recorded");
        }
        let name = name.trim();
        if name.is_empty() {
            bail!("a workflow needs a name");
        }
        self.action_recording = Some(ActionRecording::new(name.to_string(), now));
        Ok(())
    }

    pub fn capture_action_step(&mut self, step: WorkflowStep) -> Result<()> {
        let recording = self
            .action_recording
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no workflow capture is active; start one with: buddy workflow capture NAME"))?;
        // Make the application transition explicit, so a captured sequence that moves
        // between applications is a real mixed workflow rather than an implicit one.
        let app = step.app().to_string();
        if !matches!(step, WorkflowStep::OpenApp { .. }) && recording.last_app() != Some(app.as_str()) {
            recording.push(WorkflowStep::OpenApp { app: app.clone() });
        }
        recording.push(step);
        Ok(())
    }

    pub fn action_recording_len(&self) -> usize {
        self.action_recording.as_ref().map_or(0, ActionRecording::len)
    }

    pub fn has_action_recording(&self) -> bool {
        self.action_recording.is_some()
    }

    pub fn stop_action_recording(&mut self, now: f64) -> Result<Workflow> {
        let recording = self.action_recording.take().ok_or_else(|| anyhow::anyhow!("no inside-app workflow is being recorded"))?;
        let name = recording.name.clone();
        let steps = recording.finish();
        if steps.is_empty() {
            bail!("no accessible steps were captured");
        }
        self.workflows.record_steps(&name, steps, now)
    }

    pub fn cancel_action_recording(&mut self) -> bool {
        self.action_recording.take().is_some()
    }

    pub fn workflows(&self) -> &[Workflow] {
        self.workflows.list()
    }

    pub fn workflow_mut(&mut self, id: u64) -> Option<&mut Workflow> {
        self.workflows.get_mut(id)
    }

    pub fn delete_workflow(&mut self, id: u64) -> bool {
        self.workflows.remove(id)
    }

    pub fn workflow_outcome(&mut self, id: u64, ok: bool, now: f64) {
        self.workflows.outcome(id, ok, now);
    }

    pub fn mind(&self) -> Mind {
        Mind {
            personality: Some(self.personality.clone()),
            needs: self.needs.clone(),
            mood: self.mood.clone(),
            habits: self.habits.clone(),
            food_out: self.food_out,
            water_out: self.water_out,
            patterns: self.patterns.clone(),
            workflows: self.workflows.clone(),
            training: self.training.clone(),
        }
    }

    pub fn body(&self) -> Body {
        Body { x_frac: self.x_frac(), facing: self.facing }
    }

    pub fn activity(&self) -> Activity {
        self.activity
    }

    pub fn is_moving(&self) -> bool {
        self.v.abs() > 0.5 || self.dragging
    }

    /// Restores what he remembers of his recent behaviour (oldest first).
    pub fn remember(&mut self, past: impl IntoIterator<Item = (f64, Activity, u8)>) {
        for (ts, activity, variant) in past {
            self.memory.note(ts, activity, variant);
        }
    }

    /// Call once when a session starts, with how long he was not running.
    pub fn returned_after(&mut self, away_secs: f64) {
        if away_secs > 60.0 {
            self.needs.rest_through_gap(away_secs);
        }
        if away_secs > 1800.0 {
            self.greet_urge = 1.0;
            self.mood.valence = self.mood.valence.max(0.6);
        }
    }

    fn x_frac(&self) -> f64 {
        (self.x_px / self.walk_px).clamp(0.0, 1.0)
    }

    fn age_secs(&self, now: f64) -> f64 {
        (now - self.identity.born_at as f64).max(0.0)
    }

    fn maturity(&self, now: f64) -> f64 {
        model::maturity(self.age_secs(now), self.settings.adult_days)
    }

    /// Current standing height as a fraction of the adult's.
    fn scale(&self, now: f64) -> f64 {
        NEWBORN_SCALE + (1.0 - NEWBORN_SCALE) * model::growth(self.maturity(now))
    }

    /// Pixels per second at an easy walk.
    pub fn walk_speed(&self, now: f64) -> f64 {
        0.55 * self.settings.size_px * self.scale(now)
    }

    /// Top speed for what he is doing now, as a multiple of his walk.
    fn pace(&self) -> f64 {
        match (self.activity, self.variant) {
            (Activity::Zoomies, _) => 3.4,
            (Activity::Walk, 1) => 1.9, // trot
            (Activity::Walk, 2) => 0.5, // nose down, following a scent
            _ => 1.0,
        }
    }

    pub fn tick(&mut self, now: f64, hour: f64) -> Vec<Output> {
        let dt = self.last_tick.map_or(0.0, |prev| now - prev);
        self.last_tick = Some(now);
        self.life.last_seen = now as i64;
        self.hour = hour;
        if self.last_asked.is_nan() {
            self.last_asked = now; // a new session does not open with a bid for attention
        }
        if !(0.0..=MAX_STEP_SECS).contains(&dt) {
            self.v = 0.0;
            self.returned_after(dt.max(0.0));
            self.out.push(journal("gap", json!({ "secs": dt })));
            return std::mem::take(&mut self.out);
        }
        self.life.awake_secs += dt;
        let stage = model::stage(self.maturity(now));
        self.needs.advance(dt, self.activity.effort(), &self.personality, stage);
        self.mood.advance(dt, &self.needs, self.activity.effort());
        self.greet_urge *= (-dt / 30.0).exp();
        self.attended *= (-dt / 300.0).exp();

        let hide_expired = self.settings.hidden_until.is_some_and(|until| until != i64::MAX && now as i64 >= until);
        if hide_expired {
            self.settings.hidden_until = None;
            self.out.push(journal("shown", json!({ "reason": "timer" })));
        }

        self.maybe_check_on_you(now, dt);
        self.maybe_offer_help(now);

        if self.dragging {
            self.v = 0.0;
        } else if self.settings.asleep_by_user {
            if self.activity != Activity::Sleep {
                self.finish(now);
                self.begin(Activity::Sleep, "you sent him to sleep", now);
            }
        } else if self.greet_urge > 0.5 && !matches!(self.activity, Activity::Greet | Activity::Invite) {
            self.finish(now);
            self.begin(Activity::Greet, "happy to see you", now);
        } else {
            match self.activity {
                Activity::Walk | Activity::Zoomies => self.step_move(now, dt),
                Activity::Sleep => {
                    let night = night_pressure(hour - self.settings.night_shift_hours);
                    let slept = now - self.started;
                    let rested = self.needs.fatigue < 0.08 && night < 0.5;
                    let napped = self.needs.fatigue < 0.2 && night < 0.2 && slept > 1200.0;
                    if rested || napped {
                        self.finish(now);
                        self.choose(now);
                    } else if self.rng.chance(dt / 900.0) {
                        self.voice("dream", 0.2, now);
                    }
                }
                _ if now >= self.ends => {
                    self.finish(now);
                    self.choose(now);
                }
                _ => {}
            }
        }
        std::mem::take(&mut self.out)
    }

    /// Every so often, unprompted, he looks up at you. More often if he is
    /// affectionate or you have just been paying him attention; never while
    /// asleep or absorbed in something, and never twice in quick succession.
    fn maybe_check_on_you(&mut self, now: f64, dt: f64) {
        let absorbed = matches!(
            self.activity,
            Activity::Sleep | Activity::Zoomies | Activity::TailChase | Activity::Shake | Activity::Roll | Activity::Eat | Activity::Drink
        );
        // These already hold your eye as part of what they are.
        let already_looking = matches!(self.activity, Activity::Greet | Activity::SeekAttention | Activity::PlayBow);
        // And there is no point looking for someone who is not there.
        if absorbed || already_looking || self.user_away || now - self.last_glance < 12.0 {
            return;
        }
        let per_second = (0.5 + self.personality.affection) * (0.6 + 0.8 * self.attended) / 75.0;
        if self.rng.chance(per_second * dt) {
            let secs = self.rng.range(1.2, 2.5 + 2.0 * self.attended);
            self.look_at_you(secs, now);
        }
    }

    fn look_at_you(&mut self, secs: f64, now: f64) {
        self.last_glance = now + secs;
        self.out.push(Output::Look { at: "you", secs });
    }

    /// Accelerate towards his pace, then brake so he stops on the target
    /// instead of snapping to it.
    fn step_move(&mut self, now: f64, dt: f64) {
        let Some(target) = self.target_px else {
            if self.pause_until > 0.0 {
                if now >= self.pause_until {
                    self.pause_until = 0.0;
                    self.target_px = Some(self.waypoint());
                }
                return;
            }
            self.finish(now);
            self.choose(now);
            return;
        };
        let vmax = self.walk_speed(now) * self.pace();
        let accel = vmax * 2.5;
        let dist = target - self.x_px;
        let dir = if dist >= 0.0 { 1.0 } else { -1.0 };
        // The fastest speed from which he can still stop on the target, judged from
        // where this step will leave him so a coarse tick does not overshoot.
        let ahead = (dist.abs() - self.v.abs() * dt).max(0.0);
        let wanted = vmax.min((2.0 * accel * ahead).sqrt()) * dir;
        self.v += (wanted - self.v).clamp(-accel * dt, accel * dt);
        self.x_px = (self.x_px + self.v * dt).clamp(0.0, self.walk_px);
        if self.v.abs() > 1.0 {
            self.facing = if self.v > 0.0 { 1 } else { -1 };
        }
        if (target - self.x_px) * dir <= 0.3 {
            self.x_px = target;
            self.v = 0.0;
            self.target_px = None;
            if self.activity == Activity::Zoomies && now < self.ends {
                self.target_px = Some(self.somewhere_else(0.25)); // and back again
            } else if std::mem::take(&mut self.sit_on_arrival) {
                self.finish(now);
                self.perform_as(Activity::Sit, 0, 10.0, "he came when called", now);
                self.look("you", 4.0, now);
            } else if self.activity == Activity::Walk && self.legs_left > 0 {
                self.legs_left -= 1;
                self.pause_until = now + self.rng.range(0.8, 3.5);
            } else {
                self.finish(now);
                self.choose(now);
            }
        }
    }

    /// Where a walk heads next: a short step when following a scent, otherwise anywhere.
    fn waypoint(&mut self) -> f64 {
        if self.variant == 2 {
            let step = self.rng.range(0.08, 0.2) * self.walk_px * if self.rng.chance(0.5) { 1.0 } else { -1.0 };
            (self.x_px + step).clamp(0.0, self.walk_px)
        } else {
            self.somewhere_else(0.125)
        }
    }

    /// A point on the screen at least `min_frac` of its width away from him.
    fn somewhere_else(&mut self, min_frac: f64) -> f64 {
        let mut target = self.rng.range(0.0, self.walk_px);
        if (target - self.x_px).abs() < min_frac * self.walk_px {
            let hop = (min_frac + 0.15) * self.walk_px;
            target = if self.x_px < self.walk_px / 2.0 { self.x_px + hop } else { self.x_px - hop };
        }
        target.clamp(0.0, self.walk_px)
    }

    fn context(&self, now: f64) -> Context<'_> {
        let favourite = self.habits.favourite();
        Context {
            needs: &self.needs,
            mood: &self.mood,
            traits: &self.personality,
            stage: model::stage(self.maturity(now)),
            // If you are up late with him, he stays up with you.
            night: night_pressure(self.hour - self.settings.night_shift_hours) * (1.0 - self.attended),
            now,
            quiet: self.settings.quiet,
            food_out: self.food_out,
            water_out: self.water_out,
            greet_urge: self.greet_urge,
            attended: self.attended,
            secs_since_asking: (now - self.last_asked) / self.ask_patience,
            user_busy: self.busy,
            user_away: self.user_away,
            at_favourite_spot: favourite.is_some_and(|f| (f - self.x_frac()).abs() < 0.06),
            current: self.activity,
            inhibited: self.inhibited.filter(|(_, until)| now < *until).map(|(activity, _)| activity),
        }
    }

    fn choose(&mut self, now: f64) {
        let mut rng = self.rng.clone();
        let mut decision = behavior::decide(&self.context(now), &self.memory, &mut rng);
        self.rng = rng;
        // He would rather settle in his own spot than wherever he happens to be.
        let settling = matches!(decision.activity, Activity::Lie | Activity::Sleep);
        let far_from_spot = self.habits.favourite().is_some_and(|f| (f - self.x_frac()).abs() > 0.15);
        if settling && far_from_spot && self.rng.chance(0.6) {
            decision.activity = Activity::Walk;
            decision.variant = 3;
            decision.secs = 0.0;
            decision.cause = "going to his favourite spot".into();
        }
        self.start(decision, now);
    }

    /// Start an activity that circumstances, not the decision engine, call for.
    fn begin(&mut self, activity: Activity, cause: &str, now: f64) {
        let spec = behavior::spec(activity);
        let decision = Decision {
            activity,
            variant: self.memory.pick_variant(spec, now, &mut self.rng),
            secs: if spec.secs.1 > 0.0 { self.rng.range(spec.secs.0, spec.secs.1) } else { 0.0 },
            intensity: (0.6 + 0.4 * self.mood.arousal).clamp(0.0, 1.0),
            cause: cause.to_string(),
            considered: Vec::new(),
        };
        self.start(decision, now);
    }

    fn start(&mut self, d: Decision, now: f64) {
        self.activity = d.activity;
        self.variant = d.variant;
        self.intensity = d.intensity;
        self.started = now;
        self.planned = d.secs;
        self.ends = now + d.secs;
        self.seed = (self.rng.unit() * f64::from(u32::MAX)) as u32;
        self.cause = d.cause;
        self.considered = d.considered;
        if !matches!(d.activity, Activity::Walk | Activity::Zoomies) {
            self.v = 0.0;
        }
        self.target_px = None;
        self.legs_left = 0;
        self.pause_until = 0.0;
        match d.activity {
            Activity::Walk => {
                let favourite = self.habits.favourite().filter(|_| d.variant == 3);
                self.target_px = Some(match favourite {
                    Some(spot) => spot * self.walk_px,
                    None => {
                        // A wander has up to three legs; a walk to his spot is direct.
                        self.legs_left = self.rng.below(3);
                        self.waypoint()
                    }
                });
            }
            Activity::Zoomies => {
                self.target_px = Some(self.somewhere_else(0.3));
                if self.rng.chance(0.5) {
                    self.voice("bark", 0.7, now);
                }
            }
            Activity::Lie | Activity::Sleep | Activity::Sit => {
                self.habits.note(self.x_frac(), 1.0);
                if d.activity == Activity::Lie && self.rng.chance(0.3) {
                    self.voice("sigh", 0.4, now);
                }
            }
            Activity::Greet => self.voice("bark", 0.6 + 0.4 * self.mood.arousal, now),
            // A short, soft woof: an invitation, not an alarm.
            Activity::Invite => self.voice("bark", 0.4, now),
            Activity::PlayBow => {
                if self.rng.chance(0.4) {
                    self.voice("bark", 0.5, now);
                }
            }
            // Offering a paw because he was asked is not a bid for attention.
            Activity::SeekAttention if !self.performing => {
                self.last_asked = now;
                // Unanswered, he waits twice as long before trying again (up to six hours).
                self.ask_patience = (self.ask_patience * 2.0).min(18.0);
                self.voice("whine", 0.35, now);
            }
            Activity::Yawn => self.voice("yawn", 0.5, now),
            Activity::Sniff => self.voice("sniff", 0.4, now),
            Activity::Shake => self.voice("shake", 0.6, now),
            Activity::Eat => self.voice("eat", 0.5, now),
            Activity::Drink => self.voice("drink", 0.5, now),
            _ => {}
        }
    }

    /// Close the current activity: apply what it did for him and log it.
    fn finish(&mut self, now: f64) {
        self.performing = false;
        let secs = now - self.started;
        if self.started == 0.0 || secs < 0.3 {
            return;
        }
        let done = if self.planned > 0.0 { (secs / self.planned).clamp(0.0, 1.0) } else { 1.0 };
        let n = &mut self.needs;
        match self.activity {
            Activity::Stretch => n.stiffness *= 1.0 - 0.9 * done,
            Activity::Yawn => n.stiffness = (n.stiffness - 0.1 * done).max(0.0),
            Activity::Scratch => n.itch = (n.itch - 0.7 * done).max(0.0),
            Activity::Groom => n.itch = (n.itch - 0.5 * done).max(0.0),
            Activity::Shake => {
                n.itch = (n.itch - 0.35 * done).max(0.0);
                n.stiffness = (n.stiffness - 0.3 * done).max(0.0);
            }
            Activity::Sniff => n.boredom = (n.boredom - 0.25 * done).max(0.0),
            Activity::Roll => {
                n.itch = (n.itch - 0.3 * done).max(0.0);
                n.boredom = (n.boredom - 0.2 * done).max(0.0);
            }
            Activity::Greet => n.social = (n.social - 0.3 * done).max(0.0),
            Activity::Eat if done > 0.8 => {
                n.hunger = 0.05;
                self.food_out = false;
                self.mood.lift(0.1, 0.0);
            }
            Activity::Drink if done > 0.8 => {
                n.thirst = 0.05;
                self.water_out = false;
            }
            _ => {}
        }
        self.memory.note(now, self.activity, self.variant);
        self.out.push(Output::Behaved(Record {
            ts: self.started,
            activity: self.activity,
            variant: self.variant,
            secs,
            intensity: self.intensity,
            cause: self.cause.clone(),
            x_frac: self.x_frac(),
        }));
    }

    /// Makes a sound, unless quiet mode, the hour, or his own restraint says no.
    fn voice(&mut self, kind: &'static str, intensity: f64, now: f64) {
        if self.settings.quiet || now < self.hushed_until {
            return;
        }
        if matches!(kind, "bark" | "whine") {
            while self.loud_sounds.front().is_some_and(|t| now - t > SOUND_WINDOW_SECS) {
                self.loud_sounds.pop_front();
            }
            if night_pressure(self.hour - self.settings.night_shift_hours) > 0.3 || self.loud_sounds.len() >= LOUD_SOUNDS_PER_WINDOW {
                return;
            }
            self.loud_sounds.push_back(now);
        }
        self.out.push(Output::Vocalize { kind, intensity });
    }

    pub fn apply(&mut self, cmd: Command, now: f64) -> Vec<Output> {
        match cmd {
            Command::Hide { minutes } => {
                self.settings.hidden_until = Some(match minutes {
                    Some(m) if m > 0.0 => (now + m * 60.0) as i64,
                    _ => i64::MAX,
                });
                self.out.push(journal("hidden", json!({ "minutes": minutes })));
            }
            Command::Show => {
                if self.settings.hidden_until.take().is_some() {
                    self.out.push(journal("shown", json!({ "reason": "user" })));
                }
            }
            Command::Sleep => {
                self.settings.asleep_by_user = true;
                if self.activity != Activity::Sleep {
                    self.finish(now);
                    self.begin(Activity::Sleep, "you sent him to sleep", now);
                }
                self.out.push(journal("sent_to_sleep", Value::Null));
            }
            Command::Wake => {
                self.settings.asleep_by_user = false;
                if self.activity == Activity::Sleep {
                    self.finish(now);
                    self.begin(Activity::Stand, "you woke him", now);
                }
                self.out.push(journal("woken", Value::Null));
            }
            Command::Privacy(on) => {
                if self.settings.privacy_mode != on {
                    self.settings.privacy_mode = on;
                    self.out.push(journal("privacy_mode", json!({ "on": on })));
                }
            }
            Command::Quiet(on) => {
                if self.settings.quiet != on {
                    self.settings.quiet = on;
                    self.out.push(journal("quiet_mode", json!({ "on": on })));
                }
            }
            Command::Pet => {
                self.life.pets += 1;
                let missed_you = self.needs.social > 0.6;
                self.needs.social = (self.needs.social - 0.35).max(0.0);
                self.mood.lift(0.12, 0.25);
                self.attended = 1.0;
                self.ask_patience = 1.0;
                // Being petted, often, is what makes an affectionate dog.
                drift(&mut self.personality.affection, true, 0.003);
                drift(&mut self.personality.sociability, true, 0.002);
                self.habits.note(self.x_frac(), 2.0);
                let asleep = self.activity == Activity::Sleep;
                // A pat on a sleeping dog mostly earns a stir; it wakes him only if he was nearly done.
                if asleep && !self.settings.asleep_by_user && self.needs.fatigue <= 0.2 {
                    self.finish(now);
                    self.begin(Activity::Greet, "you woke him with a pat", now);
                } else if !asleep && missed_you {
                    self.greet_urge = 1.0;
                }
                if self.activity != Activity::Sleep {
                    let secs = self.rng.range(2.5, 4.5);
                    self.look_at_you(secs, now);
                }
                self.out.push(journal("petted", json!({ "total": self.life.pets })));
            }
            Command::Praise => {
                self.needs.social = (self.needs.social - 0.2).max(0.0);
                self.mood.lift(0.2, 0.15);
                self.attended = 1.0;
                self.ask_patience = 1.0;
                drift(&mut self.personality.confidence, true, 0.004);
                if self.activity != Activity::Sleep {
                    let secs = self.rng.range(2.0, 4.0);
                    self.look_at_you(secs, now);
                }
                self.out.push(journal("praised", Value::Null));
            }
            Command::Feed => {
                self.food_out = true;
                self.reconsider(now);
                self.out.push(journal("fed", Value::Null));
            }
            Command::Water => {
                self.water_out = true;
                self.reconsider(now);
                self.out.push(journal("watered", Value::Null));
            }
            Command::DragTo { x_frac } => {
                if !self.dragging && matches!(self.activity, Activity::Walk | Activity::Zoomies) {
                    self.finish(now);
                    self.begin(Activity::Stand, "you picked him up", now);
                }
                self.dragging = true;
                self.target_px = None;
                self.v = 0.0;
                self.x_px = x_frac.clamp(0.0, 1.0) * self.walk_px;
            }
            Command::DragEnd => {
                if self.dragging {
                    self.dragging = false;
                    // Being put somewhere counts towards where he thinks he belongs.
                    self.habits.note(self.x_frac(), 1.0);
                    self.out.push(journal("moved_by_user", json!({ "x_frac": self.x_frac() })));
                }
            }
            Command::SetScreen { walk_px } => {
                // Keep his place proportionally when the screen changes.
                let width = walk_px.max(1.0);
                let ratio = width / self.walk_px;
                self.x_px *= ratio;
                self.target_px = self.target_px.map(|t| t * ratio);
                self.walk_px = width;
            }
            Command::SetAdultDays(days) => {
                self.settings.adult_days = days.max(1.0);
                self.out.push(journal("setting", json!({ "adult_days": self.settings.adult_days })));
            }
            Command::SetSize(px) => {
                self.settings.size_px = px.clamp(120.0, 900.0);
                self.out.push(journal("setting", json!({ "size_px": self.settings.size_px })));
            }
            Command::Sense { source, on } => {
                if self.settings.senses.set(&source, on) {
                    self.out.push(journal("sense", json!({ "source": source, "on": on })));
                }
            }
            Command::StayInFullscreen(on) => self.settings.stay_in_fullscreen = on,
            Command::VisionModel(model) => self.settings.vision_model = model,
            Command::LanguageModel(model) => self.settings.language_model = model,
            Command::AudioModel(model) => self.settings.audio_model = model,
            Command::Rename(name) => {
                self.identity.name = name;
                self.out.push(journal("setting", json!({ "name": self.identity.name })));
            }
        }
        std::mem::take(&mut self.out)
    }

    /// Something was said to him. What he makes of it depends on what he has learned.
    pub fn hear(&mut self, heard: Heard, now: f64) -> Vec<Output> {
        self.attended = 1.0;
        self.ask_patience = 1.0;
        self.mood.confusion = 0.0;
        let asleep = self.activity == Activity::Sleep;
        match heard {
            Heard::Cue(cue) => {
                if asleep {
                    // Only being called or told to wake gets through to a sleeping dog.
                    if matches!(cue.as_str(), "wake" | "come") && !self.settings.asleep_by_user {
                        self.finish(now);
                        self.begin(Activity::Stand, "you woke him", now);
                        self.look("you", 3.0, now);
                    }
                    return std::mem::take(&mut self.out);
                }
                // A tired or over-excited dog has less attention to give.
                let focus = (1.0 - 0.5 * self.needs.fatigue - 1.6 * (self.mood.arousal - 0.75).max(0.0)).clamp(0.2, 1.0);
                match self.training.hear(&cue, &TRICKS, focus, now, &mut self.rng) {
                    Response::Does(trick) => self.perform(&trick, &cue, now),
                    Response::Puzzled => self.puzzle(now),
                }
            }
            Heard::Praise | Heard::Treat | Heard::Approval => {
                let (amount, lift) = match heard {
                    Heard::Treat => (0.30, 0.25),
                    Heard::Praise => (0.18, 0.2),
                    _ => (0.10, 0.08),
                };
                self.needs.social = (self.needs.social - 0.2).max(0.0);
                self.mood.lift(lift, 0.15);
                drift(&mut self.personality.confidence, true, 0.004);
                // If a cue came just before, this is about that: his answer to it, or what he was doing anyway.
                let doing = if self.performing { None } else { trick_for(self.activity) };
                if let Some((cue, trick, strength)) = self.training.reward(amount, doing, &TRICKS, now) {
                    self.out.push(journal("reinforced", json!({ "cue": cue, "trick": trick, "strength": (strength * 100.0).round() / 100.0 })));
                }
                if heard == Heard::Treat {
                    self.needs.hunger = (self.needs.hunger - 0.05).max(0.0);
                    self.voice("eat", 0.4, now);
                }
                if !asleep {
                    let secs = self.rng.range(2.0, 4.0);
                    self.look("you", secs, now);
                }
                self.out.push(journal(if heard == Heard::Treat { "treated" } else { "praised" }, Value::Null));
            }
            Heard::Correction => {
                if let Some((cue, trick, strength)) = self.training.correct(&TRICKS, now) {
                    self.out.push(journal("corrected", json!({ "cue": cue, "trick": trick, "strength": (strength * 100.0).round() / 100.0 })));
                }
                // He takes it in without being crushed by it.
                self.mood.valence = (self.mood.valence - 0.05).max(-0.2);
                if self.performing && !asleep {
                    self.finish(now);
                    self.perform_as(Activity::Stand, 0, 4.0, "you said no", now);
                }
                if !asleep {
                    self.look("you", 3.0, now);
                }
            }
            Heard::Stop => {
                if !asleep {
                    // Whatever it was, he will not start it again for half an hour.
                    self.inhibited = Some((self.activity, now + 1800.0));
                    self.out.push(journal("told_to_stop", json!({ "activity": self.activity })));
                    self.finish(now);
                    self.perform_as(Activity::Sit, 0, 8.0, "you told him to stop", now);
                    self.look("you", 3.0, now);
                }
            }
            Heard::Quiet => {
                self.hushed_until = now + 1800.0;
                self.out.push(journal("hushed", json!({ "minutes": 30 })));
            }
            Heard::Remember(text) => {
                self.take_to_heart(&text);
                self.out.push(Output::Remember(text));
                if !asleep {
                    self.look("you", 3.0, now);
                }
            }
            Heard::Teach { cue, trick } => {
                let cue = language::normalise(&cue);
                if TRICKS.contains(&trick.as_str()) && !cue.is_empty() {
                    self.training.teach(&cue, &trick);
                    self.out.push(journal("taught", json!({ "cue": cue, "trick": trick })));
                } else {
                    self.puzzle(now);
                }
            }
            // With nothing on offer, "yes" is plain approval and "not now" needs no answer;
            // when there is an offer, the daemon takes these before they reach here.
            Heard::Yes | Heard::Always => {
                if let Some((cue, trick, strength)) = self.training.reward(0.10, None, &TRICKS, now) {
                    self.out.push(journal("reinforced", json!({ "cue": cue, "trick": trick, "strength": (strength * 100.0).round() / 100.0 })));
                }
                if !asleep {
                    self.look("you", 2.5, now);
                }
            }
            Heard::NotNow | Heard::NoHelp | Heard::Task { .. } => {
                // A request for the computer is carried out elsewhere; here he just gives you his attention.
                if !asleep {
                    self.look("you", 3.0, now);
                }
            }
            Heard::Called => self.notice_name(now),
            Heard::Chat => {
                if !asleep {
                    let secs = self.rng.range(2.5, 4.5);
                    self.look("you", secs, now);
                    self.mood.lift(0.03, 0.08);
                }
            }
            Heard::Unknown => {
                if !asleep {
                    self.puzzle(now);
                }
            }
        }
        std::mem::take(&mut self.out)
    }

    /// The start of a habit he knows: offer to do the next step. At most one offer
    /// every ten minutes, and no more than hourly about the same habit; never to an
    /// empty room, while asleep, or in quiet mode.
    fn maybe_offer_help(&mut self, now: f64) {
        if let Some(habit) = self.patterns.settle(now) {
            self.out.push(journal("habit_noticed", json!({ "apps": habit.apps, "times": habit.times, "days": habit.days })));
        }
        if self.user_away || self.activity == Activity::Sleep || self.settings.quiet || now - self.offered_at < 600.0 {
            return;
        }
        let Some((habit, app)) = self.patterns.anticipate() else { return };
        let Some(record) = self.patterns.habit_mut(&habit) else { return };
        if now - record.last_offer < 3600.0 {
            return;
        }
        record.last_offer = now;
        self.offered_at = now;
        let standing = record.standing_yes;
        if !standing {
            self.finish(now);
            self.begin(Activity::Invite, "he has seen what you are doing and wants to help", now);
        }
        self.look("you", 4.0, now);
        self.out.push(Output::Offer { habit, app, standing });
    }

    /// How the user answered an offer about a habit. Yes is remembered, no is respected:
    /// three refusals, or one "don't help me with this", and he never offers it again.
    pub fn offer_answered(&mut self, habit: Option<&str>, answer: Answer, now: f64) -> Vec<Output> {
        if let Some(record) = habit.and_then(|key| self.patterns.habit_mut(key)) {
            match answer {
                Answer::Yes => record.accepted += 1,
                Answer::Always => {
                    record.accepted += 1;
                    record.standing_yes = true;
                }
                Answer::No => {
                    record.declined += 1;
                    record.never = record.declined >= 3;
                    record.standing_yes = false;
                }
                Answer::Never => {
                    record.declined += 1;
                    record.never = true;
                    record.standing_yes = false;
                }
                Answer::Ignored => {}
            }
        }
        match answer {
            Answer::Yes | Answer::Always => {
                self.mood.lift(0.15, 0.15);
                self.look("you", 2.5, now);
            }
            Answer::No | Answer::Never => {
                // He takes no for an answer and settles, without sulking.
                if self.activity == Activity::Invite {
                    self.finish(now);
                    self.begin(Activity::Sit, "you said no thanks", now);
                }
            }
            Answer::Ignored => {}
        }
        self.out.push(journal("offer_answered", json!({ "habit": habit, "answer": format!("{answer:?}").to_lowercase() })));
        std::mem::take(&mut self.out)
    }

    /// He is waiting for you to say yes or no to something: sitting, eyes on you.
    pub fn await_answer(&mut self, now: f64) -> Vec<Output> {
        if self.activity != Activity::Sleep && self.activity != Activity::Invite {
            self.finish(now);
            self.perform_as(Activity::Sit, 0, 20.0, "he is waiting for your answer", now);
        }
        self.look("you", 4.0, now);
        std::mem::take(&mut self.out)
    }

    /// Something he did for you on the computer finished.
    pub fn task_done(&mut self, worked: bool, now: f64) -> Vec<Output> {
        if worked {
            self.mood.lift(0.12, 0.2);
            self.look("you", 3.0, now);
        } else if self.activity != Activity::Sleep {
            self.puzzle(now);
        }
        std::mem::take(&mut self.out)
    }

    /// His name is a recall, not a trick. The first call gets a bright greeting;
    /// after that he comes to the middle of the screen and sits. A name called while
    /// he is asleep wakes him, unless the user deliberately sent him to sleep.
    fn notice_name(&mut self, now: f64) {
        if self.activity == Activity::Sleep && self.settings.asleep_by_user {
            return;
        }
        self.life.name_calls += 1;
        self.out.push(journal("called", json!({ "total": self.life.name_calls })));
        if self.activity == Activity::Sleep {
            self.finish(now);
            self.perform_as(Activity::Stand, 0, 4.0, "you called his name", now);
            self.look("you", 3.5, now);
            return;
        }
        if self.life.name_calls == 1 {
            self.finish(now);
            self.perform_as(Activity::Greet, 0, 4.0, "you called his name", now);
            self.look("you", 3.5, now);
        } else {
            let name = self.identity.name.clone();
            self.perform("come", &name, now);
            self.look("you", 3.0, now);
        }
    }

    /// He did not understand: he looks at you and tilts his head.
    fn puzzle(&mut self, now: f64) {
        self.mood.confusion = 1.0;
        self.look("you", 3.5, now);
    }

    /// Does a trick because he was asked.
    fn perform(&mut self, trick: &str, cue: &str, now: f64) {
        let because = format!("you said \u{201c}{cue}\u{201d}");
        self.finish(now);
        match trick {
            "come" => {
                // Without a camera, "here" is the middle of the screen.
                self.perform_as(Activity::Walk, 1, 0.0, &because, now);
                self.target_px = Some(self.walk_px * 0.5);
                self.legs_left = 0;
                self.sit_on_arrival = true;
            }
            "sit" => self.perform_as(Activity::Sit, 0, 12.0, &because, now),
            "down" => self.perform_as(Activity::Lie, 0, 15.0, &because, now),
            "stand" | "wake" => self.perform_as(Activity::Stand, 2, 6.0, &because, now),
            "stay" => {
                // Holds whatever he is doing a while longer.
                self.performing = true;
                self.ends = self.ends.max(now) + 15.0;
            }
            "sleep" => self.perform_as(Activity::Sleep, 2, 0.0, &because, now),
            "speak" => {
                self.perform_as(Activity::Stand, 2, 3.0, &because, now);
                self.voice("bark", 0.7, now);
            }
            "paw" => self.perform_as(Activity::SeekAttention, 1, 6.0, &because, now),
            "spin" => self.perform_as(Activity::TailChase, 0, 3.5, &because, now),
            "roll_over" => self.perform_as(Activity::Roll, 0, 5.0, &because, now),
            "bow" => self.perform_as(Activity::PlayBow, 0, 3.0, &because, now),
            "shake" => self.perform_as(Activity::Shake, 0, 2.0, &because, now),
            "stretch" => self.perform_as(Activity::Stretch, 0, 4.0, &because, now),
            "play" => {
                self.perform_as(Activity::PlayBow, 0, 3.0, &because, now);
                self.needs.boredom = (self.needs.boredom + 0.4).min(1.0);
            }
            "look" => {
                self.perform_as(Activity::Stand, 2, 5.0, &because, now);
                self.look("screen", 4.0, now);
            }
            _ => self.puzzle(now),
        }
    }

    fn perform_as(&mut self, activity: Activity, variant: u8, secs: f64, because: &str, now: f64) {
        self.performing = true;
        self.start(Decision { activity, variant, secs, intensity: 0.8, cause: because.to_string(), considered: Vec::new() }, now);
    }

    /// A few things he is told change how he lives, not just what he knows.
    fn take_to_heart(&mut self, text: &str) {
        let said = language::normalise(text);
        let late = ["work late", "working late", "stay up late", "night owl", "شب ها کار", "شبها کار", "تا دیروقت", "دیر میخوابم"];
        if late.iter().any(|phrase| said.contains(&language::normalise(phrase))) {
            // Then his night starts two hours later too.
            self.settings.night_shift_hours = 2.0;
            self.out.push(journal("preference", json!({ "night_shift_hours": 2.0 })));
        }
    }

    /// Takes in something perceived. Ignored unless that sense is allowed and privacy mode is off.
    pub fn sense(&mut self, stimulus: Stimulus, now: f64) -> Vec<Output> {
        if self.settings.privacy_mode || self.settings.senses.get(stimulus.source()) != Some(true) {
            return Vec::new();
        }
        match stimulus {
            Stimulus::UserIdle(secs) => {
                // A running impression of how steadily you are working (settles over a couple of minutes).
                let active = if secs < 3.0 { 1.0 } else { 0.0 };
                self.busy += (active - self.busy) * 0.02;
                self.idle_secs = secs;
                if !self.user_away && secs >= AWAY_AFTER_SECS {
                    self.left(now - secs);
                } else if self.user_away && !self.locked && secs < 5.0 {
                    self.came_back(now);
                }
            }
            Stimulus::Locked(on) => {
                let was_locked = std::mem::replace(&mut self.locked, on);
                if on && !self.user_away {
                    self.left(now);
                } else if was_locked && !on && self.user_away {
                    self.came_back(now);
                }
            }
            Stimulus::Fullscreen(on) => self.fullscreen = on,
            Stimulus::Overview(on) => self.overview = on,
            Stimulus::Click { .. } => {
                // A click is activity, not content. Keep it for the context layer; it
                // must not become a screen or keyboard record.
                self.attended = 1.0;
            }
            Stimulus::Focus { app } => {
                if let Some(recording) = self.recording.as_mut() {
                    recording.observe(&app, now);
                }
                if let Some(habit) = self.patterns.focus(&app, now) {
                    // He has worked out something about how you work; for now he just perks up at it.
                    self.mood.lift(0.05, 0.1);
                    self.look("screen", 2.0, now);
                    self.out.push(journal("habit_noticed", json!({ "apps": habit.apps, "times": habit.times, "days": habit.days })));
                }
            }
            Stimulus::Notification { .. } => {
                if self.activity != Activity::Sleep && !self.user_away {
                    let secs = self.rng.range(1.5, 3.0);
                    self.look("notification", secs, now);
                }
            }
            Stimulus::Presence(here) => {
                if here && self.user_away && !self.locked {
                    self.came_back(now);
                } else if !here && !self.user_away && self.idle_secs > 60.0 {
                    // Sitting still at the keyboard looks like an empty room to the camera; typing says otherwise.
                    self.left(now);
                }
            }
            Stimulus::Sound { level } => {
                if level > 0.6 && now - self.last_startle > 20.0 {
                    self.last_startle = now;
                    self.mood.lift(0.0, 0.25 * level);
                    let dozing = self.activity == Activity::Sleep && self.needs.fatigue < 0.35 && !self.settings.asleep_by_user;
                    if dozing {
                        self.finish(now);
                        self.begin(Activity::Stand, "a noise woke him", now);
                    }
                    if self.activity != Activity::Sleep {
                        self.look("sound", 2.5, now);
                    }
                }
            }
            Stimulus::Glimpse { summary } => {
                self.glimpse = Some((now, summary));
                self.needs.boredom = (self.needs.boredom - 0.1).max(0.0);
                if self.activity != Activity::Sleep {
                    self.look("screen", 2.5, now);
                }
            }
            Stimulus::Load(level) => self.load = level,
        }
        std::mem::take(&mut self.out)
    }

    fn left(&mut self, since: f64) {
        self.user_away = true;
        self.away_since = since;
        self.out.push(journal("user_away", Value::Null));
    }

    fn came_back(&mut self, now: f64) {
        let away_secs = now - self.away_since;
        self.user_away = false;
        self.ask_patience = 1.0;
        if away_secs > AWAY_AFTER_SECS {
            self.greet_urge = 1.0;
        } else if self.activity != Activity::Sleep {
            self.look("you", 2.5, now);
        }
        self.out.push(journal("user_back", json!({ "away_secs": away_secs })));
    }

    fn look(&mut self, at: &'static str, secs: f64, now: f64) {
        self.last_glance = now + secs;
        self.out.push(Output::Look { at, secs });
    }

    /// Something changed that he should respond to: cut a rest short.
    fn reconsider(&mut self, now: f64) {
        let resting = matches!(self.activity, Activity::Stand | Activity::Sit | Activity::Lie);
        if resting && !self.settings.asleep_by_user {
            self.ends = now;
        }
    }

    pub fn snapshot(&self, now: f64) -> StateMsg {
        let maturity = self.maturity(now);
        StateMsg {
            name: self.identity.name.clone(),
            activity: self.activity,
            variant: self.variant,
            intensity: self.intensity,
            started: self.started,
            planned_secs: self.planned,
            seed: self.seed,
            x_frac: self.x_frac(),
            speed_px: self.v,
            facing: self.facing,
            hidden: self.settings.hidden_until.is_some(),
            privacy: self.settings.privacy_mode,
            quiet: self.settings.quiet,
            growth: model::growth(maturity),
            scale: self.scale(now),
            stage: model::stage(maturity),
            age_days: self.age_secs(now) / model::SECS_PER_DAY,
            size_px: self.settings.size_px,
            // Rounded so that slow drifts do not count as a new state every tick.
            valence: (self.mood.valence * 50.0).round() / 50.0,
            arousal: (self.mood.arousal * 50.0).round() / 50.0,
            sleepiness: (self.needs.fatigue * 50.0).round() / 50.0,
            being_dragged: self.dragging,
            ducked: self.locked || self.overview || (self.fullscreen && !self.settings.stay_in_fullscreen),
            user_here: !self.user_away,
            load: (self.load * 10.0).round() / 10.0,
            confusion: (self.mood.confusion * 10.0).round() / 10.0,
            food_out: self.food_out,
            water_out: self.water_out,
        }
    }

    /// Why he is doing what he is doing, for the user to read.
    pub fn explain(&self, now: f64) -> Value {
        json!({
            "activity": self.activity,
            "variant": self.variant,
            "because": self.cause,
            "for_secs": if self.started > 0.0 { now - self.started } else { 0.0 },
            "also_considered": self.considered,
            "needs": self.needs,
            "emotions": self.mood.emotions(&self.needs, &self.personality),
            "personality": self.personality,
            "favourite_spot": self.habits.favourite(),
            "you": { "away": self.user_away, "busy": (self.busy * 100.0).round() / 100.0 },
            "last_look_at_the_screen": self.glimpse.as_ref().filter(|(at, _)| now - at < 600.0).map(|(_, what)| what),
            "habits_noticed": self.patterns.habits(),
            "workflows": self.workflows.list(),
            "doing_as_asked": self.performing,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn sim(born_at: i64) -> Sim {
        Sim::new(Identity { name: "Buddy".into(), born_at }, Life::default(), Settings::default(), Body::default(), Mind::default(), 42)
    }

    /// Runs him for a while at four ticks a second, collecting what he produced.
    fn live(s: &mut Sim, from: f64, secs: f64, hour: f64) -> (f64, Vec<Output>) {
        let (mut now, mut all) = (from, Vec::new());
        while now < from + secs {
            now += 0.25;
            all.extend(s.tick(now, hour));
        }
        (now, all)
    }

    #[test]
    fn a_gap_ages_him_without_adding_experience() {
        let mut s = sim(0);
        s.tick(1000.0, 12.0);
        s.tick(1001.0, 12.0);
        assert!((s.life.awake_secs - 1.0).abs() < 1e-9);
        // The computer was off for thirty days.
        let later = 1001.0 + 30.0 * model::SECS_PER_DAY;
        let events = s.tick(later, 12.0);
        assert!(matches!(&events[0], Output::Journal(e) if e.kind == "gap"));
        assert!((s.life.awake_secs - 1.0).abs() < 1e-9, "no experience across the gap");
        assert!(s.snapshot(later).age_days > 30.0, "but he is thirty days older");
        // He is pleased to be back, not reproachful.
        let (_, after) = live(&mut s, later, 5.0, 12.0);
        assert_eq!(s.activity(), Activity::Greet, "{after:?}");
        assert!(s.needs.hunger < 0.61 && s.mood.valence >= 0.55, "{:?} {:?}", s.needs, s.mood);
    }

    #[test]
    fn a_day_of_life_stays_on_screen_and_moves_plausibly() {
        let mut s = sim(0);
        s.apply(Command::SetScreen { walk_px: 2500.0 }, 0.0);
        let (dt, mut now) = (0.05, 10.0);
        let top = s.walk_speed(now) * 3.4;
        let (mut prev_v, mut seen, mut travelled) = (0.0f64, HashSet::new(), 0.0);
        for _ in 0..(86_400.0 / dt) as usize {
            now += dt;
            s.tick(now, (now / 3600.0) % 24.0);
            let st = s.snapshot(now);
            assert!((0.0..=1.0).contains(&st.x_frac));
            assert!(st.speed_px.abs() <= top * 1.02, "never faster than his fastest run");
            // Allowed: normal acceleration, or the last sliver of speed settling as he arrives.
            assert!((st.speed_px - prev_v).abs() <= top * 0.2, "no instant starts or stops");
            travelled += st.speed_px.abs() * dt;
            prev_v = st.speed_px;
            seen.insert(st.activity);
        }
        assert!(seen.len() >= 12, "a day shows most of his repertoire: {seen:?}");
        assert!(travelled > 5000.0);
    }

    #[test]
    fn hiding_for_a_while_ends_on_its_own() {
        let mut s = sim(0);
        s.tick(100.0, 12.0);
        s.apply(Command::Hide { minutes: Some(1.0) }, 100.0);
        assert!(s.snapshot(100.0).hidden);
        let (now, events) = live(&mut s, 100.0, 70.0, 12.0);
        assert!(events.iter().any(|e| matches!(e, Output::Journal(j) if j.kind == "shown")));
        assert!(!s.snapshot(now).hidden);
    }

    #[test]
    fn user_sleep_holds_until_woken_and_dragging_keeps_his_place() {
        let mut s = sim(0);
        s.tick(1.0, 12.0);
        s.apply(Command::Sleep, 1.0);
        let (now, _) = live(&mut s, 1.0, 6.0 * 3600.0, 12.0);
        assert_eq!(s.activity(), Activity::Sleep, "even fully rested, he stays down until woken");
        s.apply(Command::Wake, now);
        assert_ne!(s.activity(), Activity::Sleep);
        s.apply(Command::DragTo { x_frac: 0.25 }, now);
        s.tick(now + 0.05, 12.0);
        s.apply(Command::DragEnd, now + 0.1);
        assert!((s.body().x_frac - 0.25).abs() < 1e-9);
        // A different monitor width keeps the same relative position.
        s.apply(Command::SetScreen { walk_px: 4000.0 }, now + 0.1);
        assert!((s.body().x_frac - 0.25).abs() < 1e-9);
    }

    #[test]
    fn he_eats_when_fed_and_is_silent_in_quiet_mode() {
        let mut s = sim(0);
        s.needs.hunger = 0.8;
        s.tick(1.0, 12.0);
        s.apply(Command::Quiet(true), 1.0);
        s.apply(Command::Feed, 1.0);
        let (_, events) = live(&mut s, 1.0, 900.0, 12.0);
        let ate = events.iter().any(|e| matches!(e, Output::Behaved(r) if r.activity == Activity::Eat));
        assert!(ate && s.needs.hunger < 0.2 && !s.snapshot(901.0).food_out, "hunger {}", s.needs.hunger);
        assert!(!events.iter().any(|e| matches!(e, Output::Vocalize { .. })), "quiet mode means no sound at all");
    }

    #[test]
    fn he_never_barks_at_night_and_rations_his_barks_by_day() {
        let mut s = sim(0);
        s.tick(1.0, 2.0);
        for i in 0..40 {
            s.voice("bark", 1.0, 1.0 + f64::from(i));
        }
        assert!(s.out.is_empty(), "nothing loud at two in the morning");
        s.tick(2.0, 14.0);
        s.out.clear();
        for i in 0..40 {
            s.voice("bark", 1.0, 2.0 + f64::from(i));
        }
        assert_eq!(s.out.len(), LOUD_SOUNDS_PER_WINDOW);
    }

    #[test]
    fn a_sense_that_is_off_or_privacy_mode_perceives_nothing() {
        let mut s = sim(0);
        s.tick(1.0, 12.0);
        s.sense(Stimulus::Sound { level: 1.0 }, 2.0);
        assert!(s.last_startle < 0.0, "the microphone is off by default");
        s.apply(Command::Sense { source: "microphone".into(), on: true }, 2.0);
        s.apply(Command::Privacy(true), 2.0);
        s.sense(Stimulus::Sound { level: 1.0 }, 3.0);
        s.sense(Stimulus::Locked(true), 3.0);
        assert!(s.last_startle < 0.0 && !s.snapshot(3.0).ducked, "privacy mode shuts every sense");
        s.apply(Command::Privacy(false), 3.0);
        let heard = s.sense(Stimulus::Sound { level: 1.0 }, 4.0);
        assert!(matches!(heard[..], [Output::Look { at: "sound", .. }]), "{heard:?}");
    }

    #[test]
    fn he_notices_you_leave_and_is_glad_when_you_return() {
        let mut s = sim(0);
        let (mut now, _) = live(&mut s, 0.0, 5.0, 12.0);
        assert!(s.snapshot(now).user_here);
        // Twelve minutes without a keypress.
        let gone = s.sense(Stimulus::UserIdle(720.0), now);
        assert!(gone.iter().any(|o| matches!(o, Output::Journal(j) if j.kind == "user_away")) && !s.snapshot(now).user_here);
        let (later, alone) = live(&mut s, now, 1800.0, 12.0);
        assert!(!alone.iter().any(|o| matches!(o, Output::Look { at: "you", .. })), "he does not look for someone who is not there");
        now = later;
        s.sense(Stimulus::UserIdle(0.5), now);
        live(&mut s, now, 3.0, 12.0);
        assert_eq!(s.activity(), Activity::Greet);
    }

    #[test]
    fn he_steps_aside_for_the_lock_screen_and_fullscreen() {
        let mut s = sim(0);
        s.tick(1.0, 12.0);
        s.sense(Stimulus::Fullscreen(true), 2.0);
        assert!(s.snapshot(2.0).ducked);
        s.apply(Command::StayInFullscreen(true), 2.0);
        assert!(!s.snapshot(2.0).ducked, "unless told to stay");
        s.sense(Stimulus::Locked(true), 3.0);
        assert!(s.snapshot(3.0).ducked && !s.snapshot(3.0).user_here);
        s.sense(Stimulus::Locked(false), 4.0);
        assert!(!s.snapshot(4.0).ducked && s.snapshot(4.0).user_here);
    }

    #[test]
    fn a_loud_noise_wakes_a_dozing_dog_but_a_quiet_one_does_not() {
        let mut s = sim(0);
        s.apply(Command::Sense { source: "microphone".into(), on: true }, 0.0);
        s.tick(1.0, 12.0);
        s.begin(Activity::Sleep, "test", 1.0);
        s.needs.fatigue = 0.2;
        s.sense(Stimulus::Sound { level: 0.3 }, 2.0);
        assert_eq!(s.activity(), Activity::Sleep);
        s.sense(Stimulus::Sound { level: 0.9 }, 3.0);
        assert_eq!(s.activity(), Activity::Stand);
    }

    #[test]
    fn he_learns_a_cue_from_praise_and_then_does_it() {
        let mut s = sim(0);
        let mut now = 1.0;
        s.tick(now, 12.0);
        let sat = |s: &mut Sim, now: &mut f64, praise: bool| {
            let mut times = 0;
            for _ in 0..30 {
                *now += 40.0;
                s.needs.fatigue = 0.1;
                s.mood.arousal = 0.3;
                s.hear(Heard::Cue("sit".into()), *now);
                if s.activity() == Activity::Sit && s.performing {
                    times += 1;
                    if praise {
                        s.hear(Heard::Praise, *now + 2.0);
                    }
                }
                s.tick(*now + 3.0, 12.0);
            }
            times
        };
        sat(&mut s, &mut now, true);
        sat(&mut s, &mut now, true);
        let trained = sat(&mut s, &mut now, false);
        assert!(trained >= 26, "after practice he sits when told: {trained}/30");
        assert!(s.training.strength("sit", "sit", &TRICKS) > 0.9);
    }

    #[test]
    fn an_unknown_word_puzzles_him_and_stop_means_stop() {
        let mut s = sim(0);
        s.tick(1.0, 12.0);
        let out = s.hear(Heard::Unknown, 2.0);
        assert!(s.snapshot(2.0).confusion > 0.9 && matches!(out[..], [Output::Look { at: "you", .. }]));
        s.begin(Activity::TailChase, "test", 3.0);
        s.hear(Heard::Stop, 4.0);
        assert_eq!(s.activity(), Activity::Sit);
        let (_, after) = live(&mut s, 4.0, 1500.0, 12.0);
        assert!(
            !after.iter().any(|o| matches!(o, Output::Behaved(r) if r.activity == Activity::TailChase && r.ts > 4.0)),
            "not again for half an hour"
        );
    }

    #[test]
    fn his_name_is_a_recall_and_becomes_familiar() {
        let mut s = sim(0);
        s.apply(Command::SetScreen { walk_px: 2000.0 }, 0.0);
        s.tick(1.0, 12.0);
        let first = s.hear(Heard::Called, 2.0);
        assert_eq!(s.life.name_calls, 1);
        assert_eq!(s.activity(), Activity::Greet);
        assert!(first.iter().any(|output| matches!(output, Output::Journal(entry) if entry.kind == "called")));
        let second = s.hear(Heard::Called, 3.0);
        assert_eq!(s.life.name_calls, 2);
        assert!(matches!(s.activity(), Activity::Walk | Activity::Sit), "the second call is a recall");
        assert!(second.iter().any(|output| matches!(output, Output::Journal(entry) if entry.kind == "called")));
    }

    #[test]
    fn he_comes_when_called_and_remembers_what_he_is_told() {
        let mut s = sim(0);
        s.apply(Command::SetScreen { walk_px: 2000.0 }, 0.0);
        s.tick(1.0, 12.0);
        s.training.teach("come", "come");
        let mut now = 1.0;
        while !(s.activity() == Activity::Walk && s.performing) {
            now += 30.0;
            s.tick(now, 12.0);
            s.hear(Heard::Cue("come".into()), now);
        }
        while s.activity() == Activity::Walk {
            now += 0.25;
            s.tick(now, 12.0);
        }
        assert!((s.body().x_frac - 0.5).abs() < 0.01 && s.activity() == Activity::Sit, "at {} doing {:?}", s.body().x_frac, s.activity());
        let out = s.hear(Heard::Remember("I prefer working late".into()), now);
        assert!(out.iter().any(|o| matches!(o, Output::Remember(text) if text == "I prefer working late")));
        assert_eq!(s.settings.night_shift_hours, 2.0, "and it changes when his night begins");
    }

    #[test]
    fn he_offers_to_finish_a_habit_and_takes_no_for_an_answer() {
        let mut s = sim(0);
        s.tick(1.0, 12.0);
        // Three days of the same routine make it a habit.
        for day in 0..3 {
            let start = 100.0 + 86_400.0 * f64::from(day);
            for (i, app) in ["terminal", "editor", "browser", "terminal", "editor", "browser", "files"].iter().enumerate() {
                s.sense(Stimulus::Focus { app: app.to_string() }, start + 30.0 * i as f64);
            }
        }
        // Day four: terminal, then the editor.
        let mut now = 100.0 + 86_400.0 * 4.0;
        s.tick(now - 1.0, 12.0);
        s.sense(Stimulus::Focus { app: "terminal".into() }, now);
        s.sense(Stimulus::Focus { app: "editor".into() }, now + 30.0);
        now += 31.0;
        let mut offers = Vec::new();
        for _ in 0..40 {
            now += 0.25;
            offers.extend(s.tick(now, 12.0).into_iter().filter(|o| matches!(o, Output::Offer { .. })));
            if !offers.is_empty() {
                break;
            }
        }
        assert_eq!(offers, vec![Output::Offer { habit: "terminal > editor > browser".into(), app: "browser".into(), standing: false }]);
        assert_eq!(s.activity(), Activity::Invite);

        // "Don't help me with this": he settles, and never offers that again.
        s.offer_answered(Some("terminal > editor > browser"), Answer::Never, now);
        assert_eq!(s.activity(), Activity::Sit);
        assert!(s.mood.valence > 0.0, "and he is not crushed by it");
        let later = now + 7200.0;
        s.tick(later - 1.0, 12.0);
        s.sense(Stimulus::Focus { app: "terminal".into() }, later);
        s.sense(Stimulus::Focus { app: "editor".into() }, later + 30.0);
        let (_, after) = live(&mut s, later + 31.0, 20.0, 12.0);
        assert!(!after.iter().any(|o| matches!(o, Output::Offer { .. })));
    }

    #[test]
    fn where_he_is_petted_becomes_his_spot() {
        let mut s = sim(0);
        s.tick(1.0, 12.0);
        assert_eq!(s.habits.favourite(), None);
        s.apply(Command::DragTo { x_frac: 0.2 }, 1.0);
        s.apply(Command::DragEnd, 1.1);
        for i in 0..4 {
            s.apply(Command::Pet, 2.0 + f64::from(i));
        }
        let spot = s.habits.favourite().expect("a preference has formed");
        assert!((spot - 0.2).abs() < 0.06);
    }

    #[test]
    fn focus_events_are_recorded_while_a_recording_is_active() {
        let mut s = sim(0);
        s.start_workflow_recording("morning".into(), 0.0).unwrap();
        s.sense(Stimulus::Focus { app: "terminal".into() }, 0.0);
        s.sense(Stimulus::Focus { app: "editor".into() }, 30.0);
        let workflow = s.stop_workflow_recording(40.0).unwrap();
        assert_eq!(workflow.apps, vec!["terminal", "editor"]);
        assert_eq!(s.workflows().len(), 1);
    }

    #[test]
    fn a_recorded_workflow_survives_a_restart() {
        let mut s = sim(0);
        s.start_workflow_recording("morning".into(), 0.0).unwrap();
        s.sense(Stimulus::Focus { app: "terminal".into() }, 0.0);
        s.sense(Stimulus::Focus { app: "editor".into() }, 30.0);
        s.stop_workflow_recording(40.0).unwrap();
        let mind = s.mind();
        let restored = Sim::new(s.identity.clone(), s.life.clone(), s.settings.clone(), s.body(), mind, 42);
        assert_eq!(restored.workflows().len(), 1);
        assert_eq!(restored.workflows()[0].apps, vec!["terminal", "editor"]);
    }

    #[test]
    fn an_inside_app_recording_captures_actions_text_and_manual_steps() {
        let mut s = sim(0);
        s.start_action_recording("press".into(), 0.0).unwrap();
        s.capture_action_step(WorkflowStep::Activate {
            app: "local.buddy.A11yProbe".into(),
            frame: "Buddy A11y Probe".into(),
            role: "button".into(),
            name: "Press me".into(),
        })
        .unwrap();
        s.capture_action_step(WorkflowStep::TypeText {
            app: "local.buddy.A11yProbe".into(),
            frame: "Buddy A11y Probe".into(),
            role: "text".into(),
            name: "".into(),
            text: "hello".into(),
        })
        .unwrap();
        s.capture_action_step(WorkflowStep::Manual { app: "telegram-desktop".into(), note: "send the file".into() }).unwrap();
        let workflow = s.stop_action_recording(1.0).unwrap();
        assert_eq!(workflow.steps.len(), 5);
        assert!(matches!(workflow.steps[0], WorkflowStep::OpenApp { .. }));
        assert_eq!(workflow.apps, vec!["local.buddy.A11yProbe", "telegram-desktop"]);
        assert!(Workflows::has_actions(&workflow));
        assert!(Workflows::needs_accessibility(&workflow));
        assert!(!s.has_action_recording());
    }
}
