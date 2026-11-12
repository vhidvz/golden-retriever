//! How Buddy learns what words mean, the way a dog does: by what follows.
//!
//! Every cue he hears has a strength for each thing he might do in response.
//! Hearing a cue, he does the strongest response with a probability that grows
//! with its strength; praise or a treat soon afterwards strengthens that
//! pairing, a "no" weakens it. A word he has never been taught starts with no
//! pairing at all, and gains one when he is rewarded for what he happens to be
//! doing after hearing it, or when the user tells him outright.

use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How well he knows the built-in cues before any training: he gets them sometimes.
const INSTINCT: f64 = 0.35;
/// Seconds after a response in which praise or a correction is taken to be about it.
pub const FEEDBACK_WINDOW: f64 = 20.0;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Part of what any dog of his kind half-knows.
    Instinct,
    /// The user told him outright what the word means.
    Taught,
    /// He worked it out from being rewarded.
    Learned,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Pairing {
    pub strength: f64,
    pub source: Source,
    /// Times this pairing was rewarded and corrected.
    pub rewarded: u32,
    pub corrected: u32,
}

/// What he did on hearing a cue.
#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    /// He does this trick (which may or may not be the one the cue means).
    Does(String),
    /// He heard, but does nothing: he does not know the word, or is not minded to.
    Puzzled,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Training {
    /// cue phrase (normalised) -> trick -> how strongly he pairs them.
    cues: BTreeMap<String, BTreeMap<String, Pairing>>,
    #[serde(skip)]
    last: Option<LastHeard>,
}

/// The cue he last heard, what he did about it (if anything), and when.
#[derive(Clone, Debug, PartialEq)]
struct LastHeard(String, Option<String>, f64);

impl Training {
    fn pairing(&self, cue: &str, trick: &str, builtin: bool) -> f64 {
        match self.cues.get(cue).and_then(|tricks| tricks.get(trick)) {
            Some(p) => p.strength,
            None if builtin && cue == trick => INSTINCT,
            None => 0.0,
        }
    }

    fn entry(&mut self, cue: &str, trick: &str, builtin: bool) -> &mut Pairing {
        let start = if builtin && cue == trick {
            Pairing { strength: INSTINCT, source: Source::Instinct, rewarded: 0, corrected: 0 }
        } else {
            Pairing { strength: 0.0, source: Source::Learned, rewarded: 0, corrected: 0 }
        };
        self.cues.entry(cue.to_string()).or_default().entry(trick.to_string()).or_insert(start)
    }

    /// The cue phrases the user has taught or he has learned, for matching what is said.
    pub fn known_cues(&self, builtin: &[&str]) -> Vec<String> {
        self.cues.keys().filter(|cue| !builtin.contains(&cue.as_str())).cloned().collect()
    }

    /// He hears a cue. `focus` (0..=1) is how able he is to attend right now:
    /// a tired or over-excited dog obeys less. `builtin` lists the cues every dog half-knows.
    pub fn hear(&mut self, cue: &str, builtin: &[&str], focus: f64, now: f64, rng: &mut Rng) -> Response {
        let is_builtin = builtin.contains(&cue);
        let mut candidates: Vec<(String, f64)> =
            self.cues.get(cue).map(|t| t.iter().map(|(trick, p)| (trick.clone(), p.strength)).collect()).unwrap_or_default();
        if is_builtin && !candidates.iter().any(|(t, _)| t == cue) {
            candidates.push((cue.to_string(), INSTINCT));
        }
        candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
        let response = match candidates.first() {
            Some((trick, strength)) if rng.chance((0.15 + 0.85 * strength) * focus) => {
                // Sometimes, with a word he only half-knows, he offers the runner-up instead.
                match candidates.get(1) {
                    Some((other, second)) if *strength < 0.6 && rng.chance(0.5 * second / (strength + second)) => Response::Does(other.clone()),
                    _ => Response::Does(trick.clone()),
                }
            }
            _ => Response::Puzzled,
        };
        let did = match &response {
            Response::Does(trick) => Some(trick.clone()),
            Response::Puzzled => None,
        };
        self.last = Some(LastHeard(cue.to_string(), did, now));
        response
    }

