//! The authoritative game state and the fixed-step update.
use crate::command::{Command, TickCommands};
use crate::defs::{Class, Def, DefId, GameData, Layer, NUM_RES};
use crate::entity::*;
use crate::fixed::{FVec, Fx, ONE};
use crate::map::Map;
use crate::mapgen::{self, MapParams, GAIA};
use crate::path::{FlowField, PathScratch};
use crate::rng::SimRng;
use crate::spatial::SpatialHash;
use std::sync::OnceLock;

static DATA: OnceLock<GameData> = OnceLock::new();
pub fn data() -> &'static GameData {
    DATA.get_or_init(GameData::load)
}

#[derive(Clone, Debug)]
pub struct PlayerConfig {
    pub name: String,
    pub team: u8,
    pub color: u8,
    pub is_ai: bool,
}

#[derive(Clone, Debug)]
pub struct MatchConfig {
    pub seed: u64,
    pub map_size: u8,
    pub resources: u32,
    pub start_res: [i32; NUM_RES],
    pub pop_limit: i32,
    pub players: Vec<PlayerConfig>,
    /// reveal the whole map (debug / spectator)
    pub reveal: bool,
}

impl MatchConfig {
    pub fn skirmish(seed: u64, players: usize) -> MatchConfig {
        MatchConfig {
            seed,
            map_size: 1,
            resources: 100,
            start_res: [1500, 1500, 800, 1000, 1000],
            pop_limit: 300,
            players: (0..players)
                .map(|i| PlayerConfig { name: format!("Player {}", i + 1), team: i as u8, color: i as u8, is_ai: i != 0 })
                .collect(),
            reveal: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnitMods {
    pub attack_pct: i32,
    pub armor: i32,
    pub hp_pct: i32,
    pub speed_pct: i32,
    pub range: i32,
    pub sight_pct: i32,
    pub gather_pct: [i32; NUM_RES],
}

#[derive(Clone, Debug, Default)]
pub struct PlayerStats {
    pub trained: u32,
    pub lost: u32,
    pub kills: u32,
    pub built: u32,
    pub razed: u32,
    pub gathered: [i64; NUM_RES],
}

#[derive(Clone, Debug)]
pub struct Player {
    pub id: u8,
    pub team: u8,
    pub color: u8,
    pub name: String,
    pub is_ai: bool,
    /// whole units of each resource
    pub res: [i32; NUM_RES],
    pub pop: i32,
    pub pop_cap: i32,
    pub techs: Vec<bool>,
    pub researching: Vec<bool>,
    pub mods: Vec<UnitMods>,
    pub defeated: bool,
    pub stats: PlayerStats,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimEvent {
    Shot { from: EntityId, to: EntityId, from_pos: FVec, to_pos: FVec, weapon: u8 },
    Impact { pos: FVec, dmg_type: u8, splash: Fx, owner: u8 },
    Damaged { id: EntityId, owner: u8, attacker_owner: u8, pos: FVec },
    Died { id: EntityId, def: DefId, owner: u8, pos: FVec, killer_owner: u8 },
    Spawned { id: EntityId, def: DefId, owner: u8 },
    BuildingPlaced { id: EntityId, def: DefId, owner: u8 },
    BuildingComplete { id: EntityId, def: DefId, owner: u8 },
    ResearchComplete { owner: u8, tech: u16 },
    ResourceDepleted { id: EntityId, pos: FVec },
    Gathered { owner: u8, res: u8, amount: i32 },
    /// command feedback for the issuing player's UI
    Notice { owner: u8, text: &'static str },
    PlayerDefeated { player: u8 },
    GameOver { winner_team: Option<u8> },
}

#[derive(Clone, Debug)]
pub struct Proj {
    pub owner: u8,
    pub src: EntityId,
    pub target: EntityId,
    pub pos: FVec,
    pub aim: FVec,
    pub start: FVec,
    pub speed: Fx,
    pub damage: i32,
    pub dmg_type: crate::defs::DamageType,
    pub splash: Fx,
    pub homing: bool,
    pub weapon: u8,
    pub src_def: DefId,
    /// air/ground/water flags of the shooter's weapon
    pub vs_air: bool,
    pub id: u32,
}

pub struct World {
    pub tick: u32,
    pub config: MatchConfig,
    pub map: Map,
    pub entities: Vec<Entity>,
    free: Vec<u32>,
    pub players: Vec<Player>,
    pub rng: SimRng,
    pub projectiles: Vec<Proj>,
    pub next_proj: u32,
    /// per player, per tile: 0 unexplored, 1 explored, 2 visible
    pub vision: Vec<Vec<u8>>,
    pub events: Vec<SimEvent>,
    pub game_over: bool,
    pub winner_team: Option<u8>,
    pub starts: Vec<(i32, i32)>,
    pub fish: Vec<EntityId>,
    // ---- caches (not game state; rebuilt deterministically)
    pub(crate) spatial: SpatialHash,
    pub(crate) scratch: PathScratch,
    pub(crate) flows: Vec<FlowField>,
}

impl World {
    pub fn data(&self) -> &'static GameData {
        data()
    }

    pub fn new(config: MatchConfig) -> World {
        let d = data();
        let gen = mapgen::generate(&MapParams {
            seed: config.seed,
            players: config.players.len(),
            size: config.map_size,
            resources: config.resources,
        });
        let n = (gen.map.w * gen.map.h) as usize;
        let players = config
            .players
            .iter()
            .enumerate()
            .map(|(i, pc)| Player {
                id: i as u8,
                team: pc.team,
                color: pc.color,
                name: pc.name.clone(),
                is_ai: pc.is_ai,
                res: config.start_res,
                pop: 0,
                pop_cap: 0,
                techs: vec![false; d.techs.len()],
                researching: vec![false; d.techs.len()],
                mods: vec![UnitMods::default(); d.defs.len()],
                defeated: false,
                stats: PlayerStats::default(),
            })
            .collect::<Vec<_>>();
        let np = players.len();
        let w = gen.map.w;
        let mut world = World {
            tick: 0,
            map: gen.map,
            entities: Vec::with_capacity(8192),
            free: Vec::new(),
            players,
            rng: SimRng::new(config.seed, 0xe3),
            projectiles: Vec::new(),
            next_proj: 1,
            vision: vec![vec![0u8; n]; np],
            events: Vec::new(),
            game_over: false,
            winner_team: None,
            starts: gen.starts,
            fish: Vec::new(),
            spatial: SpatialHash::new(w, w),
            scratch: PathScratch::default(),
            flows: Vec::new(),
            config,
        };
        // slot 0 is reserved so that id 0 means "none"
        let mut dummy = Entity::new(0, 0, GAIA, FVec::ZERO, 0);
        dummy.alive = false;
        world.entities.push(dummy);
        for o in &gen.objects {
            let def = d.id(o.key);
            let dd = d.def(def);
            if dd.is_building() || (dd.is_resource() && o.key != "fish") {
                world.spawn_static(def, o.owner, (o.x, o.y), true);
            } else {
                let pos = if dd.is_resource() { FVec::tile_center(o.x, o.y) } else { FVec::tile_center(o.x, o.y) };
                let id = world.spawn(def, o.owner, pos);
                if o.key == "fish" {
                    world.fish.push(id);
                }
            }
        }
        world.recount_pop();
        world.update_vision();
        world.events.clear();
        world
    }

    // ------------------------------------------------------------------ access

    #[inline]
    pub fn get(&self, id: EntityId) -> Option<&Entity> {
        if id == 0 {
            return None;
        }
        let e = self.entities.get(slot_of(id))?;
        if e.alive && e.id == id {
            Some(e)
        } else {
            None
        }
    }
    #[inline]
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Entity> {
        if id == 0 {
            return None;
        }
        let e = self.entities.get_mut(slot_of(id))?;
        if e.alive && e.id == id {
            Some(e)
        } else {
            None
        }
    }
    #[inline]
    pub fn def_of(&self, e: &Entity) -> &'static Def {
        data().def(e.def)
    }
    pub fn is_enemy(&self, a: u8, b: u8) -> bool {
        if a == GAIA || b == GAIA || a == b {
            return false;
        }
        match (self.players.get(a as usize), self.players.get(b as usize)) {
            (Some(pa), Some(pb)) => pa.team != pb.team,
            _ => false,
        }
    }
    pub fn mods(&self, owner: u8, def: DefId) -> UnitMods {
        self.players.get(owner as usize).map(|p| p.mods[def as usize]).unwrap_or_default()
    }
    pub fn max_hp(&self, e: &Entity) -> i32 {
        let base = self.def_of(e).data.hp;
        let m = self.mods(e.owner, e.def);
        base * (100 + m.hp_pct) / 100
    }
    pub fn unit_speed(&self, e: &Entity) -> Fx {
        let d = self.def_of(e);
        let m = self.mods(e.owner, e.def);
        Fx(d.speed.0 * (100 + m.speed_pct) / 100)
    }

    /// Footprint center for multi-tile objects, position for units.
    pub fn center_of(&self, e: &Entity) -> FVec {
        e.pos
    }

    /// Distance from `p` to the edge of entity `e` (footprint rectangle for
    /// buildings/resources, circle for units).
    pub fn edge_dist(&self, p: FVec, e: &Entity) -> Fx {
        let d = self.def_of(e);
        if d.is_building() || (d.is_resource() && d.size() != (1, 1)) {
            let (sw, sh) = d.size();
            let x0 = Fx::from_int(e.tile.0);
            let y0 = Fx::from_int(e.tile.1);
            let x1 = Fx::from_int(e.tile.0 + sw);
            let y1 = Fx::from_int(e.tile.1 + sh);
            let dx = if p.x < x0 { x0 - p.x } else if p.x > x1 { p.x - x1 } else { Fx::ZERO };
            let dy = if p.y < y0 { y0 - p.y } else if p.y > y1 { p.y - y1 } else { Fx::ZERO };
            FVec::new(dx, dy).len()
        } else {
            let r = d.radius;
            let dist = p.dist(e.pos);
            (dist - r).max(Fx::ZERO)
        }
    }

    // ------------------------------------------------------------------ spawning

    fn alloc_slot(&mut self) -> (usize, EntityId) {
        if let Some(slot) = self.free.pop() {
            let old = self.entities[slot as usize].id;
            let gen = ((old >> SLOT_BITS) + 1) & 0xfff;
            let gen = if gen == 0 { 1 } else { gen };
            (slot as usize, (gen << SLOT_BITS) | slot)
        } else {
            let slot = self.entities.len();
            assert!(slot < SLOT_MASK as usize, "entity limit");
            self.entities.push(Entity::new(0, 0, GAIA, FVec::ZERO, 0));
            (slot, (1 << SLOT_BITS) | slot as u32)
        }
    }

    /// Spawn a mobile unit (or fish) at a position.
    pub fn spawn(&mut self, def: DefId, owner: u8, pos: FVec) -> EntityId {
        let (slot, id) = self.alloc_slot();
        let d = data().def(def);
        let mut e = Entity::new(id, def, owner, pos, 1);
        e.hp = d.data.hp * (100 + self.mods(owner, def).hp_pct) / 100;
        e.amount = d.data.amount * self.config.resources.clamp(50, 300) as i32 / 100;
        e.fuel = d.fuel_ticks;
        e.ammo = d.weapons.first().map(|w| w.ammo).unwrap_or(0);
        // face away from map center-ish deterministic default
        e.facing = FVec::new(Fx::ZERO, Fx::ONE);
        self.entities[slot] = e;
        if d.is_unit() && owner != GAIA {
            self.events.push(SimEvent::Spawned { id, def, owner });
        }
        id
    }

    /// Spawn a building or static resource on tiles.
    pub fn spawn_static(&mut self, def: DefId, owner: u8, tile: (i32, i32), complete: bool) -> EntityId {
        let d = data().def(def);
        let (sw, sh) = d.size();
        let pos = FVec::new(
            Fx(tile.0 * ONE + sw * ONE / 2),
            Fx(tile.1 * ONE + sh * ONE / 2),
        );
        let id = self.spawn(def, owner, pos);
        let walkable = d.data.walkable;
        let e = self.get_mut(id).unwrap();
        e.tile = tile;
        e.complete = complete;
        if d.is_building() {
            if !complete {
                e.hp = 1.max(d.data.hp / 10);
                e.progress = 0;
            } else {
                e.progress = d.build_ticks;
            }
            // buildings face south by default (toward +y)
        }
        self.map.occupy(tile.0, tile.1, sw, sh, id, walkable);
        self.invalidate_flows();
        id
    }

    pub(crate) fn invalidate_flows(&mut self) {
        let v = self.map.version;
        self.flows.retain(|f| f.version == v);
    }

    /// Remove an entity from the world.
    pub fn kill(&mut self, id: EntityId, killer_owner: u8) {
        let Some(e) = self.get(id) else { return };
        let def = e.def;
        let owner = e.owner;
        let pos = e.pos;
        let tile = e.tile;
        let cargo = e.cargo.clone();
        let inside = e.inside;
        let d = data().def(def);
        if d.is_building() || (d.is_resource() && !self.fish.contains(&id)) {
            let (sw, sh) = d.size();
            self.map.vacate(tile.0, tile.1, sw, sh, id);
            self.invalidate_flows();
        }
        if inside != 0 {
            if let Some(t) = self.get_mut(inside) {
                t.cargo.retain(|&c| c != id);
            }
        }
        {
            let e = &mut self.entities[slot_of(id)];
            e.alive = false;
            e.cargo.clear();
        }
        self.free.push(slot_of(id) as u32);
        // anything inside dies with it (transport cargo, landed aircraft)
        for c in cargo {
            self.kill(c, killer_owner);
        }
        if d.is_resource() {
            self.events.push(SimEvent::ResourceDepleted { id, pos });
            if let Some(i) = self.fish.iter().position(|&f| f == id) {
                self.fish.remove(i);
            }
            return;
        }
        if owner != GAIA {
            if let Some(p) = self.players.get_mut(owner as usize) {
                if d.is_building() {
                    p.stats.razed += 1;
                } else {
                    p.stats.lost += 1;
                }
            }
            if killer_owner != GAIA && killer_owner != owner {
                if let Some(p) = self.players.get_mut(killer_owner as usize) {
                    p.stats.kills += 1;
                }
            }
        }
        self.events.push(SimEvent::Died { id, def, owner, pos, killer_owner });
        // planes landed at a destroyed airfield are lost
        if d.data.airport {
            let landed: Vec<EntityId> = self
                .entities
                .iter()
                .filter(|x| x.alive && x.inside == id)
                .map(|x| x.id)
                .collect();
            for l in landed {
                self.kill(l, killer_owner);
            }
        }
        if d.is_building() {
            self.recount_pop();
        }
    }

    pub fn recount_pop(&mut self) {
        let d = data();
        for p in &mut self.players {
            p.pop = 0;
            p.pop_cap = 0;
        }
        for e in &self.entities {
            if !e.alive || e.owner == GAIA {
                continue;
            }
            let dd = d.def(e.def);
            let Some(p) = self.players.get_mut(e.owner as usize) else { continue };
            if dd.is_unit() {
                p.pop += dd.data.pop;
            } else if dd.is_building() && e.complete {
                p.pop_cap += dd.data.provides_pop;
            }
        }
        // units being trained count toward population
        for e in &self.entities {
            if !e.alive || e.owner == GAIA || e.production.is_empty() {
                continue;
            }
            if let Some(ProdItem::Unit(u)) = e.production.first() {
                if let Some(p) = self.players.get_mut(e.owner as usize) {
                    p.pop += d.def(*u).data.pop;
                }
            }
        }
        let lim = self.config.pop_limit;
        for p in &mut self.players {
            p.pop_cap = p.pop_cap.min(lim);
        }
    }

    // ------------------------------------------------------------------ step

    /// Advance one tick, applying `cmds` first. This is the lockstep contract.
    pub fn step(&mut self, cmds: &TickCommands) {
        self.events.clear();
        for e in self.entities.iter_mut() {
            if e.alive {
                e.prev_pos = e.pos;
            }
        }
        for c in &cmds.commands {
            self.apply_command(c);
        }
        if self.game_over {
            self.tick += 1;
            return;
        }
        self.spatial.rebuild(&self.entities, data());
        self.update_buildings();
        self.update_units();
        self.update_projectiles();
        if self.tick % 4 == 0 {
            self.update_vision();
        }
        if self.tick % 20 == 0 {
            self.recount_pop();
            self.check_victory();
            // drop flow fields nobody used for a while
            let t = self.tick;
            self.flows.retain(|f| t.wrapping_sub(f.last_used) < 600);
        }
        self.tick += 1;
    }

    pub fn apply_command(&mut self, c: &Command) {
        crate::orders::apply(self, c);
    }

    fn check_victory(&mut self) {
        let np = self.players.len();
        let mut alive = vec![false; np];
        for e in &self.entities {
            if e.alive && (e.owner as usize) < np {
                let d = data().def(e.def);
                if d.is_unit() || d.is_building() {
                    alive[e.owner as usize] = true;
                }
            }
        }
        for i in 0..np {
            if !alive[i] && !self.players[i].defeated {
                self.players[i].defeated = true;
                self.events.push(SimEvent::PlayerDefeated { player: i as u8 });
            }
        }
        let mut teams: Vec<u8> = self.players.iter().filter(|p| !p.defeated).map(|p| p.team).collect();
        teams.sort();
        teams.dedup();
        if np > 1 && teams.len() <= 1 {
            self.game_over = true;
            self.winner_team = teams.first().copied();
            self.events.push(SimEvent::GameOver { winner_team: self.winner_team });
        }
    }

    pub fn resign(&mut self, player: u8) {
        let ids: Vec<EntityId> = self.entities.iter().filter(|e| e.alive && e.owner == player).map(|e| e.id).collect();
        for id in ids {
            self.kill(id, GAIA);
        }
        self.check_victory();
    }

    /// Hash of all game state. Peers compare these to detect desyncs.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        let mut mix = |v: u64| {
            h ^= v;
            h = h.wrapping_mul(0x100000001b3);
        };
        mix(self.tick as u64);
        mix(self.rng.state_hash());
        for e in &self.entities {
            if !e.alive {
                continue;
            }
            mix(e.id as u64);
            mix(e.def as u64 | (e.owner as u64) << 16);
            mix(e.pos.x.0 as u32 as u64 | (e.pos.y.0 as u32 as u64) << 32);
            mix(e.hp as u32 as u64);
            mix(e.carry as u64 | (e.amount as u32 as u64) << 32);
            mix(e.progress as u32 as u64 | (e.inside as u64) << 32);
        }
        for p in &self.players {
            for r in p.res {
                mix(r as u32 as u64);
            }
        }
        for pr in &self.projectiles {
            mix(pr.pos.x.0 as u32 as u64 | (pr.pos.y.0 as u32 as u64) << 32);
        }
        mix(self.map.checksum());
        h
    }

    // ------------------------------------------------------------------ queries

    pub fn can_afford(&self, player: u8, cost: &crate::defs::Cost) -> bool {
        let p = &self.players[player as usize];
        cost.arr().iter().zip(p.res.iter()).all(|(c, r)| r >= c)
    }
    pub fn pay(&mut self, player: u8, cost: &crate::defs::Cost) -> bool {
        if !self.can_afford(player, cost) {
            return false;
        }
        let p = &mut self.players[player as usize];
        for (i, c) in cost.arr().iter().enumerate() {
            p.res[i] -= c;
        }
        true
    }
    pub fn refund(&mut self, player: u8, cost: &crate::defs::Cost) {
        let p = &mut self.players[player as usize];
        for (i, c) in cost.arr().iter().enumerate() {
            p.res[i] += c;
        }
    }

    /// Is the tile visible to `player` right now?
    #[inline]
    pub fn visible(&self, player: u8, x: i32, y: i32) -> bool {
        if self.config.reveal {
            return true;
        }
        match self.vision.get(player as usize) {
            Some(v) if self.map.in_bounds(x, y) => v[self.map.idx(x, y)] == 2,
            _ => false,
        }
    }
    pub fn explored(&self, player: u8, x: i32, y: i32) -> bool {
        if self.config.reveal {
            return true;
        }
        match self.vision.get(player as usize) {
            Some(v) if self.map.in_bounds(x, y) => v[self.map.idx(x, y)] >= 1,
            _ => false,
        }
    }
    /// Can `player` see entity `e`? (own/allied always; others if any covered tile is visible)
    pub fn can_see(&self, player: u8, e: &Entity) -> bool {
        if !e.on_map() {
            return e.owner == player;
        }
        if e.owner == player {
            return true;
        }
        if let (Some(a), Some(b)) = (self.players.get(player as usize), self.players.get(e.owner as usize)) {
            if a.team == b.team {
                return true;
            }
        }
        let (x, y) = e.pos.tile();
        self.visible(player, x, y)
    }

    /// Validate a building placement for `player`.
    pub fn can_place(&self, player: u8, def: DefId, tile: (i32, i32)) -> Result<(), &'static str> {
        let d = data().def(def);
        if !d.is_building() {
            return Err("not a building");
        }
        let (sw, sh) = d.size();
        let (x0, y0) = tile;
        let mut touches_deep = false;
        for y in y0..y0 + sh {
            for x in x0..x0 + sw {
                if !self.map.in_bounds(x, y) {
                    return Err("out of bounds");
                }
                let i = self.map.idx(x, y);
                if self.map.occupant[i] != 0 {
                    return Err("blocked");
                }
                if self.map.base_pass[i] & crate::map::PASS_LAND == 0 {
                    if d.data.coastal && self.map.is_water(x, y) {
                        // naval yards may hang over the shore
                        if self.map.base_pass[i] & crate::map::PASS_WATER != 0 {
                            touches_deep = true;
                        }
                        continue;
                    }
                    return Err("unbuildable terrain");
                }
                if !self.explored(player, x, y) {
                    return Err("unexplored");
                }
            }
        }
        if d.data.coastal {
            // must have at least some land and touch water
            if !touches_deep {
                let mut near = false;
                for y in y0 - 1..=y0 + sh {
                    for x in x0 - 1..=x0 + sw {
                        if self.map.in_bounds(x, y) && self.map.base_pass[self.map.idx(x, y)] & crate::map::PASS_WATER != 0 {
                            near = true;
                        }
                    }
                }
                if !near {
                    return Err("must be built on the coast");
                }
            }
            let mut land = 0;
            for y in y0..y0 + sh {
                for x in x0..x0 + sw {
                    if self.map.is_land(x, y) {
                        land += 1;
                    }
                }
            }
            if land < sw * sh / 3 {
                return Err("needs solid ground");
            }
        }
        if let Some(near_def) = d.near {
            let mut ok = false;
            for y in y0 - 1..=y0 + sh {
                for x in x0 - 1..=x0 + sw {
                    if !self.map.in_bounds(x, y) {
                        continue;
                    }
                    let occ = self.map.occupant[self.map.idx(x, y)];
                    if let Some(o) = self.get(occ) {
                        if o.def == near_def && o.owner == player {
                            ok = true;
                        }
                    }
                }
            }
            if !ok {
                return Err("must be next to a granary");
            }
        }
        // units of other players standing there block placement
        Ok(())
    }

