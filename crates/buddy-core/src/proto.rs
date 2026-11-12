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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    State(StateMsg),
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
    /// Position along the walkable width, 0..=1.
    pub x_frac: f64,
    /// Current speed in pixels per second (signed).
    pub speed_px: f64,
    pub facing: i8,
    pub hidden: bool,
    pub privacy: bool,
    /// Body growth, 0 (newborn proportions) ..= 1 (adult).
    pub growth: f64,
    /// Current standing height as a fraction of the adult's.
    pub scale: f64,
    pub stage: Stage,
    pub age_days: f64,
    /// Adult standing height in pixels.
    pub size_px: f64,
    /// 0..=1, rises when petted and decays.
    pub excitement: f64,
    pub being_dragged: bool,
}
