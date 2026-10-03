//! Entities: units, buildings and resource nodes share one struct. Slot-based
//! storage with generation-tagged ids, iterated in slot order (deterministic).
use crate::defs::DefId;
use crate::fixed::FVec;
use serde::{Deserialize, Serialize};

pub type EntityId = u32;
pub const NO_ENTITY: EntityId = 0;
pub const SLOT_BITS: u32 = 20;
pub const SLOT_MASK: u32 = (1 << SLOT_BITS) - 1;

#[inline]
pub fn slot_of(id: EntityId) -> usize {
    (id & SLOT_MASK) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Order {
    Idle,
    Move { to: FVec, attack_move: bool },
    Attack { target: EntityId },
    Gather { node: EntityId },
    ReturnCargo,
    Build { site: EntityId },
    Repair { target: EntityId },
    Board { transport: EntityId },
    Unload { at: FVec },
    ReturnToBase,
    /// aircraft: fly to `at`, circle it, engage anything in reach; refuel and come back
    Patrol { at: FVec },
    /// loop the `patrol` waypoints around the island, engaging enemies met on the way
    Scout { idx: u8 },
}

/// What the unit is visibly doing (drives animation on the client).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Action {
    Idle = 0,
    Move = 1,
    Attack = 2,
    Gather = 3,
    Build = 4,
    Carry = 5,
    Landed = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProdItem {
    Unit(DefId),
    Tech(u16),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entity {
    pub id: EntityId,
    pub alive: bool,
    pub def: DefId,
    pub owner: u8,
    pub pos: FVec,
    /// position at the start of the tick (render interpolation; not hashed)
    pub prev_pos: FVec,
    /// unit-length heading
    pub facing: FVec,
    pub hp: i32,
    /// top-left tile for buildings/resources
    pub tile: (i32, i32),

    pub order: Order,
    pub queue: Vec<Order>,
    pub action: Action,

    // movement
    pub goal: Option<FVec>,
    /// remaining waypoints, last element = next
    pub path: Vec<FVec>,
    /// flow field key when following a shared field
    pub flow: Option<(u8, i32, i32)>,
    pub repath_cd: i32,
    pub stuck: i32,
    /// goal is "reach the area around" (building/resource), not an exact point
    pub goal_rect: bool,
    pub vel: FVec,

    // combat
    pub weapon_cd: [i32; 3],
    pub ammo: i32,
    pub target: EntityId,
    pub acquire_cd: i32,
    pub last_fire_tick: u32,
    pub last_hit_tick: u32,
    /// attack order came from the player (don't drop for a closer target)
    pub forced_target: bool,

    // economy
    pub carry_res: u8,
    pub carry: i32,
    pub gather_acc: i32,
    pub last_node: EntityId,
    pub last_node_pos: FVec,
    pub last_res: u8,
    /// resource nodes: amount left; farms: unused
    pub amount: i32,
    /// resource nodes/farms: current number of gatherers (soft cap)
    pub gatherers: u8,

    // buildings
    pub progress: i32,
    pub complete: bool,
    pub production: Vec<ProdItem>,
    pub prod_progress: i32,
    pub rally: Option<FVec>,
    pub rally_target: EntityId,

    // transport / aircraft
    pub cargo: Vec<EntityId>,
    pub inside: EntityId,
    pub fuel: i32,
    pub home: EntityId,

    pub kills: u16,
    /// aircraft: patrol point to return to after refuelling
    pub sortie: Option<FVec>,
    /// scout route
    pub patrol: Vec<FVec>,
    /// last tick this unit called for help
    pub help_tick: u32,
    /// resource nodes: units actively working it right now
    pub miners: u8,
}

impl Entity {
    pub fn new(id: EntityId, def: DefId, owner: u8, pos: FVec, hp: i32) -> Entity {
        Entity {
            id,
            alive: true,
            def,
            owner,
            pos,
            prev_pos: pos,
            facing: FVec::new(crate::fixed::Fx::ZERO, crate::fixed::Fx::ONE),
            hp,
            tile: pos.tile(),
            order: Order::Idle,
            queue: Vec::new(),
            action: Action::Idle,
            goal: None,
            path: Vec::new(),
            flow: None,
            repath_cd: 0,
            stuck: 0,
            goal_rect: false,
            vel: FVec::ZERO,
            weapon_cd: [0; 3],
            ammo: 0,
            target: 0,
            acquire_cd: 0,
            last_fire_tick: 0,
            last_hit_tick: 0,
            forced_target: false,
            carry_res: 255,
            carry: 0,
            gather_acc: 0,
            last_node: 0,
            last_node_pos: pos,
            last_res: 255,
            amount: 0,
            gatherers: 0,
            progress: 0,
            complete: true,
            production: Vec::new(),
            prod_progress: 0,
            rally: None,
            rally_target: 0,
            cargo: Vec::new(),
            inside: 0,
            fuel: 0,
            home: 0,
            kills: 0,
            sortie: None,
            patrol: Vec::new(),
            help_tick: 0,
            miners: 0,
        }
    }
    /// Units inside a transport or landed at an airport are not on the map.
    #[inline]
    pub fn on_map(&self) -> bool {
        self.alive && self.inside == 0
    }
}
