//! Lives weeks or months of Buddy's life in seconds, with a stand-in user, and
//! measures whether his behaviour stays varied and his needs stay in bounds.

use crate::behavior::{REPERTOIRE, Record};
use crate::model::{Body, Identity, Life, SECS_PER_DAY, Settings};
use crate::rng::Rng;
use crate::sim::{Command, Mind, Output, Sim};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

/// Who is at the keyboard during the simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum User {
    /// The computer stays on and nobody ever interacts with him.
    Absent,
    /// The computer stays on; someone is around from morning to late evening.
    Attentive,
    /// As `Attentive`, but the computer is off overnight.
    OfficeHours,
}

#[derive(Clone, Debug)]
pub struct Scenario {
    pub days: f64,
    pub seed: u64,
    pub user: User,
    /// His age when the simulation starts, and the growth timeline in force.
    pub start_age_days: f64,
    pub adult_days: f64,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Report {
    pub days: f64,
    pub activities_logged: usize,
    /// Share of simulated time per activity.
    pub time_share: BTreeMap<String, f64>,
    pub times_done: BTreeMap<String, usize>,
    pub distinct_activities: usize,
    pub variants_used: usize,
    pub variants_available: usize,
    pub sleep_hours_per_day: f64,
    /// Average uncertainty, in bits, about what he does next given what he is
    /// doing now. A fixed routine scores 0; a coin flip between two options scores 1.
    pub next_activity_entropy_bits: f64,
    /// The most times in a row he repeated the same short cycle of activities (two to four
    /// long), and the length of that cycle. "Walk, lie, walk, lie, walk, lie" is 3 rounds of 2.
    pub longest_loop_rounds: f64,
    pub longest_loop_cycle: usize,
    /// How often the single most common three-activity sequence occurs, as a share of all such sequences.
    pub most_common_triple_share: f64,
    /// Share of timed activities done just as he did them the time before: same variant,
    /// duration within 5% and vigour within 0.05.
    pub identical_repeats_share: f64,
    pub loud_sounds_in_busiest_hour: usize,
    pub bids_for_attention_per_day: f64,
    /// Unprompted looks at the person in front of the screen, and the share of his waking time they take.
    pub glances_per_waking_hour: f64,
    pub eye_contact_share_of_waking_time: f64,
    pub lowest_mood: f64,
    pub highest_hunger: f64,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{:.0} days simulated, {} activities logged", self.days, self.activities_logged)?;
        writeln!(
            f,
            "  variety:  {} of 19 everyday activities, {} of {} variants used",
            self.distinct_activities, self.variants_used, self.variants_available
        )?;
        writeln!(f, "  unpredictability of the next activity: {:.2} bits (0 = fixed routine)", self.next_activity_entropy_bits)?;
        writeln!(
            f,
            "  longest loop: {:.1} rounds of a {}-activity cycle; most common triple: {:.1}% of all",
            self.longest_loop_rounds,
            self.longest_loop_cycle,
            100.0 * self.most_common_triple_share
        )?;
        writeln!(f, "  done exactly as the previous time: {:.2}%", 100.0 * self.identical_repeats_share)?;
        writeln!(
            f,
            "  sleep: {:.1} h/day; bids for attention: {:.1}/day; loud sounds in the busiest hour: {}",
            self.sleep_hours_per_day, self.bids_for_attention_per_day, self.loud_sounds_in_busiest_hour
        )?;
        writeln!(
            f,
            "  looks at you: {:.0} times per waking hour, {:.1}% of waking time",
            self.glances_per_waking_hour,
            100.0 * self.eye_contact_share_of_waking_time
        )?;
        writeln!(f, "  lowest mood: {:+.2} (scale -1..1); highest hunger: {:.2}", self.lowest_mood, self.highest_hunger)?;
        writeln!(f, "  time spent:")?;
        let mut shares: Vec<_> = self.time_share.iter().collect();
        shares.sort_by(|a, b| b.1.total_cmp(a.1));
        for (name, share) in shares {
            writeln!(f, "    {:<15} {:>5.1}%  ({} times)", name, 100.0 * share, self.times_done.get(name).unwrap_or(&0))?;
        }
        Ok(())
    }
}

