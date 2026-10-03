//! Territorial expansion: claim every resource field on the home island, defend
//! each claim, then ship colonists to every other island that has mines.
use crate::{Ai, View};
use ee_sim::command::CommandKind;
use ee_sim::defs::{Layer, Res};
use ee_sim::entity::{EntityId, Order};
use ee_sim::fixed::{FVec, Fx};
use ee_sim::map::PASS_LAND;
use ee_sim::world::{data, World};
use std::collections::VecDeque;

/// One landmass and the mines on it.
#[derive(Clone, Debug)]
pub(crate) struct Island {
    pub tiles: usize,
    pub center: (i32, i32),
    pub mines: Vec<(i32, i32)>,
    pub claimed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CStage {
    Gather,
    Load,
    Sail,
    Settle,
}

#[derive(Clone, Debug)]
pub(crate) struct Colony {
    pub stage: CStage,
    pub island: usize,
    pub citizens: Vec<EntityId>,
    pub escort: Vec<EntityId>,
    pub transport: EntityId,
    pub staging: FVec,
    pub staging_water: FVec,
    pub landing: FVec,
    pub mine: (i32, i32),
    pub stage_tick: u32,
}

impl Ai {
    /// Label every landmass once (terrain never changes) and list its mines.
    pub(crate) fn map_islands(&mut self, w: &World) {
        let map = &w.map;
        let n = (map.w * map.h) as usize;
        let mut comp = vec![u16::MAX; n];
        let mut islands = Vec::new();
        for start in 0..n {
            if comp[start] != u16::MAX || map.base_pass[start] & PASS_LAND == 0 {
                continue;
            }
            let id = islands.len() as u16;
            let mut q = VecDeque::new();
            q.push_back(start);
            comp[start] = id;
            let (mut sx, mut sy, mut cnt) = (0i64, 0i64, 0usize);
            while let Some(i) = q.pop_front() {
                let x = (i as i32) % map.w;
                let y = (i as i32) / map.w;
                sx += x as i64;
                sy += y as i64;
                cnt += 1;
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if !map.in_bounds(nx, ny) {
                        continue;
                    }
                    let j = map.idx(nx, ny);
                    if comp[j] == u16::MAX && map.base_pass[j] & PASS_LAND != 0 {
                        comp[j] = id;
                        q.push_back(j);
                    }
                }
            }
            islands.push(Island { tiles: cnt, center: ((sx / cnt as i64) as i32, (sy / cnt as i64) as i32), mines: vec![], claimed: false });
        }
        for e in &w.entities {
            if !e.alive || !data().def(e.def).data.key.ends_with("_mine") {
                continue;
            }
            // mines block their own tiles: look at the ring around them
            let (tx, ty) = e.tile;
            'r: for y in ty - 1..=ty + 2 {
                for x in tx - 1..=tx + 2 {
                    if map.in_bounds(x, y) {
                        let c = comp[map.idx(x, y)];
                        if c != u16::MAX {
                            islands[c as usize].mines.push(e.tile);
                            break 'r;
                        }
                    }
                }
            }
        }
        let (bx, by) = self.base_tile;
        self.home_island = comp.get(map.idx(bx, by)).copied().unwrap_or(0) as usize;
        if self.home_island < islands.len() {
            islands[self.home_island].claimed = true;
        }
        self.island_of = comp;
        self.islands = islands;
    }

    pub(crate) fn island_at(&self, w: &World, p: (i32, i32)) -> Option<usize> {
        if !w.map.in_bounds(p.0, p.1) {
            return None;
        }
        let c = *self.island_of.get(w.map.idx(p.0, p.1))?;
        if c == u16::MAX { None } else { Some(c as usize) }
    }

    /// Dropsite (any res) owned by us within `r` tiles of a tile.
    fn has_dropsite_near(&self, w: &World, t: (i32, i32), r: i32) -> bool {
        let p = FVec::tile_center(t.0 + 1, t.1 + 1);
        w.entities.iter().any(|e| {
            e.alive && e.owner == self.player && data().def(e.def).data.dropsite.len() >= 5 && e.pos.within(p, Fx::from_int(r))
        })
    }

    /// Settlement plot near a mine on the same island.
    fn settlement_plot(&self, w: &World, mine: (i32, i32)) -> Option<(i32, i32)> {
        let def = data().id("settlement");
        let isl = self.island_at(w, (mine.0 - 1, mine.1)).or_else(|| self.island_at(w, (mine.0 + 2, mine.1)));
        for r in 3..9 {
            for k in 0..16 {
                let (c, s) = ee_sim::mapgen::sincos_deg(k * 22);
                let t = (mine.0 + c * r / 1024 - 1, mine.1 + s * r / 1024 - 1);
                if w.can_place(self.player, def, t).is_ok() && (isl.is_none() || self.island_at(w, t) == isl) {
                    return Some(t);
                }
            }
        }
        None
    }

    /// Plot for a defensive building next to a point.
    fn defense_plot(&self, w: &World, near: FVec, key: &str) -> Option<(i32, i32)> {
        let def = data().id(key);
        let (cx, cy) = near.tile();
        let isl = self.island_at(w, (cx, cy));
        let (sw, sh) = data().def(def).size();
        for r in 3..11 {
            for k in 0..12 {
                let (c, s) = ee_sim::mapgen::sincos_deg(k * 30 + 15);
                let t = (cx + c * r / 1024 - sw / 2, cy + s * r / 1024 - sh / 2);
                if w.can_place(self.player, def, t).is_ok() && self.island_at(w, t) == isl {
                    return Some(t);
                }
            }
        }
        None
    }

    fn nearest_citizens(&self, w: &World, v: &View, near: FVec, n: usize, island: Option<usize>) -> Vec<EntityId> {
        let mut pool: Vec<(i64, EntityId)> = v
            .citizens
            .iter()
            .filter(|c| !v.builders.contains(c))
            .filter_map(|&c| w.get(c).map(|e| (e, c)))
            .filter(|(e, _)| island.map_or(true, |i| self.island_at(w, e.pos.tile()) == Some(i)))
            .map(|(e, c)| (e.pos.dist2_raw(near) + if e.carry > 0 { 1 << 40 } else { 0 }, c))
            .collect();
        pool.sort();
        pool.into_iter().take(n).map(|x| x.1).collect()
    }

    /// Home island: a Town Center by every unclaimed mine cluster, then a tower
    /// (and SAM site) guarding each Town Center.
    pub(crate) fn claim_fields(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        if self.islands.is_empty() || v.citizens.len() < 14 {
            return;
        }
        let d = data();
        let pl = &w.players[self.player as usize];
        let settlement = d.id("settlement");
        let tower = d.id("guard_tower");
        let sam = d.id("aa_site");
        let pending_settlement = v.sites.iter().any(|&s| w.get(s).map_or(false, |e| e.def == settlement))
            || self.pending.iter().any(|(k, _)| *k == settlement);
        // 1) claim fields on every island we already hold
        if !pending_settlement && w.can_afford(self.player, &d.def(settlement).data.cost) {
            let held: Vec<usize> = (0..self.islands.len()).filter(|&i| self.islands[i].claimed).collect();
            let mut best: Option<(i64, (i32, i32), usize)> = None;
            for &isl in &held {
                for &m in &self.islands[isl].mines {
                    if self.has_dropsite_near(w, m, 11) || !w.explored(self.player, m.0, m.1) {
                        continue;
                    }
                    let dd = ((m.0 - self.base_tile.0) as i64).pow(2) + ((m.1 - self.base_tile.1) as i64).pow(2);
                    if best.map_or(true, |b| dd < b.0) {
                        best = Some((dd, m, isl));
                    }
                }
            }
            if let Some((_, m, isl)) = best {
                if let Some(t) = self.settlement_plot(w, m) {
                    let at = FVec::tile_center(t.0 + 1, t.1 + 1);
                    let who = self.nearest_citizens(w, v, at, 2, Some(isl));
                    if !who.is_empty() {
                        out.push(CommandKind::Build { units: who, def: settlement, tile: t, queue: false });
                        self.pending.push((settlement, w.tick));
                        return;
                    }
                }
            }
        }
        // 2) forward airbases: an airfield on each colony island (up to 3)
        let airport = d.id("airport");
        if v.count(airport) >= 1 && !self.pending.iter().any(|(k, _)| *k == airport) && w.can_afford(self.player, &d.def(airport).data.cost) {
            let fields: Vec<(i32, i32)> = w.entities.iter().filter(|e| e.alive && e.owner == self.player && e.def == airport).map(|e| e.tile).collect();
            let forward = fields.iter().filter(|t| self.island_at(w, (t.0 - 1, t.1)) != Some(self.home_island)).count();
            if forward < 3 {
                for &s in v.buildings.get(&settlement).map(|x| x.as_slice()).unwrap_or(&[]) {
                    let Some(se) = w.get(s) else { continue };
                    let isl = self.island_at(w, (se.tile.0 - 1, se.tile.1)).or_else(|| self.island_at(w, (se.tile.0 + 3, se.tile.1)));
                    let Some(isl) = isl else { continue };
                    if isl == self.home_island || fields.iter().any(|t| self.island_at(w, (t.0 - 1, t.1)) == Some(isl) || self.island_at(w, (t.0 + 5, t.1)) == Some(isl)) {
                        continue;
                    }
                    if let Some(t) = self.defense_plot(w, se.pos, "airport") {
                        let who = self.nearest_citizens(w, v, se.pos, 2, Some(isl));
                        if !who.is_empty() {
                            out.push(CommandKind::Build { units: who, def: airport, tile: t, queue: false });
                            self.pending.push((airport, w.tick));
                            return;
                        }
                    }
                }
            }
        }
        // 3) every Town Center gets a guard tower; SAM sites once enemy air is around
        if pl.res[2] < 250 {
            return;
        }
        let want_sam = self.seen_air > 6 || self.diff >= crate::Difficulty::Hard;
        let towers: Vec<FVec> = w.entities.iter().filter(|e| e.alive && e.owner == self.player && (e.def == tower)).map(|e| e.pos).collect();
        let sams: Vec<FVec> = w.entities.iter().filter(|e| e.alive && e.owner == self.player && (e.def == sam)).map(|e| e.pos).collect();
        let pending_def = |k| self.pending.iter().any(|(p, _)| *p == k) || v.sites.iter().any(|&s| w.get(s).map_or(false, |e| e.def == k));
        for &s in v.buildings.get(&settlement).map(|x| x.as_slice()).unwrap_or(&[]) {
            let Some(se) = w.get(s) else { continue };
            let sp = se.pos;
            let isl = self.island_at(w, sp.tile());
            for (def, list, key, wanted) in [(tower, &towers, "guard_tower", true), (sam, &sams, "aa_site", want_sam)] {
                if !wanted || pending_def(def) || list.iter().any(|p| p.within(sp, Fx::from_int(9))) {
                    continue;
                }
                if !w.can_afford(self.player, &d.def(def).data.cost) {
                    continue;
                }
                if let Some(t) = self.defense_plot(w, sp, key) {
                    let who = self.nearest_citizens(w, v, sp, 1, isl);
                    if !who.is_empty() {
                        out.push(CommandKind::Build { units: who, def, tile: t, queue: false });
                        self.pending.push((def, w.tick));
                        return;
                    }
                }
            }
        }
    }

    /// Ship colonists (with an escort) to the nearest unclaimed island that has mines.
    pub(crate) fn colonize(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        if self.islands.is_empty() {
            return;
        }
        let tick = w.tick;
        let d = data();
        if self.colony.is_none() {
            if tick < 20 * 60 * 4 || v.citizens.len() < 20 || self.defending {
                return;
            }
            if tick.wrapping_sub(self.last_colony) < 20 * 30 {
                return;
            }
            let pl = &w.players[self.player as usize];
            let sc = d.def(d.id("settlement")).data.cost;
            if pl.res[1] < sc.wood + 150 || pl.res[2] < sc.stone + 200 {
                return;
            }
            let busy: Vec<EntityId> = self.invasion.as_ref().map(|i| i.transports.clone()).unwrap_or_default();
            let Some(&ship) = v.transports.iter().find(|&&t| !busy.contains(&t) && w.get(t).map_or(false, |e| e.cargo.is_empty() && matches!(e.order, Order::Idle | Order::Move { .. })))
            else {
                return;
            };
            // nearest unclaimed island with mines and no known enemy buildings
            let (bx, by) = self.base_tile;
            let mut best: Option<(i64, usize)> = None;
            for (i, isl) in self.islands.iter().enumerate() {
                if isl.claimed || isl.mines.is_empty() || isl.tiles < 60 {
                    continue;
                }
                let enemy_here = self.known.values().any(|k| self.island_at(w, k.tile) == Some(i));
                if enemy_here {
                    continue;
                }
                let dd = ((isl.center.0 - bx) as i64).pow(2) + ((isl.center.1 - by) as i64).pow(2);
                if best.map_or(true, |b| dd < b.0) {
                    best = Some((dd, i));
                }
            }
            let Some((_, target)) = best else { return };
            let tc = self.islands[target].center;
            let tcp = FVec::tile_center(tc.0, tc.1);
            // staging: our coast nearest the target; landing: their coast nearest us
            let Some((stage, stage_w)) = self.coast_toward(w, self.home_island, tcp) else { return };
            let Some((land, _)) = self.coast_toward(w, target, FVec::tile_center(stage.0, stage.1)) else { return };
            let mine = *self.islands[target].mines.iter().min_by_key(|m| (m.0 - land.0).pow(2) + (m.1 - land.1).pow(2)).unwrap();
            let staging = FVec::tile_center(stage.0, stage.1);
            let citizens = self.nearest_citizens(w, v, staging, 3, Some(self.home_island));
            if citizens.len() < 2 {
                return;
            }
            let escort: Vec<EntityId> = v
                .land_army
                .iter()
                .copied()
                .filter(|&u| w.get(u).map_or(false, |e| e.inside == 0 && e.order == Order::Idle && self.island_at(w, e.pos.tile()) == Some(self.home_island)))
                .take(7)
                .collect();
            let mut walkers = citizens.clone();
            walkers.extend(escort.iter().copied());
            out.push(CommandKind::Move { units: walkers, to: staging, attack_move: false, queue: false });
            let staging_water = FVec::tile_center(stage_w.0, stage_w.1);
            out.push(CommandKind::Move { units: vec![ship], to: staging_water, attack_move: false, queue: false });
            self.colony = Some(crate::expand::Colony {
                stage: CStage::Gather,
                island: target,
                citizens,
                escort,
                transport: ship,
                staging,
                staging_water,
                landing: FVec::tile_center(land.0, land.1),
                mine,
                stage_tick: tick,
            });
            self.last_colony = tick;
            return;
        }
        let mut c = self.colony.take().unwrap();
        c.citizens.retain(|&u| w.get(u).is_some());
        c.escort.retain(|&u| w.get(u).is_some());
        let elapsed = tick.wrapping_sub(c.stage_tick);
        if c.citizens.is_empty() || (w.get(c.transport).is_none() && c.stage != CStage::Settle) || elapsed > 20 * 60 * 4 {
            return; // mission failed: try again later
        }
        let everyone: Vec<EntityId> = c.citizens.iter().chain(c.escort.iter()).copied().collect();
        match c.stage {
            CStage::Gather => {
                let near = everyone.iter().filter(|&&u| w.get(u).map_or(false, |e| e.pos.within(c.staging, Fx::from_int(7)))).count();
                let ship_ok = w.get(c.transport).map_or(false, |e| e.pos.within(c.staging_water, Fx::from_int(6)));
                if (near * 10 >= everyone.len() * 7 && ship_ok) || elapsed > 20 * 90 {
                    out.push(CommandKind::Target { units: everyone.clone(), target: c.transport, queue: false });
                    c.stage = CStage::Load;
                    c.stage_tick = tick;
                } else if elapsed % 200 == 100 {
                    out.push(CommandKind::Move { units: vec![c.transport], to: c.staging_water, attack_move: false, queue: false });
                }
            }
            CStage::Load => {
                let aboard = everyone.iter().filter(|&&u| w.get(u).map_or(false, |e| e.inside == c.transport)).count();
                let citizens_aboard = c.citizens.iter().any(|&u| w.get(u).map_or(false, |e| e.inside == c.transport));
                if aboard == everyone.len() || (elapsed > 20 * 40 && citizens_aboard) {
                    out.push(CommandKind::Unload { units: vec![c.transport], at: c.landing });
                    c.citizens.retain(|&u| w.get(u).map_or(false, |e| e.inside == c.transport));
                    c.escort.retain(|&u| w.get(u).map_or(false, |e| e.inside == c.transport));
                    c.stage = CStage::Sail;
                    c.stage_tick = tick;
                } else if elapsed % 120 == 60 {
                    let left: Vec<EntityId> = everyone.iter().copied().filter(|&u| w.get(u).map_or(false, |e| e.inside == 0)).collect();
                    if !left.is_empty() {
                        out.push(CommandKind::Target { units: left, target: c.transport, queue: false });
                    }
                } else if elapsed > 20 * 70 {
                    return;
                }
            }
            CStage::Sail => {
                let ashore = c.citizens.iter().filter(|&&u| w.get(u).map_or(false, |e| e.inside == 0 && self.island_at(w, e.pos.tile()) == Some(c.island))).count();
                if ashore == c.citizens.len() && ashore > 0 {
                    if let Some(t) = self.settlement_plot(w, c.mine) {
                        out.push(CommandKind::Build { units: c.citizens.clone(), def: d.id("settlement"), tile: t, queue: false });
                        let site = FVec::tile_center(t.0 + 1, t.1 + 1);
                        if !c.escort.is_empty() {
                            out.push(CommandKind::Move { units: c.escort.clone(), to: site, attack_move: true, queue: false });
                        }
                        // the ship goes home for the next run
                        out.push(CommandKind::Move { units: vec![c.transport], to: c.staging_water, attack_move: false, queue: false });
                        if (c.island as usize) < self.islands.len() {
                            self.islands[c.island].claimed = true;
                        }
                        c.stage = CStage::Settle;
                        c.stage_tick = tick;
                    } else {
                        return;
                    }
                } else if elapsed % 300 == 150 {
                    out.push(CommandKind::Unload { units: vec![c.transport], at: c.landing });
                }
            }
            CStage::Settle => {
                // once the Town Center stands, the colonists get to work (claim_fields adds the tower)
                if elapsed > 20 * 60 {
                    return;
                }
            }
        }
        self.colony = Some(c);
    }

    /// (coastal land tile on `island`, adjacent ship water) nearest to `toward`.
    fn coast_toward(&self, w: &World, island: usize, toward: FVec) -> Option<((i32, i32), (i32, i32))> {
        let (tx, ty) = toward.tile();
        let mut best: Option<(i64, ((i32, i32), (i32, i32)))> = None;
        for (i, &c) in self.island_of.iter().enumerate() {
            if c as usize != island {
                continue;
            }
            let x = i as i32 % w.map.w;
            let y = i as i32 / w.map.w;
            if (x + y) % 2 != 0 {
                continue; // sparse scan is plenty
            }
            for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (wx, wy) = (x + ox * 2, y + oy * 2);
                if w.map.passable(wx, wy, Layer::Water) && w.map.is_water(x + ox, y + oy) && w.map.passable(x, y, Layer::Land) {
                    let dd = ((x - tx) as i64).pow(2) + ((y - ty) as i64).pow(2);
                    if best.map_or(true, |b| dd < b.0) {
                        best = Some((dd, ((x, y), (wx, wy))));
                    }
                }
            }
        }
        best.map(|b| b.1)
    }
}

#[allow(dead_code)]
fn _res(_: Res) {}
