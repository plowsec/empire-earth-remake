//! Overseas campaign: where to land (many beaches, several at once), which island to
//! launch from (colonies included), forward bases on colonies, and sea control with
//! roaming warship squadrons.
use crate::{Ai, View};
use ee_sim::command::CommandKind;
use ee_sim::entity::{EntityId, Order};
use ee_sim::fixed::{FVec, Fx};
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
        if !self.sea_routes.is_empty() && w.tick.wrapping_sub(self.sea_routes_tick) < 20 * 300 {
            return self.sea_routes.clone();
        }
        self.sea_routes_tick = w.tick;
        let mut routes = Vec::new();
        // guard every colony we hold, and bombard every island the enemy has settled
        let enemy_isles: Vec<usize> = {
            let mut v: Vec<usize> = self.known.values().filter_map(|k| self.island_at(w, k.tile).or_else(|| self.island_at(w, (k.tile.0 - 1, k.tile.1)))).collect();
            v.sort();
            v.dedup();
            v
        };
        for (i, isl) in self.islands.iter().enumerate() {
            let ours = isl.claimed && i != self.home_island;
            let theirs = enemy_isles.contains(&i) && w.starts.iter().all(|s| self.island_at(w, *s) != Some(i));
            if (ours || theirs) && routes.len() < 6 {
                let r = self.ring_route(w, isl.center, 3);
                if r.len() >= 3 {
                    routes.push(r);
                }
            }
        }
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

impl Ai {
    /// Units sealed into pockets on the home island (walled in by our own buildings or
    /// grown groves): knock out one cheap wall building, or have trapped citizens chop
    /// through the trees.
    pub(crate) fn free_trapped(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        use ee_sim::map::PASS_LAND;
        let m = &w.map;
        let n = (m.w * m.h) as usize;
        let open = |i: usize| m.pass[i] & PASS_LAND != 0;
        // the main ground: flood from the capitol's surroundings
        let (bx, by) = self.base_tile;
        let mut start = None;
        'f: for r in 1..8 {
            for dy in -r..=r {
                for dx in -r..=r {
                    let (x, y) = (bx + dx, by + dy);
                    if m.in_bounds(x, y) && open(m.idx(x, y)) {
                        start = Some(m.idx(x, y));
                        break 'f;
                    }
                }
            }
        }
        let Some(start) = start else { return };
        let mut main = vec![false; n];
        let mut stack = vec![start];
        main[start] = true;
        while let Some(i) = stack.pop() {
            let (x, y) = ((i as i32) % m.w, (i as i32) / m.w);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if m.in_bounds(nx, ny) {
                    let j = m.idx(nx, ny);
                    if !main[j] && open(j) {
                        main[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        // a trapped unit of ours on the home island
        let trapped = v.land_army.iter().chain(v.citizens.iter()).copied().find(|&u| {
            w.get(u).map_or(false, |e| {
                let (tx, ty) = e.pos.tile();
                e.inside == 0 && m.in_bounds(tx, ty) && open(m.idx(tx, ty)) && !main[m.idx(tx, ty)] && self.island_at(w, (tx, ty)) == Some(self.home_island)
            })
        });
        let Some(u) = trapped else { return };
        let (tx, ty) = w.get(u).unwrap().pos.tile();
        // its pocket
        let mut pocket = vec![false; n];
        let s0 = m.idx(tx, ty);
        let mut stack = vec![s0];
        pocket[s0] = true;
        let mut tiles = vec![s0];
        while let Some(i) = stack.pop() {
            if tiles.len() > 600 {
                return; // not a pocket: a separate region of the island
            }
            let (x, y) = ((i as i32) % m.w, (i as i32) / m.w);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if m.in_bounds(nx, ny) {
                    let j = m.idx(nx, ny);
                    if !pocket[j] && open(j) {
                        pocket[j] = true;
                        stack.push(j);
                        tiles.push(j);
                    }
                }
            }
        }
        // walls: blocking neighbours of the pocket that also touch the main ground
        let d = data();
        let cheap = ["house", "apartments", "guard_tower", "aa_site", "hospital", "granary", "abm_site"];
        let mut wall_building: Option<(i32, EntityId)> = None;
        let mut wall_tree: Option<EntityId> = None;
        for &i in &tiles {
            let (x, y) = ((i as i32) % m.w, (i as i32) / m.w);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if !m.in_bounds(nx, ny) {
                    continue;
                }
                let occ = m.occupant[m.idx(nx, ny)];
                let Some(o) = w.get(occ) else { continue };
                let od = d.def(o.def);
                // does this obstacle also border the main ground?
                let (ox, oy) = o.tile;
                let (sw, sh) = od.size();
                let touches_main = (oy - 1..=oy + sh).any(|yy| (ox - 1..=ox + sw).any(|xx| m.in_bounds(xx, yy) && main[m.idx(xx, yy)]));
                if !touches_main {
                    continue;
                }
                if o.owner == self.player && cheap.contains(&od.data.key.as_str()) {
                    let cost = od.data.cost.arr().iter().sum::<i32>();
                    if wall_building.map_or(true, |b| cost < b.0) {
                        wall_building = Some((cost, occ));
                    }
                } else if od.data.resource == Some(ee_sim::defs::Res::Wood) {
                    wall_tree = Some(occ);
                }
            }
        }
        if let Some(t) = wall_tree {
            // citizens in the pocket cut their way out
            let cutters: Vec<EntityId> = v.citizens.iter().copied().filter(|&c| w.get(c).map_or(false, |e| {
                let (cx, cy) = e.pos.tile();
                e.inside == 0 && m.in_bounds(cx, cy) && pocket[m.idx(cx, cy)]
            })).take(4).collect();
            if !cutters.is_empty() {
                out.push(CommandKind::Target { units: cutters, target: t, queue: false });
                return;
            }
        }
        if let Some((_, b)) = wall_building {
            out.push(CommandKind::Delete { units: vec![b] });
        }
    }
}

/// A small amphibious raid: one landing craft, ~10 troops, a random enemy beach.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Raid {
    pub transport: EntityId,
    pub units: Vec<EntityId>,
    pub beach: (i32, i32),
    pub stage: u8,
    pub tick: u32,
}

