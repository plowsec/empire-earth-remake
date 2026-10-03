//! Godot bridge: renders the deterministic simulation and turns UI intent into
//! commands. All game logic lives in ee_sim / ee_ai.
use godot::prelude::*;

mod batch;
mod client;
mod models;
mod terrain;
mod view;

struct EeExtension;

#[gdextension]
unsafe impl ExtensionLibrary for EeExtension {}
