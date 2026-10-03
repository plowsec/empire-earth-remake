//! Overseas campaign: where to land (many beaches, several at once), which island to
//! launch from (colonies included), forward bases on colonies, and sea control with
//! roaming warship squadrons.
use crate::{Ai, View};
use ee_sim::command::CommandKind;
use ee_sim::entity::{EntityId, Order};
use ee_sim::fixed::FVec;
use ee_sim::map::{PASS_DEEP, PASS_WATER};
use ee_sim::world::{data, World};

type Beach = ((i32, i32), (i32, i32));

fn manhattan(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

impl Ai {
    /// Landing beaches spread around an enemy's home island (cached per enemy).
    fn beaches(&mut self, w: &World, enemy: u8) -> Vec<Beach> {
        if let Some(b) = self.beaches.get(&enemy) {
            return b.clone();
        }
        let Some(&start) = w.starts.get(enemy as usize) else { return vec![] };
        let Some(isl) = self.island_at(w, start) else { return vec![] };
        let mut all: Vec<Beach> = Vec::new();
        for (i, &c) in self.island_of.iter().enumerate() {
            if c as usize != isl {
                continue;
            }
            let x = i as i32 % w.map.w;
            let y = i as i32 / w.map.w;
            if (x + y) % 2 != 0 || manhattan((x, y), start) < 20 {
                continue; // not under the capitol's guns
            }
            for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (wx, wy) = (x + ox * 2, y + oy * 2);
                if w.map.in_bounds(wx, wy) && w.map.base_pass[w.map.idx(wx, wy)] & PASS_WATER != 0 && w.map.is_water(x + ox, y + oy) {
                    all.push(((x, y), (wx, wy)));
                    break;
                }
            }
        }
        // greedy spread: beaches at least 18 tiles apart
        let mut picked: Vec<Beach> = Vec::new();
        for b in all {
            if picked.iter().all(|p| manhattan(p.0, b.0) >= 18) {
                picked.push(b);
            }
        }
        self.beaches.insert(enemy, picked.clone());
        picked
    }

    /// Pick 1 or 2 landing beaches for this wave: short sail, few known defenses, not
    /// where earlier waves died, and not the beach used last time.
    pub(crate) fn pick_landings(&mut self, w: &World, enemy: u8, from: (i32, i32), prongs: usize) -> Vec<Beach> {
        let d = data();
        let beaches = self.beaches(w, enemy);
        let defensive = ["guard_tower", "fortress", "aa_site", "capitol"];
        let mut scored: Vec<(i64, Beach)> = beaches
            .into_iter()
            .map(|b| {
                let sail = manhattan(from, b.0) as i64 * 10;
                let danger: i64 = self
                    .known
                    .values()
                    .filter(|k| defensive.contains(&d.def(k.def).data.key.as_str()) && manhattan(k.tile, b.0) <= 16)
                    .count() as i64
                    * 400;
                let losses = *self.beach_losses.get(&b.0).unwrap_or(&0) as i64 * 700;
                let repeat = if self.last_beaches.contains(&b.0) { 500 } else { 0 };
                (sail + danger + losses + repeat + self.rng.range(0, 120) as i64, b)
            })
            .collect();
        scored.sort_by_key(|s| s.0);
        let mut out: Vec<Beach> = Vec::new();
        for (_, b) in scored {
            if out.len() >= prongs {
                break;
            }
            // a second prong lands well away from the first
            if out.iter().all(|o| manhattan(o.0, b.0) >= 30) {
                out.push(b);
            }
        }
        self.last_beaches = out.iter().map(|b| b.0).collect();
        out
    }

    /// Group idle land units by island; the launch island is the one with the most.
    pub(crate) fn launch_island(&self, w: &World, units: &[EntityId]) -> Option<(usize, Vec<EntityId>)> {
        let mut by: std::collections::BTreeMap<usize, Vec<EntityId>> = Default::default();
        for &u in units {
            if let Some(e) = w.get(u) {
                let isl = self.island_at(w, e.pos.tile()).unwrap_or(self.home_island);
                by.entry(isl).or_default().push(u);
            }
        }
        by.into_iter().max_by_key(|(isl, v)| (v.len(), *isl == self.home_island)).map(|(i, v)| (i, v))
    }

    /// Record how a finished wave went at its beaches.
    pub(crate) fn wave_result(&mut self, beaches: &[(i32, i32)], start: usize, survivors: usize) {
        self.beachhead_holds = start > 0 && survivors * 2 >= start;
        if start > 0 && survivors * 10 < start * 3 {
            for b in beaches {
                *self.beach_losses.entry(*b).or_insert(0) += 1;
            }
        }
    }

    // ------------------------------------------------------------------ forward bases

    /// Barracks (and a tank factory on bigger islands) on the colonies closest to the
    /// enemy, so waves can sail from there.
    pub(crate) fn forward_bases(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) -> bool {
        let d = data();
        let Some(es) = self.enemy_start(w) else { return false };
        let settlement = d.id("settlement");
        let mut colonies: Vec<(i64, usize, FVec)> = Vec::new();
        for &s in v.buildings.get(&settlement).map(|x| x.as_slice()).unwrap_or(&[]) {
            let Some(e) = w.get(s) else { continue };
            let isl = self.island_at(w, (e.tile.0 - 1, e.tile.1)).or_else(|| self.island_at(w, (e.tile.0 + 3, e.tile.1)));
            let Some(isl) = isl else { continue };
            if isl == self.home_island || self.islands.get(isl).map_or(true, |i| i.tiles < 220) {
                continue;
            }
            if colonies.iter().any(|c| c.1 == isl) {
                continue;
            }
            colonies.push((e.pos.dist2_raw(es), isl, e.pos));
        }
        colonies.sort_by_key(|c| c.0);
        for (_, isl, at) in colonies.into_iter().take(2) {
            for (key, min_tiles) in [("barracks", 220), ("tank_factory", 350)] {
                let def = d.id(key);
                if self.islands[isl].tiles < min_tiles {
                    continue;
                }
                let here = w.entities.iter().any(|e| e.alive && e.owner == self.player && e.def == def && self.island_at(w, (e.tile.0 - 1, e.tile.1)) == Some(isl));
                let pending = self.pending.iter().any(|(k, _)| *k == def);
                if here || pending || !w.can_afford(self.player, &d.def(def).data.cost) {
                    continue;
                }
                if let Some(t) = self.defense_plot(w, at, key) {
                    let who = self.nearest_citizens(w, v, at, 2, Some(isl));
                    if !who.is_empty() {
                        out.push(CommandKind::Build { units: who, def, tile: t, queue: false });
                        self.pending.push((def, w.tick));
                        return true;
                    }
                }
            }
        }
        false
    }

    // ------------------------------------------------------------------ sea control

    /// Deep-water loop around an island, `margin` tiles off its coast.
    fn ring_route(&self, w: &World, center: (i32, i32), margin: i32) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        for k in 0..10 {
            let (c, s) = ee_sim::mapgen::sincos_deg(k * 36);
            let mut found = None;
            for r in 8..90 {
                let (x, y) = (center.0 + c * r / 1024, center.1 + s * r / 1024);
                if !w.map.in_bounds(x, y) {
                    break;
                }
                if w.map.base_pass[w.map.idx(x, y)] & PASS_DEEP != 0 {
                    let (x2, y2) = (center.0 + c * (r + margin) / 1024, center.1 + s * (r + margin) / 1024);
                    if w.map.in_bounds(x2, y2) && w.map.base_pass[w.map.idx(x2, y2)] & PASS_DEEP != 0 {
                        found = Some((x2, y2));
                    } else {
                        found = Some((x, y));
                    }
                    break;
                }
            }
            if let Some(p) = found {
                out.push(p);
            }
        }
        out
    }

    fn sea_routes(&mut self, w: &World) -> Vec<Vec<(i32, i32)>> {
        if !self.sea_routes.is_empty() {
            return self.sea_routes.clone();
        }
        let mut routes = Vec::new();
        // 1) blockade every enemy home island
        for (i, &s) in w.starts.iter().enumerate() {
            if w.is_enemy(self.player, i as u8) {
                let r = self.ring_route(w, s, 6);
                if r.len() >= 4 {
                    routes.push(r);
                }
            }
        }
        // 2) sea lanes through the archipelago: deep water by each mid-size island
        let mut lane: Vec<(i32, i32)> = Vec::new();
        let mid = (w.map.w / 2, w.map.h / 2);
        let mut isl: Vec<&crate::expand::Island> = self.islands.iter().filter(|i| i.tiles >= 60 && i.tiles < 2500).collect();
        isl.sort_by_key(|i| {
            let (dx, dy) = (i.center.0 - mid.0, i.center.1 - mid.1);
            // order by angle around the map centre (integer pseudo-angle)
            let q = if dx >= 0 && dy >= 0 { 0 } else if dx < 0 && dy >= 0 { 1 } else if dx < 0 { 2 } else { 3 };
            let t = if q % 2 == 0 { dy.abs() * 1000 / (dx.abs() + dy.abs()).max(1) } else { dx.abs() * 1000 / (dx.abs() + dy.abs()).max(1) };
            q * 1000 + t
        });
        for i in isl {
            if let Some(p) = self.coast_water_near_tile(w, i.center) {
                lane.push(p);
            }
        }
        if lane.len() >= 3 {
            routes.push(lane);
        }
        // 3) our own coast
        let home = self.ring_route(w, self.base_tile, 5);
        if home.len() >= 4 {
            routes.push(home);
        }
        self.sea_routes = routes.clone();
        routes
    }

    fn coast_water_near_tile(&self, w: &World, c: (i32, i32)) -> Option<(i32, i32)> {
        for r in 4i32..40 {
            for k in 0..8 {
                let (cs, sn) = ee_sim::mapgen::sincos_deg(k * 45);
                let (x, y) = (c.0 + cs * r / 1024, c.1 + sn * r / 1024);
                if w.map.in_bounds(x, y) && w.map.base_pass[w.map.idx(x, y)] & PASS_DEEP != 0 {
                    return Some((x, y));
                }
            }
        }
        None
    }

    /// Idle warships form squadrons and sail patrol loops (blockade, sea lanes, home
    /// waters), engaging whatever they meet.
    pub(crate) fn naval_ops(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let idle: Vec<EntityId> = v.navy.iter().copied().filter(|&s| w.get(s).map_or(false, |e| e.order == Order::Idle)).collect();
        if idle.len() < 3 {
            return;
        }
        let routes = self.sea_routes(w);
        if routes.is_empty() {
            return;
        }
        // squadrons of up to 6
        for squad in idle.chunks(6) {
            if squad.len() < 2 {
                break;
            }
            let r = &routes[self.next_route % routes.len()];
            self.next_route += 1;
            // start at the waypoint nearest the squadron and loop the whole route
            let lead = w.get(squad[0]).map(|e| e.pos.tile()).unwrap_or(self.base_tile);
            let first = (0..r.len()).min_by_key(|&i| manhattan(r[i], lead)).unwrap_or(0);
            let dir = if self.next_route % 2 == 0 { 1 } else { r.len() - 1 };
            for k in 0..=r.len() {
                let p = r[(first + k * dir) % r.len()];
                out.push(CommandKind::Move { units: squad.to_vec(), to: FVec::tile_center(p.0, p.1), attack_move: true, queue: k > 0 });
            }
        }
    }
}