    /// Praise, a treat or approval. If it follows a cue closely, it is about that:
    /// either the response he gave, or (if he gave none) what he is doing anyway,
    /// which is how a new word comes to mean something. Returns what was strengthened.
    pub fn reward(&mut self, amount: f64, doing: Option<&str>, builtin: &[&str], now: f64) -> Option<(String, String, f64)> {
        let LastHeard(cue, did, at) = self.last.clone()?;
        if now - at > FEEDBACK_WINDOW {
            return None;
        }
        // He gave no answer, so the reward attaches to what he is doing anyway. A word that
        // already means something to him can be strengthened that way, but not given a second meaning.
        let means: Vec<&String> =
            self.cues.get(&cue).map(|tricks| tricks.iter().filter(|(_, p)| p.strength >= 0.3).map(|(t, _)| t).collect()).unwrap_or_default();
        let settled = !means.is_empty() || builtin.contains(&cue.as_str());
        let trick = match (did, doing) {
            (Some(trick), _) => trick,
            (None, Some(doing)) if !settled || means.iter().any(|t| *t == doing) || (builtin.contains(&cue.as_str()) && cue == doing) => {
                doing.to_string()
            }
            _ => return None,
        };
        let is_builtin = builtin.contains(&cue.as_str());
        let pairing = self.entry(&cue, &trick, is_builtin);
        pairing.strength += amount * (1.0 - pairing.strength);
        pairing.rewarded += 1;
        let strength = pairing.strength;
        // The response is now accounted for; a second "good boy" should not double-count a guess.
        self.last = Some(LastHeard(cue.clone(), Some(trick.clone()), at));
        Some((cue, trick, strength))
    }

    /// "No": what he just did on that cue was wrong. Returns what was weakened.
    pub fn correct(&mut self, builtin: &[&str], now: f64) -> Option<(String, String, f64)> {
        let LastHeard(cue, did, at) = self.last.clone()?;
        let trick = did?;
        if now - at > FEEDBACK_WINDOW {
            return None;
        }
        let is_builtin = builtin.contains(&cue.as_str());
        let pairing = self.entry(&cue, &trick, is_builtin);
        pairing.strength *= 0.7;
        pairing.corrected += 1;
        Some((cue, trick, pairing.strength))
    }

    /// The user says outright what a word means.
    pub fn teach(&mut self, cue: &str, trick: &str) {
        let pairing = self.cues.entry(cue.to_string()).or_default().entry(trick.to_string()).or_insert(Pairing {
            strength: 0.0,
            source: Source::Taught,
            rewarded: 0,
            corrected: 0,
        });
        pairing.strength = pairing.strength.max(0.55);
        pairing.source = Source::Taught;
    }

    /// Forget everything about a cue. True if there was anything to forget.
    pub fn forget(&mut self, cue: &str) -> bool {
        self.cues.remove(cue).is_some()
    }

    /// Everything he associates, for the user to inspect: (cue, trick, pairing), strongest first.
    pub fn known(&self, builtin: &[&str]) -> Vec<(String, String, Pairing)> {
        let mut all: Vec<(String, String, Pairing)> =
            self.cues.iter().flat_map(|(cue, tricks)| tricks.iter().map(move |(trick, p)| (cue.clone(), trick.clone(), p.clone()))).collect();
        for cue in builtin {
            if !all.iter().any(|(c, t, _)| c == cue && t == cue) {
                all.push((cue.to_string(), cue.to_string(), Pairing { strength: INSTINCT, source: Source::Instinct, rewarded: 0, corrected: 0 }));
            }
        }
        all.sort_by(|a, b| b.2.strength.total_cmp(&a.2.strength).then(a.0.cmp(&b.0)));
        all
    }

