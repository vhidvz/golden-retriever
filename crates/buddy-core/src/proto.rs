//! Wire protocol between the daemon and its clients (renderer, CLI).
//! One JSON object per line over a Unix socket.

use crate::model::{Activity, Stage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    /// A renderer subscribes to state pushes; a CLI client just sends commands.
    Hello { role: String },
    /// The renderer reports how many pixels Buddy can walk across.
    Screen { walk_px: f64 },
    Command {
        name: String,
        #[serde(default)]
        args: Value,
    },
    /// A perception provider reports something. `source` is the provider ("shell", "system", …).
    Sense {
        source: String,
        kind: String,
        #[serde(default)]
        data: Value,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    State(StateMsg),
    /// Tells a perception provider what it may currently observe.
    Policy {
        desktop: bool,
        window_titles: bool,
    },
    /// A one-off the renderer should act on now, such as a sound to make.
    Event {
        name: String,
        #[serde(default)]
        args: Value,
    },
    Reply {
        ok: bool,
        #[serde(default, skip_serializing_if = "Value::is_null")]
        data: Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Everything a renderer needs to draw Buddy this instant.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StateMsg {
    pub name: String,
    pub activity: Activity,
    /// Which of his ways of doing it (see the repertoire).
    pub variant: u8,
    /// 0..=1: how vigorously.
    pub intensity: f64,
    /// Unix time the activity began, and how long he means to keep at it (0 = until done).
    pub started: f64,
    pub planned_secs: f64,
    /// Differs for every instance of an activity, so no two look identical.
    pub seed: u32,
    /// Position along the walkable width, 0..=1.
    pub x_frac: f64,
    /// Current speed in pixels per second (signed).
    pub speed_px: f64,
    pub facing: i8,
    pub hidden: bool,
    pub privacy: bool,
    pub quiet: bool,
    /// Body growth, 0 (newborn proportions) ..= 1 (adult).
    pub growth: f64,
    /// Current standing height as a fraction of the adult's.
    pub scale: f64,
    pub stage: Stage,
    pub age_days: f64,
    /// Adult standing height in pixels.
    pub size_px: f64,
    /// Mood: -1..=1 pleasantness, 0..=1 activation, 0..=1 sleepiness.
    pub valence: f64,
    pub arousal: f64,
    pub sleepiness: f64,
    /// 0..=1: he did not understand what was said and is trying to.
    pub confusion: f64,
    pub being_dragged: bool,
    /// Out of the way for now (screen locked, overview open, a fullscreen application): draw nothing.
    pub ducked: bool,
    /// Whether there is anyone to look at. False when the user has been away a while.
    pub user_here: bool,
    /// How hard the machine is working, 0..=1; the renderer eases off when it is high.
    pub load: f64,
    pub food_out: bool,
    pub water_out: bool,
}
