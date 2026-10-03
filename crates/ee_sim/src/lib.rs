//! Deterministic Empire Earth simulation core. No floats, no I/O, no engine.
pub mod buildings;
pub mod combat;
pub mod command;
pub mod defs;
pub mod entity;
pub mod fixed;
pub mod map;
pub mod mapgen;
pub mod orders;
pub mod path;
pub mod rng;
pub mod spatial;
pub mod units;
pub mod vision;
pub mod world;

pub use command::{Command, CommandKind, TickCommands};
pub use world::{data, MatchConfig, World};

/// Simulation build stamp (git revision of the sim sources, "+dirty" for local edits).
/// Replays record it: re-simulating with a different build may diverge.
pub const SIM_VERSION: &str = env!("EE_SIM_VERSION");
