//! Choosing what to do next.
//!
//! Every activity gets a score from his needs, mood, temperament and the
//! situation. Scores are damped by how recently he did the same thing
//! (habituation), and the choice is then drawn at random in proportion to the
//! scores, so behaviour follows his state without ever becoming a fixed loop.

use crate::model::{Activity, Stage};
use crate::mood::Mood;
use crate::needs::{Needs, excess};
use crate::personality::Personality;
use crate::rng::Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// One entry in his repertoire.
pub struct Spec {
    pub activity: Activity,
    /// How many distinct ways he has of doing it.
    pub variants: u8,
    /// Shortest and longest duration in seconds; (0, 0) means "until done".
    pub secs: (f64, f64),
    /// Seconds for the appeal of repeating it to recover by about two thirds.
    pub recovery: f64,
}

#[rustfmt::skip]
pub const REPERTOIRE: [Spec; 19] = [
    Spec { activity: Activity::Stand,         variants: 3, secs: (4.0, 20.0),   recovery: 60.0 },
    Spec { activity: Activity::Walk,          variants: 4, secs: (0.0, 0.0),    recovery: 90.0 },
    Spec { activity: Activity::Sit,           variants: 2, secs: (15.0, 150.0), recovery: 240.0 },
    Spec { activity: Activity::Lie,           variants: 3, secs: (60.0, 900.0), recovery: 300.0 },
    Spec { activity: Activity::Sleep,         variants: 3, secs: (0.0, 0.0),    recovery: 600.0 },
    Spec { activity: Activity::Stretch,       variants: 2, secs: (3.5, 5.0),    recovery: 900.0 },
    Spec { activity: Activity::Yawn,          variants: 1, secs: (2.5, 3.5),    recovery: 600.0 },
    Spec { activity: Activity::Scratch,       variants: 2, secs: (3.0, 8.0),    recovery: 600.0 },
    Spec { activity: Activity::Groom,         variants: 2, secs: (10.0, 40.0),  recovery: 1200.0 },
    Spec { activity: Activity::Shake,         variants: 1, secs: (1.6, 2.4),    recovery: 900.0 },
    Spec { activity: Activity::Sniff,         variants: 2, secs: (6.0, 25.0),   recovery: 400.0 },
    Spec { activity: Activity::Zoomies,       variants: 1, secs: (8.0, 18.0),   recovery: 3600.0 },
    Spec { activity: Activity::TailChase,     variants: 2, secs: (4.0, 9.0),    recovery: 5400.0 },
    Spec { activity: Activity::PlayBow,       variants: 1, secs: (2.5, 5.0),    recovery: 1200.0 },
    Spec { activity: Activity::Roll,          variants: 2, secs: (4.0, 8.0),    recovery: 2400.0 },
    Spec { activity: Activity::Greet,         variants: 2, secs: (5.0, 9.0),    recovery: 300.0 },
    Spec { activity: Activity::SeekAttention, variants: 2, secs: (6.0, 12.0),   recovery: 3600.0 },
    Spec { activity: Activity::Eat,           variants: 1, secs: (12.0, 25.0),  recovery: 60.0 },
    Spec { activity: Activity::Drink,         variants: 1, secs: (6.0, 12.0),   recovery: 60.0 },
];

pub fn spec(activity: Activity) -> &'static Spec {
    REPERTOIRE.iter().find(|s| s.activity == activity).expect("every activity is in the repertoire")
}

/// Something he did, as kept in the behaviour log.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub ts: f64,
    pub activity: Activity,
    pub variant: u8,
    pub secs: f64,
    pub intensity: f64,
    /// Why he chose it, in words.
    pub cause: String,
    pub x_frac: f64,
}

/// What he remembers of his recent behaviour, used to avoid repeating himself.
#[derive(Clone, Debug, Default)]
pub struct Memory {
    recent: VecDeque<(f64, Activity, u8)>,
}

const REMEMBERED: usize = 400;

