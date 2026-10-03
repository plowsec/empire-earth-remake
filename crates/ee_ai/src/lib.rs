//! Computer opponent. Reads the world (respecting fog of war for enemy info) and
//! returns the same `CommandKind`s a human produces. Fully deterministic: it can
//! run on every peer in lockstep, or on the host only.
use serde::{Deserialize, Serialize};
mod expand;
mod plan;
mod strategic;

use ee_sim::command::CommandKind;
use ee_sim::defs::{Class, DefId, Res, NUM_RES};
use ee_sim::entity::{EntityId, Order, ProdItem};
use ee_sim::fixed::{FVec, Fx};
use ee_sim::rng::SimRng;
use ee_sim::world::{data, World};
use ee_net::Controller;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
    Hardest,
}

impl Difficulty {
    pub fn from_index(i: i32) -> Difficulty {
        match i {
            0 => Difficulty::Easy,
            1 => Difficulty::Normal,
            2 => Difficulty::Hard,
            _ => Difficulty::Hardest,
        }
    }
    fn think_interval(self) -> u32 {
        match self {
            Difficulty::Easy => 30,
            Difficulty::Normal => 16,
            Difficulty::Hard => 10,
            Difficulty::Hardest => 6,
        }
    }
    /// workforce at which the military program kicks in
    fn econ_base(self) -> usize {
        match self {
            Difficulty::Easy => 24,
            Difficulty::Normal => 42,
            Difficulty::Hard => 80,
            Difficulty::Hardest => 100,
        }
    }
    /// workforce goal: grows with the population limit so long games keep scaling
    fn citizen_target(self, pop_limit: i32) -> usize {
        let (base, pct) = match self {
            Difficulty::Easy => (30, 14),
            Difficulty::Normal => (60, 20),
            Difficulty::Hard => (90, 24),
            Difficulty::Hardest => (110, 26),
        };
        // past a few hundred workers more citizens only starve the army of food and room
        (base.max(pop_limit * pct / 100) as usize).min(420)
    }
    /// first attack, in ticks
    fn first_attack(self) -> u32 {
        let min = match self {
            Difficulty::Easy => 20,
            Difficulty::Normal => 13,
            Difficulty::Hard => 10,
            Difficulty::Hardest => 8,
        };
        min * 60 * 20
    }
    fn wave_size(self) -> usize {
        match self {
            Difficulty::Easy => 10,
            Difficulty::Normal => 18,
            Difficulty::Hard => 26,
            Difficulty::Hardest => 34,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Stage {
    Gather,
    Load,
    Sail,
    Fight,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Invasion {
    stage: Stage,
    units: Vec<EntityId>,
    transports: Vec<EntityId>,
    staging: FVec,
    staging_water: FVec,
    landing: FVec,
    target: FVec,
    stage_tick: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Known {
    def: DefId,
    pos: FVec,
    #[allow(dead_code)]
    owner: u8,
    tile: (i32, i32),
}

#[derive(Serialize, Deserialize)]
pub struct Ai {
    pub player: u8,
    pub diff: Difficulty,
    rng: SimRng,
    phase: u32,
    base: FVec,
    base_tile: (i32, i32),
    initialized: bool,
    /// enemy buildings we have seen, still believed alive
    known: BTreeMap<EntityId, Known>,
    /// recent enemy composition (decayed counts by class)
    seen_air: i32,
    seen_heavy: i32,
    seen_inf: i32,
    seen_navy: i32,
    invasion: Option<Invasion>,
    last_wave: u32,
    wave: u32,
    last_air_strike: u32,
    last_naval_push: u32,
    staging: Option<((i32, i32), (i32, i32))>,
    landings: BTreeMap<u8, ((i32, i32), (i32, i32))>,
    defending: bool,
    pending: Vec<(DefId, u32)>,
    scouted: bool,
    last_fields_rebuild: Option<u32>,
    pub(crate) islands: Vec<expand::Island>,
    pub(crate) island_of: Vec<u16>,
    pub(crate) home_island: usize,
    pub(crate) colony: Option<expand::Colony>,
    pub(crate) last_colony: u32,
    pub(crate) last_replant: u32,
    /// food workers the economy would like (drives granary/farm expansion)
    pub(crate) food_wanted: i32,
    /// last tick an enemy missile was seen in flight
    #[serde(default)]
    pub(crate) missile_alert: u32,
    #[serde(default)]
    pub(crate) last_salvo: u32,
}

impl Ai {
    pub fn new(player: u8, diff: Difficulty, seed: u64) -> Ai {
        Ai {
            player,
            diff,
            rng: SimRng::new(seed ^ 0xa1a1, player as u64 + 7),
            phase: player as u32 * 3,
            base: FVec::ZERO,
            base_tile: (0, 0),
            initialized: false,
            known: BTreeMap::new(),
            seen_air: 0,
            seen_heavy: 0,
            seen_inf: 0,
            seen_navy: 0,
            invasion: None,
            last_wave: 0,
            wave: 0,
            last_air_strike: 0,
            last_naval_push: 0,
            staging: None,
            landings: BTreeMap::new(),
            defending: false,
            pending: Vec::new(),
            scouted: false,
            last_fields_rebuild: None,
            islands: Vec::new(),
            island_of: Vec::new(),
            home_island: 0,
            colony: None,
            last_colony: 0,
            last_replant: 0,
            food_wanted: 0,
            missile_alert: 0,
            last_salvo: 0,
        }
    }
}

/// Snapshot of our own forces, rebuilt every think.
#[derive(Default)]
pub(crate) struct View {
    pub citizens: Vec<EntityId>,
    pub idle_citizens: Vec<EntityId>,
    pub gatherers: [Vec<EntityId>; NUM_RES],
    pub builders: Vec<EntityId>,
    pub buildings: BTreeMap<DefId, Vec<EntityId>>,
    pub sites: Vec<EntityId>,
    pub land_army: Vec<EntityId>,
    pub air: Vec<EntityId>,
    pub navy: Vec<EntityId>,
    pub transports: Vec<EntityId>,
    pub boats: Vec<EntityId>,
    pub idle_boats: Vec<EntityId>,
}

impl View {
    pub fn count(&self, d: DefId) -> usize {
        self.buildings.get(&d).map_or(0, |v| v.len())
    }
}

impl Ai {
    /// Restore an AI saved with `Controller::save_state`.
    pub fn from_state(text: &str) -> Option<Ai> {
        ron::from_str(text).ok()
    }
}

impl Controller for Ai {
    fn player(&self) -> u8 {
        self.player
    }

    fn save_state(&self) -> Option<String> {
        ron::to_string(self).ok()
    }

    fn debug(&self) -> String {
        let claimed = self.islands.iter().filter(|i| i.claimed).count();
        let colony = self.colony.as_ref().map(|c| format!("{:?}->isl{}", c.stage, c.island)).unwrap_or_else(|| "-".into());
        let base = match &self.invasion {
            Some(i) => format!("wave {} stage {:?} units {} transports {} landing {:?}", self.wave, i.stage, i.units.len(), i.transports.len(), i.landing.tile()),
            None => format!("wave {} (no invasion) known {} defending {}", self.wave, self.known.len(), self.defending),
        };
        format!("{base} | islands {claimed}/{} colony {colony}", self.islands.len())
    }

    fn think(&mut self, w: &World) -> Vec<CommandKind> {
        let p = self.player;
        if w.game_over || w.players.get(p as usize).map_or(true, |pl| pl.defeated) {
            return vec![];
        }
        if !self.initialized {
            self.init(w);
        }
        let interval = self.diff.think_interval();
        if (w.tick + self.phase) % interval != 0 {
            return vec![];
        }
        let mut out = Vec::new();
        let v = self.view(w);
        self.observe(w);
        self.economy(w, &v, &mut out);
        if (w.tick / interval) % 3 == (self.phase % 3) {
            self.build(w, &v, &mut out);
        } else if (w.tick / interval) % 3 == ((self.phase + 1) % 3) {
            self.claim_fields(w, &v, &mut out);
        }
        self.colonize(w, &v, &mut out);
        if (w.tick / interval) % 4 == (self.phase % 4) {
            self.strategic(w, &v, &mut out);
        }
        self.produce(w, &v, &mut out);
        self.rebuild_fields(w, &v, &mut out);
        self.military(w, &v, &mut out);
        self.scout(w, &v, &mut out);
        out
    }
}

impl Ai {
    fn init(&mut self, w: &World) {
        self.initialized = true;
        let tile_fix = |a: &mut Ai| a.map_islands(w);
        let cap = data().id("capitol");
        if let Some(c) = w.entities.iter().find(|e| e.alive && e.owner == self.player && e.def == cap) {
            self.base = c.pos;
            self.base_tile = c.pos.tile();
        } else if let Some(s) = w.starts.get(self.player as usize) {
            self.base_tile = *s;
            self.base = FVec::tile_center(s.0, s.1);
        }
        tile_fix(self);
    }

    /// Explore the coast with one early citizen, then return it to the economy.
    fn scout(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        if w.tick >= 20 * 120 {
            let scouts: Vec<_> = v.citizens.iter().copied()
                .filter(|&id| w.get(id).is_some_and(|e| matches!(e.order, Order::Scout { .. })))
                .collect();
            if !scouts.is_empty() {
                out.push(CommandKind::Stop { units: scouts });
            }
            return;
        }
        if self.scouted || w.tick < 40 { return; }
        if let Some(&c) = v.citizens.iter().find(|c| !v.builders.contains(c)) {
            self.scouted = true;
            out.push(CommandKind::Scout { units: vec![c] });
        }
    }

    pub(crate) fn view(&self, w: &World) -> View {
        let d = data();
        let mut v = View::default();
        for e in &w.entities {
            if !e.alive || e.owner != self.player {
                continue;
            }
            let dd = d.def(e.def);
            match dd.class() {
                Class::Building => {
                    if e.complete {
                        v.buildings.entry(e.def).or_default().push(e.id);
                    } else {
                        v.sites.push(e.id);
                    }
                }
                Class::Citizen => {
                    if e.inside != 0 {
                        continue;
                    }
                    v.citizens.push(e.id);
                    match e.order {
                        Order::Idle => v.idle_citizens.push(e.id),
                        Order::Gather { node } => {
                            let r = w
                                .get(node)
                                .and_then(|n| d.def(n.def).data.resource)
                                .map(|r| r.idx())
                                .unwrap_or(e.last_res as usize % NUM_RES);
                            v.gatherers[r].push(e.id);
                        }
                        Order::ReturnCargo => {
                            let r = (e.carry_res as usize).min(NUM_RES - 1);
                            v.gatherers[r].push(e.id);
                        }
                        Order::Build { .. } | Order::Repair { .. } => v.builders.push(e.id),
                        Order::Move { .. } => {
                            if e.goal.is_none() {
                                v.idle_citizens.push(e.id)
                            }
                        }
                        _ => {}
                    }
                }
                Class::Ship => {
                    if dd.data.cargo > 0 {
                        v.transports.push(e.id);
                    } else if dd.gather_rate[0] > 0 {
                        v.boats.push(e.id);
                        if e.order == Order::Idle {
                            v.idle_boats.push(e.id);
                        }
                    } else {
                        v.navy.push(e.id);
                    }
                }
                Class::Aircraft => v.air.push(e.id),
                Class::Infantry | Class::Vehicle => v.land_army.push(e.id),
                Class::Resource => {}
            }
        }
        v
    }

    /// Remember enemy buildings and composition we can currently see.
    fn observe(&mut self, w: &World) {
        let d = data();
        let p = self.player;
        // forget buildings whose tile is visible but that are gone
        let gone: Vec<EntityId> = self
            .known
            .iter()
            .filter(|(id, k)| w.visible(p, k.tile.0, k.tile.1) && w.get(**id).is_none())
            .map(|(id, _)| *id)
            .collect();
        for g in gone {
            self.known.remove(&g);
        }
        self.seen_air = self.seen_air * 15 / 16;
        self.seen_heavy = self.seen_heavy * 15 / 16;
        self.seen_inf = self.seen_inf * 15 / 16;
        self.seen_navy = self.seen_navy * 15 / 16;
        for e in &w.entities {
            if !e.alive || !w.is_enemy(p, e.owner) || !w.can_see(p, e) {
                continue;
            }
            let dd = d.def(e.def);
            match dd.class() {
                Class::Building => {
                    self.known.insert(e.id, Known { def: e.def, pos: e.pos, owner: e.owner, tile: e.pos.tile() });
                }
                Class::Aircraft => self.seen_air += 4,
                Class::Ship => self.seen_navy += 4,
                Class::Vehicle if dd.data.armor_class == ee_sim::defs::ArmorClass::Heavy => self.seen_heavy += 4,
                Class::Infantry => self.seen_inf += 4,
                _ => {}
            }
        }
    }

    // ------------------------------------------------------------------ economy

    fn economy(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        let pl = &w.players[self.player as usize];
        // train citizens from every town center (unless some are standing around)
        let target = self.diff.citizen_target(w.config.pop_limit);
        let cid = d.id("citizen");
        let in_training: usize = w
            .entities
            .iter()
            .filter(|e| e.alive && e.owner == self.player)
            .map(|e| e.production.iter().filter(|it| matches!(it, ProdItem::Unit(u) if *u == cid)).count())
            .sum();
        let per_tc = if self.diff == Difficulty::Easy { 1 } else { 3 };
        if v.citizens.len() + in_training < target && pl.pop < pl.pop_cap && v.idle_citizens.len() < 4 {
            let mut food = pl.res[0];
            for key in ["capitol", "settlement"] {
                for &b in v.buildings.get(&d.id(key)).map(|x| x.as_slice()).unwrap_or(&[]) {
                    let q = w.get(b).map_or(9, |e| e.production.len());
                    if q < per_tc && food >= 50 {
                        out.push(CommandKind::Train { building: b, def: cid, count: 1 });
                        food -= 50;
                    }
                }
            }
        }
        // new citizens walk straight to an understaffed node near their town center
        if w.tick % 200 == (self.phase * 13) % 200 {
            self.rally_town_centers(w, v, out);
        }

        // desired distribution: base weights scaled by scarcity (low stock -> more workers)
        let military_phase = v.count(d.id("barracks")) > 0;
        let base: [i32; NUM_RES] = if military_phase { [31, 28, 8, 17, 16] } else { [40, 36, 10, 7, 7] };
        let mut want: [i32; NUM_RES] = [0; NUM_RES];
        for r in 0..NUM_RES {
            want[r] = base[r] * 1000 / (300 + pl.res[r] / 2);
        }
        let total_w: i32 = want.iter().sum();
        let workers = (v.citizens.len() - v.builders.len()) as i32;
        // food work is limited by farms + berry bushes: excess workers go to the other resources
        let food_slots = self.food_slots(w);
        let mut target = [0i32; NUM_RES];
        for r in 0..NUM_RES {
            target[r] = workers * want[r] / total_w.max(1);
        }
        let f = Res::Food as usize;
        if target[f] > food_slots {
            let spare = target[f] - food_slots;
            target[f] = food_slots;
            let rest: i32 = (0..NUM_RES).filter(|&r| r != f).map(|r| want[r]).sum::<i32>().max(1);
            for r in 0..NUM_RES {
                if r != f {
                    target[r] += spare * want[r] / rest;
                }
            }
        }
        self.food_wanted = workers * want[f] / total_w.max(1);
        let mut deficit: [i32; NUM_RES] = [0; NUM_RES];
        for r in 0..NUM_RES {
            deficit[r] = target[r] - v.gatherers[r].len() as i32;
        }
        // assign idle citizens
        let mut assigned_farms: Vec<EntityId> = Vec::new();
        for &c in &v.idle_citizens {
            let Some(ce) = w.get(c) else { continue };
            let mut order: Vec<usize> = (0..NUM_RES).collect();
            order.sort_by_key(|&r| (-deficit[r], r));
            let mut done = false;
            for r in order {
                if let Some(node) = self.find_node(w, r, ce.pos, &mut assigned_farms) {
                    out.push(CommandKind::Target { units: vec![c], target: node, queue: false });
                    deficit[r] -= 1;
                    done = true;
                    break;
                }
            }
            if !done {
                // nothing to gather nearby: walk home
                if !ce.pos.within(self.base, Fx::from_int(8)) {
                    out.push(CommandKind::Move { units: vec![c], to: self.base, attack_move: false, queue: false });
                }
            }
        }

        // rebalance: shift up to 3 workers from the most over-staffed to the most needed
        if w.tick % 120 == (self.phase * 37) % 120 {
            let (mut hi, mut lo) = (0, 0);
            for r in 0..NUM_RES {
                if deficit[r] < deficit[hi] {
                    hi = r;
                }
                if deficit[r] > deficit[lo] {
                    lo = r;
                }
            }
            if deficit[hi] <= -2 && deficit[lo] >= 2 {
                let n = (-deficit[hi]).min(deficit[lo]).min(3 + workers / 40) as usize;
                let movers: Vec<EntityId> = v.gatherers[hi]
                    .iter()
                    .rev()
                    .filter(|&&c| w.get(c).map_or(false, |e| e.carry < 3 || e.carry_res as usize == lo))
                    .take(n)
                    .copied()
                    .collect();
                for c in movers {
                    if let Some(ce) = w.get(c) {
                        if let Some(node) = self.find_node(w, lo, ce.pos, &mut assigned_farms) {
                            out.push(CommandKind::Target { units: vec![c], target: node, queue: false });
                        }
                    }
                }
            }
        }

        // fishing boats
        for &b in &v.idle_boats {
            if let Some(be) = w.get(b) {
                if let Some(f) = w.nearest_resource(Res::Food as u8, be.pos, 40, true) {
                    out.push(CommandKind::Target { units: vec![b], target: f, queue: false });
                }
            }
        }
    }

    fn find_node(&self, w: &World, r: usize, from: FVec, assigned_farms: &mut Vec<EntityId>) -> Option<EntityId> {
        let d = data();
        if r == Res::Food as usize {
            // berries first, then free farms
            if let Some(n) = w.nearest_resource(r as u8, from, 18, false) {
                let nd = d.def(w.get(n)?.def);
                if !nd.is_building() {
                    return Some(n);
                }
            }
            let farm = d.id("farm");
            for e in &w.entities {
                if e.alive && e.owner == self.player && e.def == farm && e.complete && e.gatherers == 0 && !assigned_farms.contains(&e.id) {
                    assigned_farms.push(e.id);
                    return Some(e.id);
                }
            }
            return None;
        }
        // nodes on the citizen's own island first (colonists stay where they are),
        // then around any of our town centers, then anywhere near the capitol
        let here = self.island_at(w, from.tile());
        let same_island = |n: EntityId| w.get(n).map_or(false, |e| here.is_none() || self.island_at(w, (e.tile.0 - 1, e.tile.1)) == here || self.island_at(w, (e.tile.0 + 2, e.tile.1 + 1)) == here);
        if let Some(n) = w.nearest_resource(r as u8, from, 30, false).filter(|&n| same_island(n)) {
            return Some(n);
        }
        let sett = d.id("settlement");
        let cap = d.id("capitol");
        let mut centers: Vec<(i64, FVec)> = w.entities.iter()
            .filter(|e| e.alive && e.owner == self.player && e.complete && (e.def == sett || e.def == cap))
            .map(|e| (e.pos.dist2_raw(from), e.pos))
            .collect();
        centers.sort_by_key(|c| c.0);
        for (_, c) in centers.iter().take(6) {
            if let Some(n) = w.nearest_resource(r as u8, *c, 16, false).filter(|&n| same_island(n)) {
                return Some(n);
            }
        }
        w.nearest_resource(r as u8, self.base, 60, false)
    }

    /// Worker slots on our food sources: one per farm, three per berry bush near a town center.
    fn food_slots(&self, w: &World) -> i32 {
        let d = data();
        let (farm, berries) = (d.id("farm"), d.id("berries"));
        let centers: Vec<FVec> = w.entities.iter()
            .filter(|e| e.alive && e.owner == self.player && e.complete && d.def(e.def).data.dropsite.len() >= 5)
            .map(|e| e.pos)
            .collect();
        let mut n = 0;
        for e in &w.entities {
            if !e.alive {
                continue;
            }
            if e.def == farm && e.owner == self.player && e.complete {
                n += 1;
            } else if e.def == berries && e.amount > 0 && centers.iter().any(|c| c.within(e.pos, Fx::from_int(16))) {
                n += 3;
            }
        }
        n
    }

    /// Point each town center's rally at the nearest node that still has free worker slots.
    fn rally_town_centers(&self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        for key in ["capitol", "settlement"] {
            for &b in v.buildings.get(&d.id(key)).map(|x| x.as_slice()).unwrap_or(&[]) {
                let Some(be) = w.get(b) else { continue };
                // mines first (the long-game resources), then wood, then berries
                let mut pick = None;
                for r in [Res::Gold, Res::Iron, Res::Stone, Res::Wood, Res::Food] {
                    if let Some(n) = w.nearest_resource(r as u8, be.pos, 14, false) {
                        let ok = w.get(n).map_or(false, |e| {
                            let nd = d.def(e.def);
                            !nd.is_building() && (e.gatherers as i32) < ee_sim::world::gather_cap(nd)
                        });
                        if ok {
                            pick = Some(n);
                            break;
                        }
                    }
                }
                if let Some(n) = pick {
                    if be.rally_target != n {
                        let to = w.get(n).unwrap().pos;
                        out.push(CommandKind::SetRally { buildings: vec![b], to, target: n });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
