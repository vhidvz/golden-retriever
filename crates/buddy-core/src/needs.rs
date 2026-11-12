//! What his body wants. Each need runs from 0 (satisfied) to 1 (urgent) and
//! changes with time and with what he is doing.

use crate::model::{Effort, Stage};
use crate::personality::Personality;
use serde::{Deserialize, Serialize};

const HOUR: f64 = 3600.0;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Needs {
    pub fatigue: f64,
    pub hunger: f64,
    pub thirst: f64,
    /// The urge to play or explore.
    pub boredom: f64,
    /// The wish for company.
    pub social: f64,
    pub itch: f64,
    /// Builds up while lying still; stretching and walking clear it.
    pub stiffness: f64,
}

impl Default for Needs {
    fn default() -> Self {
        Self { fatigue: 0.2, hunger: 0.3, thirst: 0.3, boredom: 0.3, social: 0.3, itch: 0.1, stiffness: 0.1 }
    }
}

/// How far a need is past "noticeable" (0.5), as 0..=1.
pub fn excess(need: f64) -> f64 {
    ((need - 0.5) / 0.5).clamp(0.0, 1.0)
}

impl Needs {
    pub fn advance(&mut self, dt: f64, effort: Effort, traits: &Personality, stage: Stage) {
        // Young and old dogs tire sooner; an energetic temperament tires later.
        let tiring = match stage {
            Stage::Puppy => 2.2,
            Stage::Adolescent => 1.3,
            Stage::YoungAdult => 1.0,
            Stage::Mature => 1.25,
        } * (1.3 - 0.6 * traits.energy);
        let restless = (0.6 + 0.8 * traits.playfulness) * if stage == Stage::Puppy { 1.3 } else { 1.0 };
        // Change per hour of: fatigue, boredom, stiffness, extra thirst.
        let (fatigue, boredom, stiffness, sweat) = match effort {
            Effort::Asleep => (-0.33, -1.0, 0.6, 0.0),
            Effort::Resting => (0.09 * tiring, 2.2 * restless, 0.9, 0.0),
            Effort::Standing => (0.13 * tiring, 1.6 * restless, -0.5, 0.0),
            Effort::Moving => (0.30 * tiring, -40.0, -10.0, 0.1),
            Effort::Playing => (1.05 * tiring, -120.0, -10.0, 0.4),
        };
        let awake = if effort == Effort::Asleep { 0.25 } else { 1.0 };
        let hours = dt / HOUR;
        let step = |need: &mut f64, per_hour: f64| *need = (*need + per_hour * hours).clamp(0.0, 1.0);
        step(&mut self.fatigue, fatigue);
        step(&mut self.boredom, boredom);
        step(&mut self.stiffness, stiffness);
        // Paced for two meals a day and water every few hours.
        step(&mut self.hunger, 1.0 / 16.0);
        step(&mut self.thirst, 1.0 / 10.0 + sweat);
        step(&mut self.social, (0.25 + 0.3 * traits.sociability) * awake);
        step(&mut self.itch, 1.1 * awake);
    }

    /// Time he did not live through (computer off or suspended). He comes back
    /// rested and glad to see you; absence is never held against the user, so
    /// hunger, thirst and loneliness stop short of urgent.
    pub fn rest_through_gap(&mut self, secs: f64) {
        let hours = secs / HOUR;
        self.fatigue = (self.fatigue - 0.33 * hours).max(0.05);
        self.hunger = (self.hunger + hours / 16.0).min(self.hunger.max(0.6));
        self.thirst = (self.thirst + hours / 10.0).min(self.thirst.max(0.6));
        self.social = (self.social + hours / 4.0).min(self.social.max(0.7));
        self.boredom = self.boredom.min(0.4);
        self.itch = (self.itch + 0.3 * hours).min(self.itch.max(0.5));
        self.stiffness = (self.stiffness + 0.6 * hours).min(0.9);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absence_never_leaves_him_desperate() {
        let mut n = Needs::default();
        n.rest_through_gap(30.0 * 24.0 * HOUR);
        assert!(n.hunger <= 0.6 && n.thirst <= 0.6 && n.social <= 0.7, "{n:?}");
        assert!(n.fatigue <= 0.05 + 1e-9);
        // But a need that was already higher is not reduced by being away.
        let mut hungry = Needs { hunger: 0.9, ..Needs::default() };
        hungry.rest_through_gap(HOUR);
        assert!(hungry.hunger >= 0.9);
    }

    #[test]
    fn sleeping_restores_and_play_tires() {
        let traits = Personality::default();
        let mut n = Needs { fatigue: 0.8, ..Needs::default() };
        n.advance(2.0 * HOUR, Effort::Asleep, &traits, Stage::YoungAdult);
        assert!(n.fatigue < 0.2);
        let before = n.fatigue;
        n.advance(600.0, Effort::Playing, &traits, Stage::YoungAdult);
        assert!(n.fatigue > before + 0.15 && n.boredom == 0.0, "{n:?}");
    }
}
