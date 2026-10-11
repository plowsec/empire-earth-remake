//! Conquered islands become fortresses: the town center keeps training settlers, and
//! once the army has cleared the ground they ring the coast with towers (one AA site for
//! every three), add anti-ICBM sites, a naval yard and an airport. Ships and fighters
//! from there patrol around the island, and every outpost raises the fleet and air force
//! the AI wants - control of the waters and skies, not a fight.
use crate::{Ai, Difficulty, View};
use ee_sim::command::CommandKind;
use ee_sim::defs::{Class, DefId};
use ee_sim::entity::{EntityId, Order};
use ee_sim::fixed::FVec;
use ee_sim::world::{data, World};

/// Local settlers an outpost trains for itself (on top of the empire's workforce):
/// eight per resource field on the island, never fewer than this.
const MIN_LOCAL_CITIZENS: usize = 12;
const PER_FIELD: usize = 8;

impl Ai {
    /// Islands other than home with a finished town center of ours.
    pub(crate) fn outposts(&self, w: &World) -> Vec<(usize, EntityId)> {
        let sett = data().id("settlement");
        let mut out: Vec<(usize, EntityId)> = Vec::new();
        for e in &w.entities {
            if e.alive && e.owner == self.player && e.def == sett && e.complete {
                if let Some(isl) = self.island_at(w, e.tile).or_else(|| self.island_at(w, (e.tile.0 - 1, e.tile.1))) {
                    if isl != self.home_island && !out.iter().any(|o| o.0 == isl) {
                        out.push((isl, e.id));
                    }
                }
            }
        }
        out
    }

    /// Coastal land tiles spread around an island (one per `n` directions).
    pub(crate) fn coast_ring(&self, w: &World, isl: usize, n: i32) -> Vec<(i32, i32)> {
        let Some(info) = self.islands.get(isl) else { return vec![] };
        let c = info.center;
        let mut ring = Vec::new();
        for k in 0..n {
            let (cs, sn) = ee_sim::mapgen::sincos_deg(k * 360 / n);
            let mut last = None;
            for r in 0..120 {
                let t = (c.0 + cs * r / 1024, c.1 + sn * r / 1024);
                if self.island_at(w, t) == Some(isl) {
                    last = Some(t);
                } else if last.is_some() {
                    break;
                }
            }
            if let Some(t) = last {
                ring.push(t);
            }
        }
        ring
    }

    fn on_island(&self, w: &World, e: &ee_sim::entity::Entity, isl: usize) -> bool {
        self.island_at(w, e.tile).or_else(|| self.island_at(w, (e.tile.0 - 1, e.tile.1))).or_else(|| self.island_at(w, e.pos.tile())) == Some(isl)
    }

    /// Our buildings (and construction sites) of a kind on an island.
    fn count_on(&self, w: &World, v: &View, def: DefId, isl: usize) -> usize {
        let built = v.buildings.get(&def).map_or(0, |b| b.iter().filter(|&&id| w.get(id).map_or(false, |e| self.on_island(w, e, isl))).count());
        let sites = v.sites.iter().filter(|&&id| w.get(id).map_or(false, |e| e.def == def && self.on_island(w, e, isl))).count();
        built + sites
    }