impl Ai {
    /// Guard posts spread along an island's coast (cached).
    fn coast_posts(&mut self, w: &World, isl: usize) -> Vec<(i32, i32)> {
        if let Some(p) = self.coast_posts.get(&isl) {
            return p.clone();
        }
        let spacing = if isl == self.home_island { 22 } else { 14 };
        let mut posts: Vec<(i32, i32)> = Vec::new();
        for (i, &c) in self.island_of.iter().enumerate() {
            if c as usize != isl {
                continue;
            }
            let (x, y) = (i as i32 % w.map.w, i as i32 / w.map.w);
            if (x + y) % 3 != 0 {
                continue;
            }
            let coastal = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| w.map.is_water(x + dx * 2, y + dy * 2));
            if coastal && posts.iter().all(|p| manhattan(*p, (x, y)) >= spacing) {
                posts.push((x, y));
            }
        }
        self.coast_posts.insert(isl, posts.clone());
        posts
    }

    /// Every island we hold gets a guard tower (and a SAM site when enemy aircraft are
    /// about) at each coastal post. One building per call.
    pub(crate) fn fortify_coasts(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) -> bool {
        if v.citizens.len() < 50 || w.tick < self.diff.first_attack() {
            return false;
        }
        let d = data();
        let tower = d.id("guard_tower");
        let sam = d.id("aa_site");
        // static defense only from surplus, and in proportion to the economy
        let pl = &w.players[self.player as usize];
        let statics = v.count(tower) + v.count(sam);
        if pl.res[2] < 900 || pl.res[0] < 400 || statics >= 8 + v.citizens.len() / 25 {
            return false;
        }
        let air = self.seen(0) + self.seen(1) >= 3;
        let held: Vec<usize> = (0..self.islands.len()).filter(|&i| self.islands[i].claimed).collect();
        let mine: Vec<(ee_sim::defs::DefId, FVec)> = w.entities.iter().filter(|e| e.alive && e.owner == self.player && (e.def == tower || e.def == sam)).map(|e| (e.def, e.pos)).collect();
        for isl in held {
            let posts = self.coast_posts(w, isl);
            for (k, post) in posts.into_iter().enumerate() {
                let pp = FVec::tile_center(post.0, post.1);
                for (def, key, want) in [(tower, "guard_tower", true), (sam, "aa_site", air && k % 2 == 0)] {
                    if !want || mine.iter().any(|(dd, p)| *dd == def && p.within(pp, Fx::from_int(10))) {
                        continue;
                    }
                    if self.pending.iter().any(|(pd, _)| *pd == def) || !w.can_afford(self.player, &d.def(def).data.cost) {
                        return false;
                    }
                    let Some(t) = self.defense_plot(w, pp, key) else { continue };
                    let who = self.nearest_citizens(w, v, pp, 2, Some(isl));
                    if who.is_empty() {
                        continue;
                    }
                    out.push(CommandKind::Build { units: who, def, tile: t, queue: false });
                    self.pending.push((def, w.tick));
                    return true;
                }
            }
        }
        false
    }

    /// Hit-and-run landings on random enemy beaches between the big waves.
    pub(crate) fn raid_tick(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let tick = w.tick;
        if let Some(mut r) = self.raid.take() {
            r.units.retain(|&u| w.get(u).is_some());
            let Some(ship) = w.get(r.transport) else { self.last_raid = tick; return };
            let elapsed = tick.wrapping_sub(r.tick);
            match r.stage {
                0 => {
                    let aboard = r.units.iter().filter(|&&u| w.get(u).map_or(false, |e| e.inside == r.transport)).count();
                    if aboard > 0 && (aboard == r.units.len() || elapsed > 20 * 60) {
                        r.units.retain(|&u| w.get(u).map_or(false, |e| e.inside == r.transport));
                        out.push(CommandKind::Unload { units: vec![r.transport], at: FVec::tile_center(r.beach.0, r.beach.1) });
                        r.stage = 1;
                        r.tick = tick;
                    } else if elapsed > 20 * 90 {
                        self.last_raid = tick;
                        return; // nobody got aboard
                    }
                }
                _ => {
                    if ship.cargo.is_empty() || elapsed > 20 * 150 {
                        let ashore: Vec<EntityId> = r.units.iter().copied().filter(|&u| w.get(u).map_or(false, |e| e.inside == 0)).collect();
                        let bp = FVec::tile_center(r.beach.0, r.beach.1);
                        let to = self.known.values().min_by_key(|k| k.pos.dist2_raw(bp)).map(|k| k.pos).unwrap_or(bp);
                        if !ashore.is_empty() {
                            out.push(CommandKind::Move { units: ashore, to, attack_move: true, queue: false });
                        }
                        out.push(CommandKind::Move { units: vec![r.transport], to: self.base, attack_move: false, queue: false });
                        self.last_raid = tick;
                        return;
                    }
                }
            }
            self.raid = Some(r);
            return;
        }
        if tick < self.diff.first_attack() || tick.wrapping_sub(self.last_raid) < 20 * 100 || v.land_army.len() < 60 {
            return;
        }
        let Some(enemy) = self.enemy_target_player(w) else { return };
        let beaches = self.beaches(w, enemy);
        if beaches.is_empty() {
            return;
        }
        // a free landing craft (not the invasion's, not the colonists')
        let busy: Vec<EntityId> = self.invasion.as_ref().map(|i| i.transports.clone()).unwrap_or_default();
        let colony_ship = self.colony.as_ref().map(|c| c.transport);
        let Some(&ship) = v.transports.iter().find(|&&t| !busy.contains(&t) && Some(t) != colony_ship && w.get(t).map_or(false, |e| e.cargo.is_empty() && matches!(e.order, Order::Idle))) else { return };
        let sp = w.get(ship).unwrap().pos;
        let invading: Vec<EntityId> = self.invasion.as_ref().map(|i| i.units.clone()).unwrap_or_default();
        let mut pool: Vec<(i64, EntityId)> = v.land_army.iter().copied()
            .filter(|u| !invading.contains(u))
            .filter_map(|u| w.get(u).filter(|e| e.inside == 0 && e.order == Order::Idle && self.island_at(w, e.pos.tile()) == Some(self.home_island)).map(|e| (e.pos.dist2_raw(sp), u)))
            .collect();
        pool.sort();
        let units: Vec<EntityId> = pool.into_iter().take(10).map(|x| x.1).collect();
        if units.len() < 6 {
            return;
        }
        let beach = beaches[self.rng.below(beaches.len() as u32) as usize].0;
        out.push(CommandKind::Target { units: units.clone(), target: ship, queue: false });
        self.raid = Some(Raid { transport: ship, units, beach, stage: 0, tick });
    }
}