pub fn run(scenario: &Scenario) -> Report {
    // Start at 08:00 on day zero; hours of the day are read straight off the clock.
    let t0 = 1000.0 * SECS_PER_DAY + 8.0 * 3600.0;
    let born_at = (t0 - scenario.start_age_days * SECS_PER_DAY) as i64;
    let settings = Settings { adult_days: scenario.adult_days, ..Settings::default() };
    let mut sim = Sim::new(Identity { name: "Buddy".into(), born_at }, Life::default(), settings, Body::default(), Mind::default(), scenario.seed);
    let mut user = Rng::new(scenario.seed ^ 0x9E37_79B9_7F4A_7C15);
    sim.apply(Command::SetScreen { walk_px: 2500.0 }, t0);

    let hour_of = |t: f64| (t / 3600.0).rem_euclid(24.0);
    let (mut now, end) = (t0, t0 + scenario.days * SECS_PER_DAY);
    let mut records: Vec<Record> = Vec::new();
    let mut loud: Vec<f64> = Vec::new();
    let (mut lowest_mood, mut highest_hunger) = (1.0f64, 0.0f64);
    let mut respond_at: Option<f64> = None;
    let (mut glances, mut glance_secs) = (0usize, 0.0f64);
    let mut last_hour = hour_of(now);

    while now < end {
        let dt = if sim.is_moving() { 0.25 } else { 1.0 };
        now += dt;
        let hour = hour_of(now);
        let present = scenario.user != User::Absent && (8.0..23.0).contains(&hour);

        if present {
            // Routine care, on the hour it falls due.
            for due in [8.5, 18.5] {
                if last_hour < due && hour >= due {
                    sim.apply(Command::Feed, now);
                }
            }
            for due in [9.0, 14.0, 20.0] {
                if last_hour < due && hour >= due {
                    sim.apply(Command::Water, now);
                }
            }
            // Mostly leaves a sleeping dog alone.
            let fondness = if sim.activity() == crate::model::Activity::Sleep { 0.5 } else { 2.0 };
            if user.chance(dt * fondness / 3600.0) || respond_at.is_some_and(|t| now >= t) {
                respond_at = None;
                sim.apply(Command::Pet, now);
            }
            if user.chance(dt * 0.5 / 3600.0) {
                sim.apply(Command::Praise, now);
            }
        }
        last_hour = hour;

        for output in sim.tick(now, hour) {
            match output {
                Output::Behaved(record) => records.push(record),
                Output::Look { at: "you", secs } => {
                    glances += 1;
                    glance_secs += secs;
                }
                Output::Vocalize { kind, .. } if matches!(kind, "bark" | "whine") => {
                    loud.push(now);
                    // Someone at the keyboard answers a whine a little more than half the time.
                    if present && kind == "whine" && user.chance(0.6) {
                        respond_at = Some(now + user.range(3.0, 20.0));
                    }
                }
                _ => {}
            }
        }
        lowest_mood = lowest_mood.min(sim.mood.valence);
        highest_hunger = highest_hunger.max(sim.needs.hunger);

        if scenario.user == User::OfficeHours && hour >= 23.5 {
            now += 8.5 * 3600.0; // switched off until 08:00; the next tick sees the gap
            last_hour = 0.0;
        }
    }
    let mut report = summarise(scenario.days, &records, &loud, lowest_mood, highest_hunger);
    let waking_secs = (1.0 - report.time_share.get("sleep").copied().unwrap_or(0.0)) * records.iter().map(|r| r.secs).sum::<f64>();
    report.glances_per_waking_hour = glances as f64 / (waking_secs / 3600.0).max(1e-9);
    report.eye_contact_share_of_waking_time = glance_secs / waking_secs.max(1e-9);
    report
}