    pub fn strength(&self, cue: &str, trick: &str, builtin: &[&str]) -> f64 {
        self.pairing(cue, trick, builtin.contains(&cue))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILTIN: [&str; 3] = ["sit", "down", "spin"];

    /// Gives a cue `times` times, rewarding the right response and correcting a wrong one. Returns how often he got it right.
    fn drill(t: &mut Training, cue: &str, right: &str, times: usize, rng: &mut Rng, start: f64) -> usize {
        let mut correct = 0;
        for i in 0..times {
            let now = start + 60.0 * i as f64;
            match t.hear(cue, &BUILTIN, 1.0, now, rng) {
                Response::Does(trick) if trick == right => {
                    correct += 1;
                    t.reward(0.18, None, &BUILTIN, now + 3.0);
                }
                Response::Does(_) => {
                    t.correct(&BUILTIN, now + 3.0);
                }
                Response::Puzzled => {}
            }
        }
        correct
    }

    #[test]
    fn practice_with_praise_makes_him_reliable() {
        let (mut t, mut rng) = (Training::default(), Rng::new(5));
        // Asked without any feedback, so that asking does not itself teach him.
        let sits = |t: &mut Training, rng: &mut Rng, start: f64| {
            (0..40).filter(|i| t.hear("sit", &BUILTIN, 1.0, start + f64::from(*i), rng) == Response::Does("sit".into())).count()
        };
        let early = sits(&mut t, &mut rng, 0.0);
        drill(&mut t, "sit", "sit", 60, &mut rng, 2000.0);
        let late = sits(&mut t, &mut rng, 9000.0);
        assert!(early < 26, "an untrained dog sits only sometimes: {early}/40");
        assert!(late >= 36, "a trained one nearly always: {late}/40");
        assert!(t.strength("sit", "sit", &BUILTIN) > 0.9);
    }

    #[test]
    fn a_new_word_is_learned_by_rewarding_what_he_happens_to_do() {
        let (mut t, mut rng) = (Training::default(), Rng::new(9));
        assert_eq!(t.hear("بپر", &BUILTIN, 1.0, 0.0, &mut rng), Response::Puzzled, "he has never heard it");
        // The user says the word while he is spinning of his own accord, then praises him. Several times.
        for i in 0..6 {
            let now = 100.0 * f64::from(i);
            t.hear("بپر", &BUILTIN, 1.0, now, &mut rng);
            t.reward(0.18, Some("spin"), &BUILTIN, now + 4.0);
        }
        assert!(t.strength("بپر", "spin", &BUILTIN) > 0.5);
        assert_eq!(t.known_cues(&BUILTIN), vec!["بپر".to_string()]);
        let got = (0..20).filter(|i| t.hear("بپر", &BUILTIN, 1.0, 1000.0 + f64::from(*i), &mut rng) == Response::Does("spin".into())).count();
        assert!(got >= 10, "now the word means something to him: {got}/20");
    }

    #[test]
    fn a_no_weakens_only_what_he_just_did_and_only_soon_after() {
        let (mut t, mut rng) = (Training::default(), Rng::new(3));
        t.teach("bang", "down");
        let before = t.strength("bang", "down", &BUILTIN);
        while t.hear("bang", &BUILTIN, 1.0, 10.0, &mut rng) == Response::Puzzled {}
        assert_eq!(t.correct(&BUILTIN, 100.0), None, "a correction a minute later is not about that");
        assert!(t.correct(&BUILTIN, 15.0).is_some());
        assert!(t.strength("bang", "down", &BUILTIN) < before);
        assert_eq!(t.strength("sit", "sit", &BUILTIN), INSTINCT, "nothing else is touched");
    }

    #[test]
    fn telling_him_outright_works_at_once_and_can_be_forgotten() {
        let (mut t, mut rng) = (Training::default(), Rng::new(1));
        t.teach("بچرخ بچرخ", "spin");
        let got = (0..20).filter(|i| t.hear("بچرخ بچرخ", &BUILTIN, 1.0, f64::from(*i), &mut rng) == Response::Does("spin".into())).count();
        assert!(got >= 9, "{got}/20");
        assert!(t.known(&BUILTIN).iter().any(|(cue, trick, p)| cue == "بچرخ بچرخ" && trick == "spin" && p.source == Source::Taught));
        assert!(t.forget("بچرخ بچرخ") && !t.forget("بچرخ بچرخ"));
        // What was learned survives being saved.
        t.teach("x", "sit");
        let restored: Training = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(restored.known(&BUILTIN), t.known(&BUILTIN));
    }

    #[test]
    fn a_tired_dog_obeys_less() {
        let mut rng = Rng::new(2);
        let mut fresh = Training::default();
        let mut tired = Training::default();
        fresh.teach("sit", "sit");
        tired.teach("sit", "sit");
        let count = |t: &mut Training, focus: f64, rng: &mut Rng| {
            (0..200).filter(|i| t.hear("sit", &BUILTIN, focus, f64::from(*i), rng) != Response::Puzzled).count()
        };
        assert!(count(&mut fresh, 1.0, &mut rng) > count(&mut tired, 0.4, &mut rng) + 40);
    }
}
