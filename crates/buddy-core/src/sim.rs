//! Advances Buddy through time.
//!
//! Phase 1 scope: time accounting (age vs. experience), locomotion with real
//! acceleration, and a deliberately simple activity picker so there is
//! something alive to render. The picker is a stand-in: the behaviour engine
//! (needs, emotions, habituation) replaces `choose_next` without touching the
//! rest of this file.

use crate::model::{self, Activity, Body, Identity, Life, Settings, Stage};
use crate::proto::StateMsg;
use serde_json::{Value, json};

/// A tick longer than this is treated as time Buddy did not live through
/// (suspend, a stalled process, a clock change): he ages, but gains no experience
/// and nothing is simulated across the gap.
pub const MAX_STEP_SECS: f64 = 5.0;

/// Height of a newborn relative to the adult.
const NEWBORN_SCALE: f64 = 0.45;

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
    Pet,
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
}

/// Something worth remembering; the daemon writes these to the journal.
#[derive(Clone, Debug, PartialEq)]
pub struct JournalEntry {
    pub kind: &'static str,
    pub data: Value,
}

fn entry(kind: &'static str, data: Value) -> JournalEntry {
    JournalEntry { kind, data }
}

/// xorshift64*: small, seedable, good enough for behaviour variety and reproducible in tests.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next()
    }
}

pub struct Sim {
    pub identity: Identity,
    pub life: Life,
    pub settings: Settings,
    facing: i8,
    activity: Activity,
    activity_ends: f64,
    target_px: Option<f64>,
    x_px: f64,
    v: f64,
    walk_px: f64,
    last_tick: Option<f64>,
    excitement: f64,
    dragging: bool,
    rng: Rng,
}

impl Sim {
    pub fn new(identity: Identity, life: Life, settings: Settings, body: Body, seed: u64) -> Self {
        let walk_px = 1000.0;
        Self {
            identity,
            life,
            settings,
            facing: if body.facing < 0 { -1 } else { 1 },
            activity: Activity::Stand,
            activity_ends: 0.0,
            target_px: None,
            x_px: body.x_frac.clamp(0.0, 1.0) * walk_px,
            v: 0.0,
            walk_px,
            last_tick: None,
            excitement: 0.0,
            dragging: false,
            rng: Rng(seed | 1),
        }
    }

    pub fn body(&self) -> Body {
        Body {
            x_frac: self.x_frac(),
            facing: self.facing,
        }
    }

    pub fn activity(&self) -> Activity {
        self.activity
    }