fn summarise(days: f64, records: &[Record], loud: &[f64], lowest_mood: f64, highest_hunger: f64) -> Report {
    let names: Vec<String> = records.iter().map(|r| r.activity.name()).collect();
    let total_secs: f64 = records.iter().map(|r| r.secs).sum::<f64>().max(1.0);
    let mut time_share = BTreeMap::new();
    let mut times_done = BTreeMap::new();
    for (record, name) in records.iter().zip(&names) {
        *time_share.entry(name.clone()).or_insert(0.0) += record.secs / total_secs;
        *times_done.entry(name.clone()).or_insert(0usize) += 1;
    }

    // Conditional entropy H(next | current).
    let mut follows: BTreeMap<&str, BTreeMap<&str, f64>> = BTreeMap::new();
    for pair in names.windows(2) {
        *follows.entry(&pair[0]).or_default().entry(&pair[1]).or_insert(0.0) += 1.0;
    }
    let pairs = names.len().saturating_sub(1).max(1) as f64;
    let entropy: f64 = follows
        .values()
        .map(|nexts| {
            let n: f64 = nexts.values().sum();
            (n / pairs) * nexts.values().map(|c| -(c / n) * (c / n).log2()).sum::<f64>()
        })
        .sum();

    // For each cycle length, the longest stretch where every activity equals the one a cycle earlier.
    let (longest_loop_rounds, longest_loop_cycle) = (2..=4)
        .map(|cycle| {
            let (mut best, mut run) = (0, 0);
            for i in cycle..names.len() {
                run = if names[i] == names[i - cycle] { run + 1 } else { 0 };
                best = best.max(run);
            }
            ((best + cycle) as f64 / cycle as f64, cycle)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap_or((0.0, 0));

    let mut triples: HashMap<&[String], usize> = HashMap::new();
    for triple in names.windows(3) {
        *triples.entry(triple).or_insert(0) += 1;
    }
    let most_common_triple_share = triples.values().max().map_or(0.0, |&m| m as f64 / names.len().saturating_sub(2).max(1) as f64);

    let timed: Vec<&Record> = records.iter().filter(|r| crate::behavior::spec(r.activity).secs.1 > 0.0).collect();
    let mut previous: HashMap<_, &Record> = HashMap::new();
    let mut identical = 0;
    for record in &timed {
        if let Some(before) = previous.insert(record.activity, record) {
            let same_length = (record.secs - before.secs).abs() <= 0.05 * before.secs;
            if record.variant == before.variant && same_length && (record.intensity - before.intensity).abs() <= 0.05 {
                identical += 1;
            }
        }
    }

    let busiest = loud.iter().map(|start| loud.iter().filter(|t| (**t - start).abs() < 1800.0).count()).max().unwrap_or(0);
    let variants: HashSet<_> = records.iter().map(|r| (r.activity, r.variant)).collect();

    Report {
        days,
        activities_logged: records.len(),
        distinct_activities: time_share.len(),
        variants_used: variants.len(),
        variants_available: REPERTOIRE.iter().map(|s| usize::from(s.variants)).sum(),
        sleep_hours_per_day: 24.0 * time_share.get("sleep").copied().unwrap_or(0.0),
        next_activity_entropy_bits: entropy,
        longest_loop_rounds,
        longest_loop_cycle,
        most_common_triple_share,
        identical_repeats_share: f64::from(identical) / timed.len().max(1) as f64,
        loud_sounds_in_busiest_hour: busiest,
        glances_per_waking_hour: 0.0,
        eye_contact_share_of_waking_time: 0.0,
        bids_for_attention_per_day: *times_done.get("seek_attention").unwrap_or(&0) as f64 / days,
        lowest_mood,
        highest_hunger,
        time_share,
        times_done,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario(user: User, start_age_days: f64) -> Scenario {
        Scenario { days: 45.0, seed: 11, user, start_age_days, adult_days: 365.0 }
    }

    #[test]
    fn weeks_of_life_do_not_settle_into_a_routine() {
        for (who, age) in [(User::Attentive, 400.0), (User::Attentive, 20.0), (User::OfficeHours, 400.0)] {
            let r = run(&scenario(who, age));
            assert!(r.next_activity_entropy_bits > 2.0, "{who:?} at {age} days is too predictable:\n{r}");
            assert!(r.longest_loop_rounds <= 5.0, "{who:?} at {age} days fell into a loop:\n{r}");
            assert!(r.most_common_triple_share < 0.03, "{who:?} at {age} days repeats one sequence:\n{r}");
            assert!(r.identical_repeats_share < 0.05, "{who:?} at {age} days repeats himself exactly:\n{r}");
            assert!(
                r.distinct_activities >= 17 && r.variants_used >= r.variants_available - 3,
                "{who:?} at {age} days leaves his repertoire unused:\n{r}"
            );
            assert!(r.time_share.values().all(|share| *share < 0.75), "{who:?} at {age} days does almost nothing but one thing:\n{r}");
        }
    }

    #[test]
    fn he_sleeps_like_a_dog_and_puppies_sleep_more() {
        let adult = run(&scenario(User::Attentive, 400.0));
        let puppy = run(&scenario(User::Attentive, 20.0));
        assert!((9.0..=16.0).contains(&adult.sleep_hours_per_day), "adult sleep:\n{adult}");
        assert!(
            puppy.sleep_hours_per_day > adult.sleep_hours_per_day + 1.0,
            "puppy {} vs adult {}",
            puppy.sleep_hours_per_day,
            adult.sleep_hours_per_day
        );
    }

    #[test]
    fn being_ignored_never_turns_into_nagging_or_misery() {
        let r = run(&scenario(User::Absent, 400.0));
        assert!(r.bids_for_attention_per_day <= 4.0, "he keeps asking:\n{r}");
        assert!(r.lowest_mood >= -0.21, "he is being made miserable:\n{r}");
        assert!(r.loud_sounds_in_busiest_hour <= 6, "he is noisy:\n{r}");
        assert!(r.next_activity_entropy_bits > 1.8, "he shuts down:\n{r}");
        // With someone around, asking is occasional too.
        let cared_for = run(&scenario(User::Attentive, 400.0));
        assert!(cared_for.bids_for_attention_per_day <= 6.0 && cared_for.highest_hunger < 0.95, "{cared_for}");
    }

    #[test]
    fn he_looks_at_you_now_and_then_but_does_not_stare() {
        for who in [User::Attentive, User::Absent] {
            let r = run(&scenario(who, 400.0));
            assert!((10.0..=80.0).contains(&r.glances_per_waking_hour), "{who:?}:\n{r}");
            assert!((0.01..=0.08).contains(&r.eye_contact_share_of_waking_time), "{who:?}:\n{r}");
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_life() {
        let a = run(&Scenario { days: 5.0, ..scenario(User::Attentive, 400.0) });
        let b = run(&Scenario { days: 5.0, ..scenario(User::Attentive, 400.0) });
        assert_eq!(a, b);
        let c = run(&Scenario { days: 5.0, seed: 12, ..scenario(User::Attentive, 400.0) });
        assert_ne!(a.times_done, c.times_done);
    }
}