    /// Nearest own completed dropsite for a resource.
    pub fn nearest_dropsite(&self, owner: u8, res: u8, from: FVec, layer: Layer) -> Option<EntityId> {
        let d = data();
        let mut best: Option<(i64, EntityId)> = None;
        for e in &self.entities {
            if !e.alive || e.owner != owner || !e.complete {
                continue;
            }
            let dd = d.def(e.def);
            if !dd.is_building() || !dd.data.dropsite.iter().any(|r| *r as u8 == res) {
                continue;
            }
            // boats can only use coastal dropsites, land units can't drop at sea-only
            if layer == Layer::Water && !dd.data.coastal {
                continue;
            }
            let dist = from.dist2_raw(e.pos);
            if best.map_or(true, |(bd, _)| dist < bd) {
                best = Some((dist, e.id));
            }
        }
        best.map(|b| b.1)
    }

    /// Find the nearest resource node of a type around `from` within `radius` tiles.
    pub fn nearest_resource(&self, res: u8, from: FVec, radius: i32, water: bool) -> Option<EntityId> {
        let d = data();
        if water {
            let mut best: Option<(i64, EntityId)> = None;
            for &f in &self.fish {
                if let Some(e) = self.get(f) {
                    let dd = d.def(e.def);
                    if dd.data.resource.map(|r| r as u8) != Some(res) {
                        continue;
                    }
                    let dist = from.dist2_raw(e.pos);
                    let lim = (radius as i64 * ONE as i64).pow(2);
                    if dist <= lim && best.map_or(true, |(bd, _)| dist < bd) {
                        best = Some((dist, f));
                    }
                }
            }
            return best.map(|b| b.1);
        }
        let (cx, cy) = from.tile();
        let mut best: Option<(i64, EntityId)> = None;
        for r in 0..=radius {
            // once we have a hit, finish this ring and the next (diagonals) then stop
            if let Some((bd, _)) = best {
                let ring_d = ((r - 1).max(0) as i64 * ONE as i64).pow(2);
                if ring_d > bd {
                    break;
                }
            }
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (x, y) = (cx + dx, cy + dy);
                    if !self.map.in_bounds(x, y) {
                        continue;
                    }
                    let occ = self.map.occupant[self.map.idx(x, y)];
                    if occ == 0 {
                        continue;
                    }
                    let Some(e) = self.get(occ) else { continue };
                    let dd = d.def(e.def);
                    if !dd.is_resource() || dd.data.resource.map(|r| r as u8) != Some(res) {
                        continue;
                    }
                    // spread workers: skip crowded nodes
                    let cap = if dd.size() == (1, 1) { 2 } else { 9 };
                    if e.gatherers >= cap {
                        continue;
                    }
                    let dist = from.dist2_raw(e.pos);
                    if best.map_or(true, |(bd, _)| dist < bd) {
                        best = Some((dist, occ));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }

    /// A free tile next to a footprint where a unit of `layer` can appear.
    pub fn exit_tile(&self, e: &Entity, layer: Layer, toward: Option<FVec>) -> Option<FVec> {
        let d = self.def_of(e);
        let (sw, sh) = d.size();
        let (x0, y0) = e.tile;
        let mut best: Option<(i64, (i32, i32))> = None;
        let target = toward.unwrap_or(FVec::new(e.pos.x, e.pos.y + Fx::from_int(sh)));
        for ring in 1..6 {
            for y in y0 - ring..y0 + sh + ring {
                for x in x0 - ring..x0 + sw + ring {
                    let edge = x == x0 - ring || x == x0 + sw + ring - 1 || y == y0 - ring || y == y0 + sh + ring - 1;
                    if !edge || !self.map.passable(x, y, layer) {
                        continue;
                    }
                    let c = FVec::tile_center(x, y);
                    let dist = c.dist2_raw(target);
                    if best.map_or(true, |(bd, _)| dist < bd) {
                        best = Some((dist, (x, y)));
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        best.map(|(_, (x, y))| FVec::tile_center(x, y))
    }

    pub fn class_of(&self, id: EntityId) -> Option<Class> {
        self.get(id).map(|e| self.def_of(e).class())
    }
}
