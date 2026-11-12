//! Advances Buddy through time: his needs and mood drift, he decides what to
//! do (see `behavior`), carries it out, and remembers it.

use crate::behavior::{self, Considered, Context, Decision, Memory, Record};
use crate::model::{self, Activity, Body, Identity, Life, Settings};
use crate::mood::Mood;
use crate::needs::Needs;
use crate::patterns::Patterns;
use crate::perception::Stimulus;
use crate::personality::{Personality, drift};
use crate::proto::StateMsg;
use crate::rng::Rng;
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
    /// He makes a sound.
    Vocalize {
        kind: &'static str,
        intensity: f64,
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
            rng,
            out: Vec::new(),
        }
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

        if self.dragging {
            self.v = 0.0;
        } else if self.settings.asleep_by_user {
            if self.activity != Activity::Sleep {
                self.finish(now);
                self.begin(Activity::Sleep, "you sent him to sleep", now);
            }
        } else if self.greet_urge > 0.5 && self.activity != Activity::Greet {
            self.finish(now);
            self.begin(Activity::Greet, "happy to see you", now);
        } else {
            match self.activity {
                Activity::Walk | Activity::Zoomies => self.step_move(now, dt),
                Activity::Sleep => {
                    let night = night_pressure(hour);
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
            night: night_pressure(self.hour) * (1.0 - self.attended),
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
            Activity::PlayBow => {
                if self.rng.chance(0.4) {
                    self.voice("bark", 0.5, now);
                }
            }
            Activity::SeekAttention => {
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
        if self.settings.quiet {
            return;
        }
        if matches!(kind, "bark" | "whine") {
            while self.loud_sounds.front().is_some_and(|t| now - t > SOUND_WINDOW_SECS) {
                self.loud_sounds.pop_front();
            }
            if night_pressure(self.hour) > 0.3 || self.loud_sounds.len() >= LOUD_SOUNDS_PER_WINDOW {
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
            Command::Rename(name) => {
                self.identity.name = name;
                self.out.push(journal("setting", json!({ "name": self.identity.name })));
            }
        }
        std::mem::take(&mut self.out)
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
            Stimulus::Focus { app } => {
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
}
