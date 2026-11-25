//! Noticing how the user works: which applications follow which, again and again.
//!
//! This is the first, coarse layer of pattern recognition. It sees only
//! application identifiers and the order they come to the front in, never
//! their contents. Sequences that keep recurring are what later phases can
//! offer to help with.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

/// Shortest time an application must stay in front to count as a step (skips alt-tab fly-bys).
const MIN_DWELL_SECS: f64 = 2.0;
/// A pause this long ends a sequence: what comes after is a new piece of work.
const BREAK_SECS: f64 = 15.0 * 60.0;
const SEQUENCE_LEN: usize = 3;
const MAX_TRACKED: usize = 400;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Seen {
    pub times: u32,
    /// Distinct days it was seen on; a habit recurs across days, a burst does not.
    pub days: u32,
    pub last_day: i64,
    pub last_seen: f64,
    /// How the user has answered when he offered to help with this.
    #[serde(default)]
    pub accepted: u32,
    #[serde(default)]
    pub declined: u32,
    /// "Always do that": he may go ahead without asking.
    #[serde(default)]
    pub standing_yes: bool,
    /// "Don't help me with this", or declined three times: he never offers again.
    #[serde(default)]
    pub never: bool,
    #[serde(default)]
    pub last_offer: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Patterns {
    /// "app > app > app" to how often that sequence has occurred.
    sequences: BTreeMap<String, Seen>,
    #[serde(skip)]
    recent: VecDeque<String>,
    #[serde(skip)]
    pending: Option<(String, f64)>,
    #[serde(skip)]
    last_step: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Recurring {
    pub apps: Vec<String>,
    pub times: u32,
    pub days: u32,
}

impl Patterns {
    /// An application came to the front at `now`. Returns a sequence that has
    /// just recurred often enough to count as a habit, the moment it does.
    pub fn focus(&mut self, app: &str, now: f64) -> Option<Recurring> {
        // The previous application becomes a step only once we know it stayed a while.
        let mut noticed = None;
        if let Some((previous, since)) = self.pending.take().filter(|(previous, since)| now - since >= MIN_DWELL_SECS && previous != app) {
            noticed = self.step(previous, since);
        }
        self.pending = Some((app.to_string(), now));
        noticed
    }

    fn step(&mut self, app: String, at: f64) -> Option<Recurring> {
        if at - self.last_step > BREAK_SECS {
            self.recent.clear();
        }
        self.last_step = at;
        if self.recent.back() == Some(&app) {
            return None;
        }
        self.recent.push_back(app);
        if self.recent.len() > SEQUENCE_LEN {
            self.recent.pop_front();
        }
        if self.recent.len() < SEQUENCE_LEN {
            return None;
        }
        let key = self.recent.iter().cloned().collect::<Vec<_>>().join(" > ");
        let day = (at / 86_400.0) as i64;
        let seen = self.sequences.entry(key.clone()).or_default();
        let was_habit = seen.is_habit();
        seen.times += 1;
        if seen.last_day != day || seen.days == 0 {
            seen.days += 1;
            seen.last_day = day;
        }
        seen.last_seen = at;
        // Announced once, at the moment it first qualifies.
        let result = (!was_habit && seen.is_habit()).then(|| Recurring {
            apps: key.split(" > ").map(str::to_string).collect(),
            times: seen.times,
            days: seen.days,
        });
        self.forget_the_rarest();
        result
    }

    fn forget_the_rarest(&mut self) {
        while self.sequences.len() > MAX_TRACKED {
            let Some(rarest) = self
                .sequences
                .iter()
                .min_by(|a, b| (a.1.times, a.1.last_seen).partial_cmp(&(b.1.times, b.1.last_seen)).unwrap())
                .map(|(k, _)| k.clone())
            else {
                return;
            };
            self.sequences.remove(&rarest);
        }
    }

    /// Sequences seen at least `HABIT_TIMES` times on at least `HABIT_DAYS` different days, most frequent first.
    pub fn habits(&self) -> Vec<Recurring> {
        let mut found: Vec<Recurring> = self
            .sequences
            .iter()
            .filter(|(_, s)| s.is_habit())
            .map(|(k, s)| Recurring { apps: k.split(" > ").map(str::to_string).collect(), times: s.times, days: s.days })
            .collect();
        found.sort_by_key(|habit| std::cmp::Reverse(habit.times));
        found
    }

    /// If the last two applications are how one of his noticed habits begins, the
    /// habit and the application that usually comes third.
    pub fn anticipate(&self) -> Option<(String, String)> {
        if self.recent.len() < 2 {
            return None;
        }
        let begun = format!("{} > {} > ", self.recent[self.recent.len() - 2], self.recent[self.recent.len() - 1]);
        self.sequences
            .iter()
            .filter(|(key, seen)| key.starts_with(&begun) && seen.is_habit() && !seen.never)
            .max_by_key(|(_, seen)| seen.times)
            .map(|(key, _)| (key.clone(), key[begun.len()..].to_string()))
    }

    /// Whether this application is part of any sequence he has learned as a habit.
    pub fn in_habit(&self, app: &str) -> bool {
        self.sequences.iter().any(|(key, seen)| seen.is_habit() && key.split(" > ").any(|part| part == app))
    }

    /// The record for one habit, by its key ("app > app > app").
    pub fn habit_mut(&mut self, key: &str) -> Option<&mut Seen> {
        self.sequences.get_mut(key)
    }

    pub fn habit(&self, key: &str) -> Option<&Seen> {
        self.sequences.get(key)
    }

    /// The application currently in front, once it has been there long enough to count.
    pub fn settle(&mut self, now: f64) -> Option<Recurring> {
        let (app, since) = self.pending.clone()?;
        if now - since < MIN_DWELL_SECS || self.recent.back() == Some(&app) {
            return None;
        }
        self.step(app, since)
    }

    pub fn tracked(&self) -> usize {
        self.sequences.len()
    }
}

pub const HABIT_TIMES: u32 = 5;
pub const HABIT_DAYS: u32 = 2;

impl Seen {
    fn is_habit(&self) -> bool {
        self.times >= HABIT_TIMES && self.days >= HABIT_DAYS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brings each application to the front in turn, 30 seconds apart, and reports any habit noticed.
    fn work(p: &mut Patterns, start: f64, apps: &[&str]) -> Vec<Recurring> {
        let mut noticed = Vec::new();
        for (i, app) in apps.iter().enumerate() {
            noticed.extend(p.focus(app, start + 30.0 * i as f64));
        }
        noticed
    }

    #[test]
    fn a_sequence_repeated_across_days_becomes_a_habit() {
        let mut p = Patterns::default();
        let routine = ["terminal", "editor", "browser", "terminal", "editor", "browser", "files"];
        let day = 86_400.0;
        assert!(work(&mut p, 1000.0, &routine).is_empty(), "twice in one sitting is not a habit");
        assert!(p.habits().is_empty());
        let mut noticed = Vec::new();
        for d in 1..4 {
            noticed.extend(work(&mut p, 1000.0 + day * f64::from(d), &routine));
        }
        let habits = p.habits();
        assert!(habits.iter().any(|h| h.apps == ["terminal", "editor", "browser"]), "{habits:?}");
        assert!(habits.iter().all(|h| h.days >= HABIT_DAYS && h.times >= HABIT_TIMES));
        assert_eq!(noticed.len(), habits.len(), "each habit is announced exactly once, when it forms: {noticed:?}");
    }

    #[test]
    fn a_learned_routine_can_be_recognised_by_each_application_in_it() {
        let mut p = Patterns::default();
        let day = 86_400.0;
        for d in 0..6 {
            work(&mut p, 1000.0 + day * f64::from(d), &["terminal", "editor", "browser"]);
        }
        assert!(p.in_habit("terminal") && p.in_habit("editor") && p.in_habit("browser"));
        assert!(!p.in_habit("chat"));
    }

    #[test]
    fn fly_bys_and_long_breaks_do_not_make_sequences() {
        let mut p = Patterns::default();
        // Alt-tabbing through an application for half a second is not a step.
        p.focus("terminal", 0.0);
        p.focus("editor", 30.0);
        p.focus("chat", 60.0);
        p.focus("browser", 60.5);
        p.focus("files", 90.0);
        p.focus("terminal", 120.0);
        assert!(p.sequences.keys().all(|k| !k.contains("chat")), "{:?}", p.sequences.keys());
        // After a long break the earlier steps do not chain onto the new ones.
        let before = p.tracked();
        p.focus("editor", 120.0 + 3600.0);
        p.focus("browser", 150.0 + 3600.0);
        assert_eq!(p.tracked(), before + 1, "only the sequence ending before the break was completed");
    }

    #[test]
    fn the_start_of_a_habit_lets_him_anticipate_the_rest() {
        let mut p = Patterns::default();
        for d in 0..3 {
            work(&mut p, 86_400.0 * f64::from(d), &["terminal", "editor", "browser", "terminal", "editor", "browser", "files"]);
        }
        // A new day: terminal, then editor. The browser has always come next.
        let day = 86_400.0 * 5.0;
        p.focus("terminal", day);
        p.focus("editor", day + 30.0);
        assert_eq!(p.anticipate(), None, "the editor has only just come to the front");
        p.settle(day + 40.0);
        let (habit, next) = p.anticipate().expect("he sees it coming");
        assert_eq!((habit.as_str(), next.as_str()), ("terminal > editor > browser", "browser"));
        p.habit_mut(&habit).unwrap().never = true;
        assert_eq!(p.anticipate(), None, "told not to help with this, he does not offer");
    }

    #[test]
    fn what_was_learned_survives_being_saved() {
        let mut p = Patterns::default();
        for d in 0..3 {
            work(&mut p, 86_400.0 * f64::from(d), &["a", "b", "c", "a", "b", "c", "d"]);
        }
        let restored: Patterns = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(restored.habits(), p.habits());
    }
}
