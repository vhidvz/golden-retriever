//! How he feels. Two continuous variables carry the state (how pleasant, how
//! activated); the named emotions shown to the user are read off them and his
//! needs, so every expression traces back to a cause.

use crate::model::Effort;
use crate::needs::{Needs, excess};
use crate::personality::Personality;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Mood {
    /// -1 (unpleasant) ..= 1 (pleasant).
    pub valence: f64,
    /// 0 (asleep) ..= 1 (wild).
    pub arousal: f64,
    /// Rises when he cannot make sense of something; fades in seconds.
    pub confusion: f64,
}

impl Default for Mood {
    fn default() -> Self {
        Self { valence: 0.5, arousal: 0.3, confusion: 0.0 }
    }
}

/// Named emotions, each 0..=1.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Emotions {
    pub happiness: f64,
    pub excitement: f64,
    pub curiosity: f64,
    pub relaxation: f64,
    pub sleepiness: f64,
    pub alertness: f64,
    pub confusion: f64,
    pub playfulness: f64,
    pub loneliness: f64,
    pub contentment: f64,
}

impl Mood {
    pub fn advance(&mut self, dt: f64, needs: &Needs, effort: Effort) {
        let discomfort = 0.30 * excess(needs.hunger) + 0.20 * excess(needs.thirst) + 0.30 * excess(needs.social) + 0.10 * needs.itch;
        // Unmet needs lower his mood only mildly: he is never made miserable to pressure the user.
        let settled = (0.55 - 0.7 * discomfort).max(-0.2);
        self.valence += (settled - self.valence) * (1.0 - (-dt / 600.0).exp());
        let activation = match effort {
            Effort::Asleep => 0.05,
            Effort::Resting => 0.22,
            Effort::Standing => 0.38,
            Effort::Moving => 0.50,
            Effort::Playing => 0.90,
        };
        self.arousal += (activation - self.arousal) * (1.0 - (-dt / 25.0).exp());
        self.confusion *= (-dt / 20.0).exp();
    }

    /// Something good happened.
    pub fn lift(&mut self, valence: f64, arousal: f64) {
        self.valence = (self.valence + valence).clamp(-1.0, 1.0);
        self.arousal = (self.arousal + arousal).clamp(0.0, 1.0);
    }

    pub fn emotions(&self, needs: &Needs, traits: &Personality) -> Emotions {
        let pleasant = self.valence.max(0.0);
        let energy = 1.0 - needs.fatigue;
        Emotions {
            happiness: pleasant,
            excitement: (self.arousal * (0.4 + 0.6 * pleasant)).clamp(0.0, 1.0),
            curiosity: (traits.curiosity * needs.boredom * energy * 1.4).clamp(0.0, 1.0),
            relaxation: ((1.0 - self.arousal) * (0.5 + 0.5 * pleasant)).clamp(0.0, 1.0),
            sleepiness: needs.fatigue,
            alertness: (self.arousal * energy * 1.3).clamp(0.0, 1.0),
            confusion: self.confusion,
            playfulness: (traits.playfulness * needs.boredom * energy * 1.5).clamp(0.0, 1.0),
            // Shown gently: loneliness is information for the user, not a reproach.
            loneliness: (excess(needs.social) * 0.6).clamp(0.0, 1.0),
            contentment: (pleasant * (1.0 - 0.5 * needs.hunger.max(needs.thirst).max(needs.social))).clamp(0.0, 1.0),
        }
    }
}
