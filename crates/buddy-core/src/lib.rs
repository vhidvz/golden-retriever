//! Buddy's brain: who he is, how time passes for him, and what gets remembered.
//!
//! Nothing here knows about windows, GPUs or language models. The daemon drives
//! `Sim` with wall-clock time and persists it through `Store`; a renderer only
//! ever sees the `StateMsg` snapshots.

pub mod behavior;
pub mod model;
pub mod mood;
pub mod needs;
pub mod paths;
pub mod personality;
pub mod proto;
pub mod rng;
pub mod sim;
pub mod simulate;
pub mod store;

pub use behavior::Record;
pub use model::{Activity, Body, Identity, Life, Settings, Stage};
pub use proto::{ClientMsg, ServerMsg, StateMsg};
pub use sim::{Command, JournalEntry, Mind, Output, Sim};
pub use store::Store;
