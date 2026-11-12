//! Who Buddy is: stable traits, each 0..=1, that weight his decisions.
//! They start from a Golden Retriever's typical temperament with individual
//! variation, and shift very slowly with how he is treated.

use crate::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Personality {
    pub affection: f64,
    pub curiosity: f64,
    pub playfulness: f64,
    pub confidence: f64,
    pub sociability: f64,
    pub independence: f64,
    pub energy: f64,
    pub attention_seeking: f64,
    pub calmness: f64,
}

impl Default for Personality {
    /// The breed's typical temperament.
    fn default() -> Self {
        Self {
            affection: 0.85,
            curiosity: 0.65,
            playfulness: 0.75,
            confidence: 0.60,
            sociability: 0.85,
            independence: 0.30,
            energy: 0.70,
            attention_seeking: 0.55,
            calmness: 0.50,
        }
    }
}

impl Personality {
    /// An individual: the breed baseline, each trait nudged by up to ±0.12.
    pub fn born(rng: &mut Rng) -> Self {
        let mut p = Self::default();
        for t in p.traits_mut() {
            *t = (*t + rng.range(-0.12, 0.12)).clamp(0.05, 0.95);
        }
        p
    }

    fn traits_mut(&mut self) -> [&mut f64; 9] {
        [
            &mut self.affection,
            &mut self.curiosity,
            &mut self.playfulness,
            &mut self.confidence,
            &mut self.sociability,
            &mut self.independence,
            &mut self.energy,
            &mut self.attention_seeking,
            &mut self.calmness,
        ]
    }
}

/// Moves a trait a small step towards 0 or 1. Experience shapes him, slowly:
/// hundreds of similar experiences are needed for a visible change.
pub fn drift(trait_value: &mut f64, towards_one: bool, step: f64) {
    let target = if towards_one { 0.95 } else { 0.05 };
    *trait_value += (target - *trait_value) * step;
}
