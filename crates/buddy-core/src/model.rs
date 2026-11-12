use serde::{Deserialize, Serialize};

pub const SECS_PER_DAY: f64 = 86_400.0;

/// What never changes about Buddy once he is born.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Identity {
    pub name: String,
    /// Unix seconds. Chronological age is always derived from this, so time
    /// spent with the computer off still ages him.
    pub born_at: i64,
}

/// Accumulated experience, kept apart from chronological age: a month with the
/// computer off makes him a month older but adds nothing here.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Life {
    /// Seconds he has actually been running and simulated.
    pub awake_secs: f64,
    pub sessions: u64,
    /// Unix seconds of the last simulated moment.
    pub last_seen: i64,
    pub pets: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Real-world days from birth to full size. Changing it re-times growth
    /// without resetting anything.
    pub adult_days: f64,
    /// Standing height of the adult dog on screen, in pixels.
    pub size_px: f64,
    /// Hidden until this Unix time; `i64::MAX` means until shown again.
    pub hidden_until: Option<i64>,
    /// The user sent him to sleep; he stays asleep until woken.
    pub asleep_by_user: bool,
    /// Suspends every perception source. Nothing observes anything while set.
    pub privacy_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            adult_days: 365.0,
            size_px: 420.0,
            hidden_until: None,
            asleep_by_user: false,
            privacy_mode: false,
        }
    }
}

/// Where he was last standing, as a fraction of the walkable width so it
/// survives a change of monitor or resolution.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Body {
    pub x_frac: f64,
    /// +1 facing right, -1 facing left.
    pub facing: i8,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            x_frac: 0.7,
            facing: -1,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Stand,
    Walk,
    Sit,
    Lie,
    Sleep,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Puppy,
    Adolescent,
    YoungAdult,
    Mature,
}

/// Fraction of the way to adulthood, unclamped (1.0 = `adult_days` old).
pub fn maturity(age_secs: f64, adult_days: f64) -> f64 {
    (age_secs / SECS_PER_DAY / adult_days.max(1.0)).max(0.0)
}

pub fn stage(maturity: f64) -> Stage {
    match maturity {
        m if m < 0.25 => Stage::Puppy,
        m if m < 0.60 => Stage::Adolescent,
        m if m < 1.00 => Stage::YoungAdult,
        _ => Stage::Mature,
    }
}

/// Body growth in 0..=1. Dogs gain most of their size early, so this eases out
/// rather than tracking age linearly.
pub fn growth(maturity: f64) -> f64 {
    let m = maturity.clamp(0.0, 1.0);
    1.0 - (1.0 - m) * (1.0 - m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_follow_the_configured_timeline() {
        let day = SECS_PER_DAY;
        assert_eq!(stage(maturity(0.0, 365.0)), Stage::Puppy);
        assert_eq!(stage(maturity(100.0 * day, 365.0)), Stage::Adolescent);
        assert_eq!(stage(maturity(300.0 * day, 365.0)), Stage::YoungAdult);
        assert_eq!(stage(maturity(400.0 * day, 365.0)), Stage::Mature);
        // A faster timeline reaches the same stage sooner, with no reset.
        assert_eq!(stage(maturity(40.0 * day, 30.0)), Stage::Mature);
    }

    #[test]
    fn growth_is_front_loaded_and_bounded() {
        assert_eq!(growth(0.0), 0.0);
        assert!(growth(0.5) > 0.7);
        assert_eq!(growth(1.0), 1.0);
        assert_eq!(growth(5.0), 1.0);
    }
}
