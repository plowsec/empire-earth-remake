//! Player commands: the ONLY way anything outside the sim changes the game.
//! Human UI, AI and (later) network peers all produce these.
use crate::defs::DefId;
use crate::entity::EntityId;
use crate::fixed::FVec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandKind {
    /// Move (or attack-move) to a point. `queue` appends instead of replacing.
    Move { units: Vec<EntityId>, to: FVec, attack_move: bool, queue: bool },
    /// Smart right-click on an entity: attack enemies, gather resources, repair/build
    /// own structures, board own transports.
    Target { units: Vec<EntityId>, target: EntityId, queue: bool },
    /// Force attack (even allies/own).
    Attack { units: Vec<EntityId>, target: EntityId, queue: bool },
    /// Place a building foundation; listed citizens go build it.
    Build { units: Vec<EntityId>, def: DefId, tile: (i32, i32), queue: bool },
    Train { building: EntityId, def: DefId, count: u8 },
    Research { building: EntityId, tech: u16 },
    /// Cancel the production item at `index` (refunds).
    CancelProduction { building: EntityId, index: u8 },
    SetRally { buildings: Vec<EntityId>, to: FVec, target: EntityId },
    Stop { units: Vec<EntityId> },
    /// Transports: unload all cargo near `at`.
    Unload { units: Vec<EntityId>, at: FVec },
    /// Aircraft: return to the nearest airfield.
    ReturnToBase { units: Vec<EntityId> },
    Delete { units: Vec<EntityId> },
    /// Loop around the island attacking anything encountered.
    Scout { units: Vec<EntityId> },
    /// Granary: lay out farms on every free plot around it and send citizens to work them.
    RebuildFarms { building: EntityId },
    /// Missile silo: fire one stored ICBM at a point.
    Launch { building: EntityId, at: FVec },
    Resign,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub player: u8,
    pub kind: CommandKind,
}

/// All commands that execute on one tick, in canonical order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickCommands {
    pub tick: u32,
    pub commands: Vec<Command>,
}

impl TickCommands {
    /// Canonical order: by player, then by submission order within that player
    /// (stable sort). Every peer must apply commands in the same order.
    pub fn canonicalize(&mut self) {
        self.commands.sort_by_key(|c| c.player);
    }
}