impl Memory {
    pub fn note(&mut self, ts: f64, activity: Activity, variant: u8) {
        self.recent.push_back((ts, activity, variant));
        if self.recent.len() > REMEMBERED {
            self.recent.pop_front();
        }
    }

    /// How "used up" an activity (or one variant of it) feels right now:
    /// each past occurrence counts 1 when fresh and fades with `recovery`.
    pub fn habituation(&self, activity: Activity, variant: Option<u8>, now: f64, recovery: f64) -> f64 {
        self.recent
            .iter()
            .filter(|(_, a, v)| *a == activity && variant.is_none_or(|want| want == *v))
            .map(|(ts, _, _)| (-(now - ts).max(0.0) / recovery).exp())
            .sum()
    }

    /// How many times he has just gone back and forth between `candidate` and
    /// what he is doing now (…candidate, current, candidate, current). Picking
    /// `candidate` again would extend the back-and-forth.
    pub fn alternations(&self, candidate: Activity) -> usize {
        let mut tail = self.recent.iter().rev().map(|(_, a, _)| *a);
        let Some(current) = tail.next() else { return 0 };
        let mut count = 0;
        while tail.next() == Some(candidate) {
            count += 1;
            if tail.next() != Some(current) {
                break;
            }
        }
        count
    }

    /// Prefers the variant he has done least recently, without making it certain.
    pub fn pick_variant(&self, spec: &Spec, now: f64, rng: &mut Rng) -> u8 {
        let weights: Vec<f64> =
            (0..spec.variants).map(|v| 1.0 / (0.15 + self.habituation(spec.activity, Some(v), now, spec.recovery * 4.0))).collect();
        let mut roll = rng.unit() * weights.iter().sum::<f64>();
        for (v, w) in weights.iter().enumerate() {
            if roll < *w {
                return v as u8;
            }
            roll -= w;
        }
        spec.variants - 1
    }
}

/// Everything a decision depends on.
pub struct Context<'a> {
    pub needs: &'a Needs,
    pub mood: &'a Mood,
    pub traits: &'a Personality,
    pub stage: Stage,
    /// 0 by day ..= 1 in the middle of the night.
    pub night: f64,
    pub now: f64,
    pub quiet: bool,
    pub food_out: bool,
    pub water_out: bool,
    /// 1 just after the user came back or petted him after a while; fades in under a minute.
    pub greet_urge: f64,
    /// 1 just after any interaction; fades over minutes.
    pub attended: f64,
    pub secs_since_asking: f64,
    /// 0..=1: how steadily the user has been typing and pointing for the last couple of minutes.
    pub user_busy: f64,
    /// Nobody has touched the computer for a while, or the screen is locked.
    pub user_away: bool,
    pub at_favourite_spot: bool,
    pub current: Activity,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Considered {
    pub activity: Activity,
    pub appeal: f64,
    pub reason: &'static str,
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub activity: Activity,
    pub variant: u8,
    pub secs: f64,
    /// 0..=1: how vigorously he does it.
    pub intensity: f64,
    pub cause: String,
    /// The strongest candidates at the moment of choosing, best first.
    pub considered: Vec<Considered>,
}

/// Minimum seconds between two bids for attention, however lonely he is.
pub const ASKING_INTERVAL: f64 = 20.0 * 60.0;