    /// A naval yard plot on an island's coast with open water next to it.
    fn outpost_yard_site(&self, w: &World, isl: usize, def: DefId) -> Option<(i32, i32)> {
        let (sw, sh) = data().def(def).size();
        for t in self.coast_ring(w, isl, 16) {
            for dy in -4..=2 {
                for dx in -4..=2 {
                    let p = (t.0 + dx, t.1 + dy);
                    if self.site_ok(w, def, p) && self.deep_water_near(w, p.0, p.1, sw, sh) && self.island_at(w, (p.0 + sw / 2, p.1 + sh / 2)).map_or(true, |i| i == isl) {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    pub(crate) fn fortify_outposts(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        let tick = w.tick;
        let outposts = self.outposts(w);
        let pl = &w.players[self.player as usize];
        let cid = d.id("citizen");
        for (k, &(isl, tc)) in outposts.iter().enumerate() {
            // each outpost works on its own beat
            if (tick + k as u32 * 37) % 60 >= self.diff.think_interval() {
                continue;
            }
            let locals: Vec<EntityId> = v.citizens.iter().copied().filter(|&c| w.get(c).map_or(false, |e| e.inside == 0 && self.island_at(w, e.pos.tile()) == Some(isl))).collect();
            // settlers, straight away: eight for every mine still in the ground here
            let fields = w.entities.iter().filter(|e| {
                e.alive && e.amount > 0 && d.def(e.def).data.key.ends_with("_mine") && self.on_island(w, e, isl)
            }).count();
            let wanted = (fields * PER_FIELD).max(MIN_LOCAL_CITIZENS);
            // every town center on the island trains them
            let tcs: Vec<EntityId> = v.buildings.get(&d.id("settlement")).map(|b| b.iter().copied().filter(|&t| w.get(t).map_or(false, |e| e.complete && self.on_island(w, e, isl))).collect()).unwrap_or_else(|| vec![tc]);
            let mut queued: usize = tcs.iter().filter_map(|&t| w.get(t)).map(|e| e.production.len()).sum();
            let mut food = pl.res[0];
            for &t in &tcs {
                let q = w.get(t).map_or(9, |e| e.production.len());
                if locals.len() + queued < wanted && q < 2 && food >= 100 && pl.pop + (queued as i32) < pl.pop_cap {
                    out.push(CommandKind::Train { building: t, def: cid, count: 2 });
                    queued += 2;
                    food -= 100;
                }
            }
            // build only on clean ground
            let hostile = w.entities.iter().any(|e| {
                e.alive && e.inside == 0 && w.is_enemy(self.player, e.owner) && w.can_see(self.player, e)
                    && matches!(d.def(e.def).class(), Class::Infantry | Class::Vehicle) && self.island_at(w, e.pos.tile()) == Some(isl)
            });
            if hostile {
                continue;
            }
            let free: Vec<EntityId> = locals.iter().copied().filter(|c| !v.builders.contains(c)).collect();
            if free.is_empty() {
                continue;
            }
            // the plan: coastal towers (size-scaled), AA for every three, anti-ICBM, a yard, an airfield
            let ring_n = (self.islands[isl].tiles as f64).sqrt() as i32 * 2 / 3;
            let towers_wanted = (ring_n as usize).clamp(4, 14);
            let tower = d.id("guard_tower");
            let aa = d.id("aa_site");
            let abm = d.id("abm_site");
            let yard = d.id("naval_yard");
            let field = d.id("airport");
            let towers = self.count_on(w, v, tower, isl);
            let aas = self.count_on(w, v, aa, isl);
            let abms = self.count_on(w, v, abm, isl);
            let yards = self.count_on(w, v, yard, isl);
            let fields = self.count_on(w, v, field, isl);
            let abm_wanted = if self.diff >= Difficulty::Hard { 2 + (self.missile_alert > 0) as usize } else { 0 };
            let mut todo: Vec<DefId> = Vec::new();
            if towers < towers_wanted / 2 { todo.push(tower); }
            if aas * 3 < towers.max(3) { todo.push(aa); }
            if yards == 0 { todo.push(yard); }
            if fields == 0 && self.islands[isl].tiles >= 120 { todo.push(field); }
            if towers < towers_wanted { todo.push(tower); }
            if abms < abm_wanted { todo.push(abm); }
            let mut started = 0;
            let mut builders = free.into_iter();
            for def in todo {
                if started >= 2 {
                    break;
                }
                if !w.can_afford(self.player, &d.def(def).data.cost) {
                    continue;
                }
                let tile = if def == yard {
                    self.outpost_yard_site(w, isl, def)
                } else if def == tower || def == aa {
                    // spread around the coast: the ring point farthest from what we have
                    let have: Vec<FVec> = w.entities.iter().filter(|e| e.alive && e.owner == self.player && (e.def == tower || e.def == aa) && self.on_island(w, e, isl)).map(|e| e.pos).collect();
                    let ring = self.coast_ring(w, isl, (towers_wanted as i32).max(6));
                    let spot = ring.into_iter().max_by_key(|t| {
                        let p = FVec::tile_center(t.0, t.1);
                        have.iter().map(|h| h.dist2_raw(p)).min().unwrap_or(i64::MAX)
                    });
                    spot.and_then(|t| {
                        // a couple of tiles inland
                        let c = self.islands[isl].center;
                        let inl = (t.0 + (c.0 - t.0).signum() * 2, t.1 + (c.1 - t.1).signum() * 2);
                        self.plot_near(w, def, inl, isl)
                    })
                } else {
                    let te = w.get(tc).map(|e| e.tile).unwrap_or(self.islands[isl].center);
                    self.plot_near(w, def, te, isl)
                };
                let Some(tile) = tile else { continue };
                let Some(b) = builders.next() else { break };
                let mut crew = vec![b];
                if let Some(b2) = builders.next() {
                    crew.push(b2);
                }
                out.push(CommandKind::Build { units: crew, def, tile, queue: false });
                started += 1;
            }
            // what's built here guards here: fighters circle the island, ships patrol its waters
            if tick % 400 < self.diff.think_interval() {
                let c = self.islands[isl].center;
                let over = FVec::tile_center(c.0, c.1);
                for &f in v.buildings.get(&field).map(|x| x.as_slice()).unwrap_or(&[]) {
                    if let Some(e) = w.get(f) {
                        if self.on_island(w, e, isl) && e.rally.map_or(true, |r| !r.within(over, ee_sim::fixed::Fx::from_int(4))) {
                            out.push(CommandKind::SetRally { buildings: vec![f], to: over, target: 0 });
                        }
                    }
                }
                let ring = self.coast_ring(w, isl, 8);
                for (j, &y) in v.buildings.get(&yard).map(|x| x.as_slice()).unwrap_or(&[]).iter().enumerate() {
                    if let Some(e) = w.get(y) {
                        if !self.on_island(w, e, isl) || ring.is_empty() {
                            continue;
                        }
                        // a spot of open water a few tiles off a coast point
                        let t = ring[(j + (tick / 400) as usize) % ring.len()];
                        let (dx, dy) = ((t.0 - c.0).signum() * 5, (t.1 - c.1).signum() * 5);
                        let sea = (t.0 + dx, t.1 + dy);
                        if w.map.is_water(sea.0, sea.1) {
                            out.push(CommandKind::SetRally { buildings: vec![y], to: FVec::tile_center(sea.0, sea.1), target: 0 });
                        }
                    }
                }
                // idle warships near the outpost sail its circuit
                let ships: Vec<EntityId> = v.navy.iter().copied().filter(|&s| w.get(s).map_or(false, |e| e.order == Order::Idle && e.pos.within(over, ee_sim::fixed::Fx::from_int(40)))).collect();
                if !ships.is_empty() && !ring.is_empty() {
                    let t = ring[(tick / 400) as usize % ring.len()];
                    let sea = (t.0 + (t.0 - c.0).signum() * 5, t.1 + (t.1 - c.1).signum() * 5);
                    if w.map.is_water(sea.0, sea.1) {
                        out.push(CommandKind::Move { units: ships, to: FVec::tile_center(sea.0, sea.1), attack_move: true, queue: false });
                    }
                }
            }
        }
    }
}