    pub fn is_moving(&self) -> bool {
        self.v.abs() > 0.5 || self.dragging
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

    fn walk_speed(&self, now: f64) -> f64 {
        0.55 * self.settings.size_px * self.scale(now)
    }

    pub fn tick(&mut self, now: f64) -> Vec<JournalEntry> {
        let mut out = Vec::new();
        let dt = self.last_tick.map_or(0.0, |prev| now - prev);
        self.last_tick = Some(now);
        self.life.last_seen = now as i64;
        if !(0.0..=MAX_STEP_SECS).contains(&dt) {
            self.v = 0.0;
            out.push(entry("gap", json!({ "secs": dt })));
            return out;
        }
        self.life.awake_secs += dt;

        let hide_expired = self
            .settings
            .hidden_until
            .is_some_and(|until| until != i64::MAX && now as i64 >= until);
        if hide_expired {
            self.settings.hidden_until = None;
            out.push(entry("shown", json!({ "reason": "timer" })));
        }
        self.excitement *= (-dt / 4.0).exp();

        if self.dragging {
            self.v = 0.0;
        } else if self.settings.asleep_by_user {
            self.activity = Activity::Sleep;
            self.target_px = None;
            self.v = 0.0;
        } else if self.activity == Activity::Walk {
            self.step_walk(now, dt);
        } else if now >= self.activity_ends {
            self.choose_next(now);
        }
        out
    }

    /// Accelerate towards walking speed, then brake so he stops on the target
    /// instead of snapping to it.
    fn step_walk(&mut self, now: f64, dt: f64) {
        let Some(target) = self.target_px else {
            self.rest(Activity::Stand, now, 1.0);
            return;
        };
        let vmax = self.walk_speed(now);
        let accel = vmax * 2.5;
        let dist = target - self.x_px;
        let dir = if dist >= 0.0 { 1.0 } else { -1.0 };
        // The fastest speed from which he can still stop on the target, judged from
        // where this step will leave him so a coarse tick does not overshoot.
        let ahead = (dist.abs() - self.v.abs() * dt).max(0.0);
        let wanted = vmax.min((2.0 * accel * ahead).sqrt()) * dir;
        self.v += (wanted - self.v).clamp(-accel * dt, accel * dt);
        self.x_px += self.v * dt;
        if self.v.abs() > 1.0 {
            self.facing = if self.v > 0.0 { 1 } else { -1 };
        }
        let remaining = (target - self.x_px) * dir;
        if remaining <= 0.3 {
            self.x_px = target;
            self.target_px = None;
            let pause = self.rng.range(1.5, 4.0);
            self.rest(Activity::Stand, now, pause);
        }
        self.x_px = self.x_px.clamp(0.0, self.walk_px);
    }

    fn rest(&mut self, activity: Activity, now: f64, secs: f64) {
        self.activity = activity;
        self.activity_ends = now + secs;
        self.v = 0.0;
    }

    /// Placeholder activity picker (see module docs).
    fn choose_next(&mut self, now: f64) {
        let puppy = model::stage(self.maturity(now)) == Stage::Puppy;
        let options: [(Activity, f64); 5] = if puppy {
            [
                (Activity::Walk, 5.0),
                (Activity::Stand, 2.0),
                (Activity::Sit, 2.0),
                (Activity::Lie, 1.5),
                (Activity::Sleep, 1.5),
            ]
        } else {
            [
                (Activity::Walk, 3.0),
                (Activity::Stand, 2.0),
                (Activity::Sit, 2.5),
                (Activity::Lie, 2.5),
                (Activity::Sleep, 1.5),
            ]
        };
        // Never pick the activity he is already doing.
        let total: f64 = options
            .iter()
            .filter(|(a, _)| *a != self.activity)
            .map(|(_, w)| w)
            .sum();
        let mut roll = self.rng.next() * total;
        let mut next = Activity::Stand;
        for (a, w) in options.iter().filter(|(a, _)| *a != self.activity) {
            next = *a;
            if roll < *w {
                break;
            }
            roll -= w;
        }
        let pace = if puppy { 0.6 } else { 1.0 };
        match next {
            Activity::Walk => {
                // Somewhere at least an eighth of the screen away.
                let mut target = self.rng.range(0.0, self.walk_px);
                if (target - self.x_px).abs() < 0.125 * self.walk_px {
                    target = if self.x_px < self.walk_px / 2.0 {
                        self.x_px + 0.3 * self.walk_px
                    } else {
                        self.x_px - 0.3 * self.walk_px
                    };
                }
                self.target_px = Some(target.clamp(0.0, self.walk_px));
                self.activity = Activity::Walk;
            }
            Activity::Stand => {
                let secs = self.rng.range(3.0, 8.0);
                self.rest(Activity::Stand, now, secs * pace);
            }
            Activity::Sit => {
                let secs = self.rng.range(8.0, 25.0);
                self.rest(Activity::Sit, now, secs * pace);
            }
            Activity::Lie => {
                let secs = self.rng.range(15.0, 60.0);
                self.rest(Activity::Lie, now, secs * pace);
            }
            Activity::Sleep => {
                let secs = self.rng.range(60.0, 300.0);
                self.rest(Activity::Sleep, now, secs);
            }
        }
    }

    pub fn apply(&mut self, cmd: Command, now: f64) -> Vec<JournalEntry> {
        let mut out = Vec::new();
        match cmd {
            Command::Hide { minutes } => {
                self.settings.hidden_until = Some(match minutes {
                    Some(m) if m > 0.0 => (now + m * 60.0) as i64,
                    _ => i64::MAX,
                });
                out.push(entry("hidden", json!({ "minutes": minutes })));
            }
            Command::Show => {
                if self.settings.hidden_until.take().is_some() {
                    out.push(entry("shown", json!({ "reason": "user" })));
                }
            }
            Command::Sleep => {
                self.settings.asleep_by_user = true;
                self.activity = Activity::Sleep;
                self.target_px = None;
                self.v = 0.0;
                out.push(entry("sent_to_sleep", Value::Null));
            }
            Command::Wake => {
                self.settings.asleep_by_user = false;
                self.rest(Activity::Stand, now, 2.0);
                out.push(entry("woken", Value::Null));
            }
            Command::Privacy(on) => {
                if self.settings.privacy_mode != on {
                    self.settings.privacy_mode = on;
                    out.push(entry("privacy_mode", json!({ "on": on })));
                }
            }
            Command::Pet => {
                self.life.pets += 1;
                self.excitement = (self.excitement + 0.6).min(1.0);
                if self.activity == Activity::Sleep && !self.settings.asleep_by_user {
                    self.rest(Activity::Stand, now, 3.0);
                }
                out.push(entry("petted", json!({ "total": self.life.pets })));
            }
            Command::DragTo { x_frac } => {
                self.dragging = true;
                self.target_px = None;
                self.v = 0.0;
                self.x_px = x_frac.clamp(0.0, 1.0) * self.walk_px;
                if self.activity == Activity::Walk {
                    self.activity = Activity::Stand;
                }
            }
            Command::DragEnd => {
                if self.dragging {
                    self.dragging = false;
                    if !self.settings.asleep_by_user && self.activity != Activity::Sleep {
                        self.rest(Activity::Stand, now, 2.0);
                    }
                    out.push(entry("moved_by_user", json!({ "x_frac": self.x_frac() })));
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
                out.push(entry(
                    "setting",
                    json!({ "adult_days": self.settings.adult_days }),
                ));
            }
            Command::SetSize(px) => {
                self.settings.size_px = px.clamp(120.0, 900.0);
                out.push(entry(
                    "setting",
                    json!({ "size_px": self.settings.size_px }),
                ));
            }
            Command::Rename(name) => {
                self.identity.name = name;
                out.push(entry("setting", json!({ "name": self.identity.name })));
            }
        }
        out
    }

    pub fn snapshot(&self, now: f64) -> StateMsg {
        let maturity = self.maturity(now);
        StateMsg {
            name: self.identity.name.clone(),
            activity: self.activity,
            x_frac: self.x_frac(),
            speed_px: self.v,
            facing: self.facing,
            hidden: self.settings.hidden_until.is_some(),
            privacy: self.settings.privacy_mode,
            growth: model::growth(maturity),
            scale: self.scale(now),
            stage: model::stage(maturity),
            age_days: self.age_secs(now) / model::SECS_PER_DAY,
            size_px: self.settings.size_px,
            excitement: self.excitement,
            being_dragged: self.dragging,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn sim(born_at: i64) -> Sim {
        Sim::new(
            Identity {
                name: "Buddy".into(),
                born_at,
            },
            Life::default(),
            Settings::default(),
            Body::default(),
            42,
        )
    }

    #[test]
    fn a_gap_ages_him_without_adding_experience() {
        let mut s = sim(0);
        s.tick(1000.0);
        s.tick(1001.0);
        assert!((s.life.awake_secs - 1.0).abs() < 1e-9);
        // The computer was off for thirty days.
        let later = 1001.0 + 30.0 * model::SECS_PER_DAY;
        let events = s.tick(later);
        assert_eq!(events[0].kind, "gap");
        assert!(
            (s.life.awake_secs - 1.0).abs() < 1e-9,
            "no experience across the gap"
        );
        assert!(
            s.snapshot(later).age_days > 30.0,
            "but he is thirty days older"
        );
    }

    #[test]
    fn a_day_of_life_stays_on_screen_and_moves_plausibly() {
        let mut s = sim(0);
        s.apply(Command::SetScreen { walk_px: 2500.0 }, 0.0);
        let (dt, mut now) = (0.05, 10.0);
        let vmax = s.walk_speed(now);
        let (mut prev_v, mut seen, mut travelled) = (0.0f64, HashSet::new(), 0.0);
        for _ in 0..(86_400.0 / dt) as usize {
            now += dt;
            s.tick(now);
            let st = s.snapshot(now);
            assert!((0.0..=1.0).contains(&st.x_frac));
            assert!(
                st.speed_px.abs() <= vmax * 1.02,
                "never faster than his walk"
            );
            // Allowed: normal acceleration, or the last sliver of speed settling as he arrives.
            assert!(
                (st.speed_px - prev_v).abs() <= vmax * 0.2,
                "no instant starts or stops"
            );
            travelled += st.speed_px.abs() * dt;
            prev_v = st.speed_px;
            seen.insert(format!("{:?}", st.activity));
        }
        assert_eq!(
            seen.len(),
            5,
            "every placeholder activity shows up in a day: {seen:?}"
        );
        assert!(travelled > 5000.0);
    }

    #[test]
    fn hiding_for_a_while_ends_on_its_own() {
        let mut s = sim(0);
        s.tick(100.0);
        s.apply(Command::Hide { minutes: Some(1.0) }, 100.0);
        assert!(s.snapshot(100.0).hidden);
        let mut now = 100.0;
        let mut reappeared = false;
        while now < 170.0 {
            now += 1.0;
            reappeared |= s.tick(now).iter().any(|e| e.kind == "shown");
        }
        assert!(reappeared && !s.snapshot(now).hidden);
    }

    #[test]
    fn user_sleep_holds_until_woken_and_dragging_keeps_his_place() {
        let mut s = sim(0);
        s.tick(1.0);
        s.apply(Command::Sleep, 1.0);
        let mut now = 1.0;
        for _ in 0..20_000 {
            now += 0.5;
            s.tick(now);
            assert_eq!(s.activity(), Activity::Sleep);
        }
        s.apply(Command::Wake, now);
        s.apply(Command::DragTo { x_frac: 0.25 }, now);
        s.tick(now + 0.05);
        s.apply(Command::DragEnd, now + 0.1);
        assert!((s.body().x_frac - 0.25).abs() < 1e-9);
        // A different monitor width keeps the same relative position.
        s.apply(Command::SetScreen { walk_px: 4000.0 }, now + 0.1);
        assert!((s.body().x_frac - 0.25).abs() < 1e-9);
    }
}