fn total(terms: &[(&'static str, f64)]) -> (f64, &'static str) {
    let sum = terms.iter().map(|t| t.1).sum();
    let reason = terms.iter().max_by(|a, b| a.1.total_cmp(&b.1)).map_or("", |t| t.0);
    (sum, reason)
}

/// How much he feels like an activity right now, and the main reason.
pub fn appeal(activity: Activity, c: &Context) -> (f64, &'static str) {
    let (n, t) = (c.needs, c.traits);
    let energy = 1.0 - n.fatigue;
    let youth = match c.stage {
        Stage::Puppy => 1.0,
        Stage::Adolescent => 0.7,
        Stage::YoungAdult => 0.5,
        Stage::Mature => 0.15,
    };
    let drive = n.boredom * energy; // wants to do something, and has the energy for it
    let pleasant = c.mood.valence.max(0.0);
    let day = c.night < 0.3;
    match activity {
        Activity::Sleep => {
            let (score, reason) = total(&[("tired", 1.5 * n.fatigue.powf(1.5)), ("it is night", 0.7 * c.night * (0.3 + n.fatigue))]);
            // By day he sleeps only when properly tired (a little sooner in the after-lunch lull).
            (if c.night < 0.5 && n.fatigue < 0.45 - 0.6 * c.night { 0.0 } else { score }, reason)
        }
        Activity::Lie => total(&[
            ("resting", 0.24),
            ("tired", 0.45 * n.fatigue),
            ("calm", 0.25 * t.calmness * (1.0 - n.boredom)),
            ("his favourite spot", if c.at_favourite_spot { 0.3 } else { 0.0 }),
            ("nobody is around", if c.user_away { 0.2 } else { 0.0 }),
        ]),
        Activity::Sit => total(&[("settled", 0.28), ("keeping you company", 0.2 * t.sociability), ("calm", 0.1 * t.calmness)]),
        Activity::Stand => total(&[("watching", 0.28 + 0.25 * c.mood.arousal)]),
        Activity::Walk => total(&[("restless", 0.75 * drive), ("energetic", 0.2 * t.energy * energy), ("wandering", 0.12)]),
        Activity::Sniff => {
            total(&[("curious", 0.9 * t.curiosity * drive), ("looking for food", if c.food_out { 0.0 } else { 0.3 * excess(n.hunger) })])
        }
        Activity::Zoomies if energy > 0.55 && c.mood.valence > 0.2 && !c.quiet && day => {
            // Less of a whirlwind while you are concentrating.
            total(&[("bursting with energy", 2.2 * drive * drive * t.playfulness * youth * (1.0 - 0.5 * c.user_busy))])
        }
        Activity::TailChase if energy > 0.4 => total(&[("playful", 1.4 * drive * t.playfulness * youth * youth)]),
        Activity::PlayBow if energy > 0.4 => total(&[("wants to play with you", 0.9 * drive * t.playfulness * c.attended)]),
        Activity::Roll => total(&[("feeling good", 0.5 * pleasant * t.playfulness * n.boredom), ("itchy back", 0.35 * n.itch)]),
        Activity::Stretch => total(&[("stiff", 1.3 * n.stiffness.powf(1.3))]),
        Activity::Yawn => total(&[("sleepy", 0.45 * n.fatigue), ("stiff", 0.25 * n.stiffness)]),
        Activity::Scratch => total(&[("itchy", 1.2 * n.itch.powf(1.2))]),
        Activity::Groom => total(&[("grooming", 0.7 * n.itch), ("calm", 0.15 * t.calmness * (1.0 - c.mood.arousal))]),
        Activity::Shake => total(&[("shaking off", 0.6 * n.itch), ("loosening up", 0.4 * n.stiffness)]),
        Activity::Greet => total(&[("happy to see you", 2.5 * c.greet_urge)]),
        // Rare by construction: never in quiet mode or at night, never twice within twenty minutes.
        // He does not ask when nobody is there to answer, or when you are plainly busy.
        Activity::SeekAttention if !c.quiet && day && c.secs_since_asking > ASKING_INTERVAL && !c.user_away && c.user_busy < 0.7 => {
            total(&[("wants company", 1.1 * excess(n.social) * t.attention_seeking * (1.0 - 0.6 * t.independence))])
        }
        Activity::Eat if c.food_out && n.hunger > 0.2 => total(&[("hungry", 0.4 + 2.0 * n.hunger)]),
        Activity::Drink if c.water_out && n.thirst > 0.2 => total(&[("thirsty", 0.3 + 2.0 * n.thirst)]),
        _ => (0.0, ""),
    }
}

pub fn decide(c: &Context, memory: &Memory, rng: &mut Rng) -> Decision {
    let mut options: Vec<Considered> = REPERTOIRE
        .iter()
        .filter(|s| s.activity != c.current)
        .filter_map(|s| {
            let (raw, reason) = appeal(s.activity, c);
            // Everyday postures wear out slowly; one-off antics wear out fast.
            let wear = match s.activity {
                Activity::Stand | Activity::Sit | Activity::Lie | Activity::Sleep | Activity::Walk => 0.5,
                Activity::Eat | Activity::Drink | Activity::Greet => 0.0,
                _ => 1.6,
            };
            let appeal = raw / (1.0 + wear * memory.habituation(s.activity, None, c.now, s.recovery));
            // Pacing between the same two things looks mechanical; each round makes another less likely.
            let appeal = appeal / (1.0 + 0.8 * memory.alternations(s.activity) as f64);
            (appeal > 0.0).then_some(Considered { activity: s.activity, appeal, reason })
        })
        .collect();
    options.sort_by(|a, b| b.appeal.total_cmp(&a.appeal));
    // Only real contenders are in the draw. Without this, the sheer number of decisions in a
    // day would let every long shot come up far more often than it should.
    let best = options.first().map_or(0.0, |o| o.appeal);
    options.retain(|o| o.appeal >= 0.2 * best);

    // Draw in proportion to exp(appeal / temperature): the best option usually wins,
    // close seconds win often, long shots occasionally. Playful young dogs are less predictable.
    let youth = if matches!(c.stage, Stage::Puppy | Stage::Adolescent) { 1.0 } else { 0.4 };
    let temperature = 0.10 + 0.08 * c.traits.playfulness * youth;
    let weights: Vec<f64> = options.iter().map(|o| ((o.appeal - best) / temperature).exp()).collect();
    let mut roll = rng.unit() * weights.iter().sum::<f64>();
    let mut chosen = options.first().cloned().unwrap_or(Considered { activity: Activity::Stand, appeal: 0.0, reason: "nothing else to do" });
    for (option, weight) in options.iter().zip(&weights) {
        if roll < *weight {
            chosen = option.clone();
            break;
        }
        roll -= weight;
    }

    let spec = spec(chosen.activity);
    // Puppies do not stay with anything for long.
    let attention = match c.stage {
        Stage::Puppy => 0.45,
        Stage::Adolescent => 0.7,
        _ => 1.0,
    };
    // A long rest suits a tired or contented dog; a restless one gets up again sooner.
    let settled = 0.35 + 0.65 * c.needs.fatigue.max(1.0 - c.needs.boredom);
    let long_rest = spec.secs.1 > 30.0;
    let secs = if spec.secs.1 > 0.0 { rng.range(spec.secs.0, spec.secs.1) * if long_rest { attention * settled } else { 1.0 } } else { 0.0 };
    options.truncate(5);
    Decision {
        activity: chosen.activity,
        variant: memory.pick_variant(spec, c.now, rng),
        secs,
        intensity: (rng.range(0.35, 0.85) + 0.3 * c.mood.arousal).clamp(0.0, 1.0),
        cause: chosen.reason.to_string(),
        considered: options,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(needs: &'a Needs, mood: &'a Mood, traits: &'a Personality) -> Context<'a> {
        Context {
            needs,
            mood,
            traits,
            stage: Stage::YoungAdult,
            night: 0.0,
            now: 10_000.0,
            quiet: false,
            food_out: false,
            water_out: false,
            greet_urge: 0.0,
            attended: 0.0,
            secs_since_asking: 1e9,
            user_busy: 0.0,
            user_away: false,
            at_favourite_spot: false,
            current: Activity::Stand,
        }
    }

    fn tally(c: &Context, memory: &Memory, draws: usize) -> std::collections::HashMap<Activity, usize> {
        let mut rng = Rng::new(7);
        let mut counts = std::collections::HashMap::new();
        for _ in 0..draws {
            *counts.entry(decide(c, memory, &mut rng).activity).or_insert(0) += 1;
        }
        counts
    }

    #[test]
    fn his_state_drives_the_choice_without_making_it_certain() {
        let (mood, traits) = (Mood::default(), Personality::default());
        let tired = Needs { fatigue: 0.9, ..Needs::default() };
        let counts = tally(&context(&tired, &mood, &traits), &Memory::default(), 1000);
        assert!(counts[&Activity::Sleep] > 600, "an exhausted dog mostly sleeps: {counts:?}");
        assert!(counts.len() > 1, "but not with certainty");

        let itchy = Needs { itch: 0.95, ..Needs::default() };
        let counts = tally(&context(&itchy, &mood, &traits), &Memory::default(), 1000);
        let relief =
            counts.get(&Activity::Scratch).unwrap_or(&0) + counts.get(&Activity::Groom).unwrap_or(&0) + counts.get(&Activity::Shake).unwrap_or(&0);
        assert!(relief > 600, "an itchy dog deals with the itch: {counts:?}");
    }

    #[test]
    fn having_just_done_something_makes_it_less_likely() {
        let (mood, traits) = (Mood::default(), Personality::default());
        let itchy = Needs { itch: 0.8, ..Needs::default() };
        let c = context(&itchy, &mood, &traits);
        let fresh = tally(&c, &Memory::default(), 2000)[&Activity::Scratch];
        let mut memory = Memory::default();
        for i in 0..3 {
            memory.note(c.now - 30.0 * f64::from(i), Activity::Scratch, 0);
        }
        let worn = *tally(&c, &memory, 2000).get(&Activity::Scratch).unwrap_or(&0);
        assert!(worn * 2 < fresh, "scratching thrice in a row should be unlikely: {fresh} vs {worn}");
    }

    #[test]
    fn he_asks_for_attention_rarely_and_never_in_quiet_mode() {
        let (mood, traits) = (Mood::default(), Personality::default());
        let lonely = Needs { social: 1.0, ..Needs::default() };
        let mut c = context(&lonely, &mood, &traits);
        assert!(appeal(Activity::SeekAttention, &c).0 > 0.0);
        c.secs_since_asking = 60.0;
        assert_eq!(appeal(Activity::SeekAttention, &c).0, 0.0, "not again so soon");
        c.secs_since_asking = 1e9;
        c.quiet = true;
        assert_eq!(appeal(Activity::SeekAttention, &c).0, 0.0);
        c.quiet = false;
        c.user_busy = 0.9;
        assert_eq!(appeal(Activity::SeekAttention, &c).0, 0.0, "not while you are busy");
        c.user_busy = 0.0;
        c.user_away = true;
        assert_eq!(appeal(Activity::SeekAttention, &c).0, 0.0, "not when nobody is there");
        c.user_away = false;
        c.night = 1.0;
        assert_eq!(appeal(Activity::SeekAttention, &c).0, 0.0, "never at night");
    }

    #[test]
    fn going_back_and_forth_is_noticed() {
        let mut memory = Memory::default();
        for (i, a) in [Activity::Sit, Activity::Walk, Activity::Lie, Activity::Walk, Activity::Lie, Activity::Walk].iter().enumerate() {
            memory.note(i as f64, *a, 0);
        }
        // He has just done walk, lie, walk, lie, walk: lying down again would be the third round.
        assert_eq!(memory.alternations(Activity::Lie), 2);
        assert_eq!(memory.alternations(Activity::Sit), 0);
    }

    #[test]
    fn variants_rotate() {
        let spec = spec(Activity::Lie);
        let mut memory = Memory::default();
        let mut rng = Rng::new(3);
        let mut seen = [0usize; 3];
        for i in 0..300 {
            let v = memory.pick_variant(spec, f64::from(i) * 600.0, &mut rng);
            memory.note(f64::from(i) * 600.0, Activity::Lie, v);
            seen[v as usize] += 1;
        }
        assert!(seen.iter().all(|&n| n > 60), "all three ways of lying down get used: {seen:?}");
    }
}
