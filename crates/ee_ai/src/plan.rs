//! AI construction, production and military operations.
use crate::{Ai, Difficulty, Invasion, Stage, View};
use ee_sim::command::CommandKind;
use ee_sim::defs::{Class, DefId, Layer};
use ee_sim::entity::{EntityId, Order};
use ee_sim::fixed::{FVec, Fx};
use ee_sim::map::PASS_WATER;
use ee_sim::world::{data, World};
use std::collections::VecDeque;

impl Ai {
    // ------------------------------------------------------------------ building

    pub(crate) fn build(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        let pl = &w.players[self.player as usize];
        let id = |k: &str| d.id(k);
        let cits = v.citizens.len();
        // drop stale pending entries (site placed or failed)
        let tick = w.tick;
        self.pending.retain(|(_, t)| tick.wrapping_sub(*t) < 200);
        let pending_count = |p: &Vec<(DefId, u32)>, def: DefId| p.iter().filter(|(x, _)| *x == def).count();
        let sites_of = |def: DefId| v.sites.iter().filter(|&&s| w.get(s).map_or(false, |e| e.def == def)).count();
        let have = |def: DefId| v.count(def) + sites_of(def) + pending_count(&self.pending, def);

        // build list in priority order
        let mut wants: Vec<DefId> = Vec::new();
        let prod_buildings = v.count(id("barracks")) + v.count(id("tank_factory")) + v.count(id("airport")) + v.count(id("naval_yard"));
        let headroom = pl.pop_cap - pl.pop;
        let houses_building = sites_of(id("house")) + pending_count(&self.pending, id("house"));
        if pl.pop_cap < w.config.pop_limit && headroom < 6 + prod_buildings as i32 * 3 && houses_building < 1 + cits / 30 {
            wants.push(id("house"));
        }
        let hard = self.diff >= Difficulty::Hard;
        if cits >= 10 && have(id("barracks")) == 0 {
            wants.push(id("barracks"));
        }
        if cits >= 14 && have(id("granary")) == 0 {
            wants.push(id("granary"));
        }
        if cits >= 15 && have(id("naval_yard")) == 0 {
            wants.push(id("naval_yard"));
        }
        if cits >= 20 && have(id("tank_factory")) == 0 {
            wants.push(id("tank_factory"));
        }
        if cits >= 26 && have(id("airport")) == 0 && self.diff != Difficulty::Easy {
            wants.push(id("airport"));
        }
        if cits >= 30 && have(id("guard_tower")) < 2 {
            wants.push(id("guard_tower"));
        }
        if cits >= 30 && (self.seen_air > 8 || hard) && have(id("aa_site")) < 2 {
            wants.push(id("aa_site"));
        }
        if cits >= 34 && have(id("hospital")) == 0 {
            wants.push(id("hospital"));
        }
        if cits >= 36 && have(id("barracks")) < 2 {
            wants.push(id("barracks"));
        }
        if hard && cits >= 42 && have(id("tank_factory")) < 2 {
            wants.push(id("tank_factory"));
        }
        if hard && cits >= 46 && have(id("airport")) < 2 {
            wants.push(id("airport"));
        }
        if cits >= 40 && have(id("naval_yard")) < 2 {
            wants.push(id("naval_yard"));
        }
        if cits >= 40 && have(id("granary")) < 2 && have(id("farm")) >= 8 {
            wants.push(id("granary"));
        }
        if cits >= 24 && have(id("settlement")) < 1 + cits / 30 {
            wants.push(id("settlement"));
        }
        // swimming in resources: more production and defenses
        let rich = pl.res[1] > 2500 && pl.res[2] > 1500;
        if rich {
            for (k, cap) in [("barracks", 3), ("tank_factory", 3), ("airport", 2), ("naval_yard", 3), ("guard_tower", 6), ("aa_site", 4)] {
                if have(id(k)) < cap {
                    wants.push(id(k));
                }
            }
        }
        // research
        self.research(w, v, out);
        self.replant(w, v, out);

        let mut started = 0;
        // money set aside for the first big item we can't afford yet
        let mut reserve = [0i32; 5];
        for def in wants {
            if started >= 2 {
                break;
            }
            let cost = d.def(def).data.cost;
            let critical = def == id("farm") || def == id("house");
            let budget_ok = cost.arr().iter().enumerate().all(|(r, c)| pl.res[r] - if critical { 0 } else { reserve[r] } >= *c);
            if !budget_ok {
                if !critical && reserve.iter().all(|&x| x == 0) {
                    reserve = cost.arr();
                }
                continue;
            }
            let Some(tile) = self.find_site(w, def) else { continue };
            // builders: nearest citizens, prefer idle
            let n_builders = match d.def(def).data.size.0 {
                1 | 2 => 1,
                3 => 2,
                _ => 3,
            };
            let center = FVec::tile_center(tile.0, tile.1);
            let mut pool: Vec<(i64, EntityId)> = v
                .citizens
                .iter()
                .filter(|c| !v.builders.contains(c))
                .filter(|&&c| !matches!(w.get(c).map(|e| e.order), Some(Order::Scout { .. })))
                .filter_map(|&c| w.get(c).map(|e| (e.pos.dist2_raw(center) + if e.carry > 0 { 1 << 40 } else { 0 }, c)))
                .collect();
            pool.sort();
            let builders: Vec<EntityId> = pool.iter().take(n_builders).map(|x| x.1).collect();
            if builders.is_empty() {
                break;
            }
            out.push(CommandKind::Build { units: builders, def, tile, queue: false });
            self.pending.push((def, w.tick));
            started += 1;
        }

        // idle citizens help finish construction sites
        for &s in &v.sites {
            let Some(site) = w.get(s) else { continue };
            let working = v.builders.iter().filter(|&&b| matches!(w.get(b).map(|e| e.order), Some(Order::Build { site: x }) if x == s)).count();
            if working == 0 {
                let mut pool: Vec<(i64, EntityId)> = v
                    .citizens
                    .iter()
                    .filter(|c| !v.builders.contains(c))
                    .filter(|&&c| !matches!(w.get(c).map(|e| e.order), Some(Order::Scout { .. })))
                    .filter_map(|&c| w.get(c).map(|e| (e.pos.dist2_raw(site.pos), c)))
                    .collect();
                pool.sort();
                if let Some(&(_, c)) = pool.first() {
                    out.push(CommandKind::Target { units: vec![c], target: s, queue: false });
                }
            }
        }

        // repair damaged buildings when not under heavy attack
        if !self.defending && w.tick % 100 == 0 {
            for list in v.buildings.values() {
                for &b in list {
                    let Some(be) = w.get(b) else { continue };
                    if be.hp * 3 < w.max_hp(be) * 2 && w.tick.wrapping_sub(be.last_hit_tick) > 200 {
                        if let Some(&c) = v.idle_citizens.first().or(v.citizens.first()) {
                            out.push(CommandKind::Target { units: vec![c], target: b, queue: false });
                            return;
                        }
                    }
                }
            }
        }
    }

    /// Forests near the base thinning out: plant a grove next to a drop-off.
    fn replant(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        if w.tick.wrapping_sub(self.last_replant) < 20 * 25 || v.citizens.len() < 12 {
            return;
        }
        let near_wood = w.nearest_resource(ee_sim::defs::Res::Wood as u8, self.base, 14, false).is_some();
        let pl = &w.players[self.player as usize];
        // replant when the home forest is gone, or whenever wood runs low
        let low = pl.res[1] < 400;
        if (near_wood && !low) || pl.res[0] < 120 {
            return;
        }
        self.last_replant = w.tick;
        let sap = d.id("sapling");
        let (bx, by) = self.base_tile;
        let start = self.rng.below(8) as i32;
        let mut groves = 0;
        for r in 7..16 {
            for k in 0..8 {
                let (c, s) = ee_sim::mapgen::sincos_deg((start + k) * 45);
                let (cx, cy) = (bx + c * r / 1024, by + s * r / 1024);
                let tiles: Vec<(i32, i32)> = (-1..=1).flat_map(|dy| (-1..=1).map(move |dx| (cx + dx, cy + dy))).filter(|&t| w.can_place(self.player, sap, t).is_ok()).collect();
                if tiles.len() >= 6 {
                    let workers: Vec<EntityId> = v.gatherers[ee_sim::defs::Res::Wood as usize].iter().take(2).copied().collect();
                    let workers = if workers.is_empty() { v.citizens.iter().take(2).copied().collect() } else { workers };
                    for (n, t) in tiles.into_iter().enumerate() {
                        out.push(CommandKind::Build { units: workers.clone(), def: sap, tile: t, queue: n > 0 });
                    }
                    groves += 1;
                    if groves >= 2 {
                        return;
                    }
                }
            }
        }
    }

    fn research(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        let pl = &w.players[self.player as usize];
        if v.citizens.len() < 25 {
            return;
        }
        for t in &d.techs {
            if pl.techs[t.id as usize] || pl.researching[t.id as usize] {
                continue;
            }
            let Some(bs) = v.buildings.get(&t.at) else { continue };
            // keep a reserve for the army: only research when comfortably rich
            let rich = t.data.cost.arr().iter().zip(pl.res.iter()).all(|(c, r)| *r >= c * 2 + 200);
            if !rich {
                continue;
            }
            if let Some(&b) = bs.iter().find(|&&b| w.get(b).map_or(false, |e| e.production.len() < 2)) {
                out.push(CommandKind::Research { building: b, tech: t.id });
                return;
            }
        }
    }

    /// Find a placement tile for `def` near the base.
    pub(crate) fn find_site(&mut self, w: &World, def: DefId) -> Option<(i32, i32)> {
        let d = data();
        let dd = d.def(def);
        let (sw, sh) = dd.size();
        let (bx, by) = self.base_tile;
        let p = self.player;
        if def == d.id("farm") {
            let granary = d.id("granary");
            for e in &w.entities {
                if !(e.alive && e.owner == p && e.def == granary && e.complete) {
                    continue;
                }
                let (gx, gy) = e.tile;
                let cands = [
                    (gx - 3, gy), (gx + 3, gy), (gx, gy - 3), (gx, gy + 3),
                    (gx - 3, gy - 3), (gx + 3, gy - 3), (gx - 3, gy + 3), (gx + 3, gy + 3),
                    (gx - 3, gy - 1), (gx + 3, gy + 1), (gx - 1, gy - 3), (gx + 1, gy + 3),
                ];
                for c in cands {
                    if w.can_place(p, def, c).is_ok() {
                        return Some(c);
                    }
                }
            }
            return None;
        }
        if dd.data.coastal {
            let mut best: Option<(i32, (i32, i32))> = None;
            for y in by - 34..by + 34 {
                for x in bx - 34..bx + 34 {
                    if !w.map.in_bounds(x, y) {
                        continue;
                    }
                    // quick filter: must have water within the footprint ring
                    if !w.map.is_water(x + sw / 2, y + sh / 2) && !w.map.is_water(x + sw, y + sh / 2) && !w.map.is_water(x - 1, y + sh / 2)
                        && !w.map.is_water(x + sw / 2, y - 1) && !w.map.is_water(x + sw / 2, y + sh)
                    {
                        continue;
                    }
                    let dist = (x - bx).pow(2) + (y - by).pow(2);
                    if best.map_or(false, |(bd, _)| dist >= bd) {
                        continue;
                    }
                    if w.can_place(p, def, (x, y)).is_ok() && self.deep_water_near(w, x, y, sw, sh) && self.reachable_by_land(w, x, y, sw, sh) {
                        best = Some((dist, (x, y)));
                    }
                }
            }
            return best.map(|b| b.1);
        }
        if dd.data.key == "granary" {
            return self.granary_site(w, def);
        }
        let start_ang = self.rng.below(8) as i32;
        let (rmin, rmax) = match dd.data.key.as_str() {
            "guard_tower" | "aa_site" => (9, 16),
            "settlement" => (16, 30),
            "house" => (5, 18),
            _ => (6, 24),
        };
        if dd.data.key == "settlement" {
            // next to a mine cluster away from the capitol
            return self.settlement_site(w, def);
        }
        for r in rmin..rmax {
            let n = (r * 6).max(8);
            for k in 0..n {
                let ang = (start_ang * 45 + k * 360 / n) % 360;
                let (c, s) = ee_sim::mapgen::sincos_deg(ang);
                let x = bx + c * r / 1024 - sw / 2;
                let y = by + s * r / 1024 - sh / 2;
                if w.can_place(p, def, (x, y)).is_ok() && self.margin_clear(w, x, y, sw, sh) && self.reachable_by_land(w, x, y, sw, sh) {
                    return Some((x, y));
                }
            }
        }
        None
    }

    /// Granary with room for a ring of eight 3x3 farms around it (9x9 clear).
    fn granary_site(&mut self, w: &World, def: DefId) -> Option<(i32, i32)> {
        let (bx, by) = self.base_tile;
        let start_ang = self.rng.below(8) as i32;
        let mut fallback = None;
        for r in 7..26 {
            let n = (r * 6).max(8);
            for k in 0..n {
                let ang = (start_ang * 45 + k * 360 / n) % 360;
                let (c, s) = ee_sim::mapgen::sincos_deg(ang);
                let x = bx + c * r / 1024 - 1;
                let y = by + s * r / 1024 - 1;
                if w.can_place(self.player, def, (x, y)).is_err() || !self.margin_clear(w, x, y, 3, 3) {
                    continue;
                }
                let mut free = 0;
                for yy in y - 3..y + 6 {
                    for xx in x - 3..x + 6 {
                        if w.map.in_bounds(xx, yy) && w.map.pass[w.map.idx(xx, yy)] & ee_sim::map::PASS_LAND != 0 && w.explored(self.player, xx, yy) {
                            free += 1;
                        }
                    }
                }
                if free >= 76 {
                    return Some((x, y));
                }
                if fallback.is_none() && free >= 60 {
                    fallback = Some((x, y));
                }
            }
        }
        fallback
    }

    fn settlement_site(&self, w: &World, def: DefId) -> Option<(i32, i32)> {
        let d = data();
        let mines = [d.id("gold_mine"), d.id("iron_mine"), d.id("stone_mine")];
        let (bx, by) = self.base_tile;
        let mut best: Option<(i32, (i32, i32))> = None;
        for e in &w.entities {
            if !e.alive || !mines.contains(&e.def) {
                continue;
            }
            let (mx, my) = e.tile;
            let dist = (mx - bx).pow(2) + (my - by).pow(2);
            if dist < 16 * 16 || dist > 34 * 34 || !w.explored(self.player, mx, my) {
                continue;
            }
            // no own dropsite already close
            let near_drop = w.nearest_dropsite(self.player, d.def(e.def).data.resource.unwrap() as u8, e.pos, Layer::Land)
                .and_then(|id| w.get(id))
                .map_or(false, |ds| ds.pos.within(e.pos, Fx::from_int(10)));
            if near_drop {
                continue;
            }
            for (ox, oy) in [(3, 0), (-4, 0), (0, 3), (0, -4), (3, 3), (-4, -4)] {
                let t = (mx + ox, my + oy);
                if w.can_place(self.player, def, t).is_ok() && self.margin_clear(w, t.0, t.1, 3, 3) {
                    if best.map_or(true, |(bd, _)| dist < bd) {
                        best = Some((dist, t));
                    }
                    break;
                }
            }
        }
        best.map(|b| b.1)
    }

    fn margin_clear(&self, w: &World, x: i32, y: i32, sw: i32, sh: i32) -> bool {
        // leave the farm ring around granaries free
        let granary = data().id("granary");
        for e in &w.entities {
            if e.alive && e.owner == self.player && e.def == granary {
                let (gx, gy) = e.tile;
                let overlap = x < gx + 6 && x + sw > gx - 3 && y < gy + 6 && y + sh > gy - 3;
                if overlap && !(x == gx && y == gy) {
                    return false;
                }
            }
        }
        // keep a 1-tile lane free of other buildings around new buildings
        for yy in y - 1..=y + sh {
            for xx in x - 1..=x + sw {
                if !w.map.in_bounds(xx, yy) {
                    return false;
                }
                let occ = w.map.occupant[w.map.idx(xx, yy)];
                if occ != 0 {
                    if let Some(o) = w.get(occ) {
                        if data().def(o.def).is_building() && !data().def(o.def).data.walkable {
                            return false;
                        }
                    }
                }
            }
        }
        true
    }

    fn deep_water_near(&self, w: &World, x: i32, y: i32, sw: i32, sh: i32) -> bool {
        let mut n = 0;
        for yy in y - 2..=y + sh + 1 {
            for xx in x - 2..=x + sw + 1 {
                if w.map.in_bounds(xx, yy) && w.map.base_pass[w.map.idx(xx, yy)] & PASS_WATER != 0 {
                    n += 1;
                }
            }
        }
        n >= 6
    }

    fn reachable_by_land(&self, w: &World, x: i32, y: i32, sw: i32, sh: i32) -> bool {
        // cheap check: a land tile next to the footprint shares the base's island
        let d = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() + (a.1 - b.1).abs();
        let _ = d;
        for yy in y - 1..=y + sh {
            for xx in x - 1..=x + sw {
                if w.map.passable(xx, yy, Layer::Land) {
                    return true;
                }
            }
        }
        false
    }

    // ------------------------------------------------------------------ production

    pub(crate) fn produce(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        let pl = &w.players[self.player as usize];
        if pl.pop >= pl.pop_cap {
            return;
        }
        let econ_ready = v.citizens.len() * 100 >= self.diff.citizen_target() * 55;
        let id = |k: &str| d.id(k);
        // boats for fish
        if let Some(yards) = v.buildings.get(&id("naval_yard")) {
            let fish_left = !w.fish.is_empty();
            if v.boats.len() < 5 && fish_left {
                if let Some(&y) = yards.iter().find(|&&y| w.get(y).map_or(false, |e| e.production.is_empty())) {
                    if pl.res[1] >= 75 {
                        out.push(CommandKind::Train { building: y, def: id("fishing_boat"), count: 1 });
                    }
                }
            }
        }
        if !econ_ready && w.tick < self.diff.first_attack() / 2 {
            // only a token defense early
            if v.land_army.len() >= 4 {
                return;
            }
        }
        let air_threat = self.seen_air > 10;
        let armor_threat = self.seen_heavy > 10;
        let navy_threat = self.seen_navy > 8;

        // land mix: (key, weight)
        let mut land: Vec<(&str, i32)> = vec![
            ("rifleman", 22),
            ("machine_gunner", 10),
            ("bazooka", if armor_threat { 22 } else { 12 }),
            ("stinger", if air_threat { 16 } else { 4 }),
            ("mortar", 5),
            ("medic", 4),
            ("sniper", 3),
            ("tank", 26),
            ("at_gun", if armor_threat { 8 } else { 3 }),
            ("aa_vehicle", if air_threat { 12 } else { 3 }),
            ("howitzer", 6),
            ("recon", 2),
        ];
        if self.diff == Difficulty::Easy {
            land.retain(|(k, _)| matches!(*k, "rifleman" | "machine_gunner" | "bazooka" | "tank" | "mortar"));
        }
        let air: Vec<(&str, i32)> = vec![
            ("fighter", if air_threat { 45 } else { 25 }),
            ("bomber", 25),
            ("strike_fighter", if armor_threat { 25 } else { 12 }),
            ("helicopter", 18),
        ];
        let navy: Vec<(&str, i32)> = vec![
            ("frigate", if air_threat { 35 } else { 20 }),
            ("battleship", 30),
            ("submarine", if navy_threat { 35 } else { 15 }),
        ];

        let me = self.player;
        let count_of = |key: &str| -> i32 {
            let k = d.id(key);
            let mut n = 0;
            for e in &w.entities {
                if e.alive && e.owner == me && e.def == k {
                    n += 1;
                }
                if e.alive && e.owner == me {
                    n += e.production.iter().filter(|it| matches!(it, ee_sim::entity::ProdItem::Unit(u) if *u == k)).count() as i32;
                }
            }
            n
        };

        let pick = |mix: &[(&str, i32)], trainable: &[DefId], ai: &mut Ai| -> Option<DefId> {
            let total: i32 = mix.iter().map(|(_, wt)| wt).sum();
            let counts: Vec<i32> = mix.iter().map(|(k, _)| count_of(k)).collect();
            let sum: i32 = counts.iter().sum::<i32>().max(1);
            // largest share deficit among affordable options
            let mut best: Option<(i32, DefId)> = None;
            for (i, (k, wt)) in mix.iter().enumerate() {
                let def = d.id(k);
                if !trainable.contains(&def) {
                    continue;
                }
                if !w.can_afford(ai.player, &d.def(def).data.cost) {
                    continue;
                }
                let deficit = wt * 1000 / total - counts[i] * 1000 / sum + ai.rng.range(0, 40);
                if best.map_or(true, |(b, _)| deficit > b) {
                    best = Some((deficit, def));
                }
            }
            best.map(|b| b.1)
        };

        let cit_target = self.diff.citizen_target();
        let reserve_ok = |def: DefId| -> bool {
            // don't starve citizen production of food
            let c = d.def(def).data.cost;
            c.food == 0 || pl.res[0] - c.food >= 100 || v.citizens.len() >= cit_target
        };

        let mut reserve = [0i32; 5];
        let mut nuclear_queued = false;
        for (bkey, mix) in [("airport", &air), ("naval_yard", &navy), ("tank_factory", &land), ("barracks", &land)] {
            let Some(bs) = v.buildings.get(&id(bkey)) else { continue };
            for &b in bs {
                let Some(be) = w.get(b) else { continue };
                if be.production.len() >= 2 {
                    continue;
                }
                let trainable = &d.def(be.def).trains;
                // naval yard: transports first when an invasion needs them
                if bkey == "naval_yard" {
                    let want_tr = self.transports_wanted(v);
                    if (v.transports.len() as i32) < want_tr && w.can_afford(self.player, &d.def(id("transport")).data.cost) {
                        out.push(CommandKind::Train { building: b, def: id("transport"), count: 1 });
                        continue;
                    }
                    if v.navy.len() >= 6 + self.wave as usize * 2 {
                        continue;
                    }
                }
                // Maintain one strategic bomber after the economy matures. It gets
                // its own slot so an existing conventional air force cannot block it.
                let nuclear = id("nuke_bomber");
                let nuclear_cost = d.def(nuclear).data.cost.arr();
                if bkey == "airport" && self.diff >= Difficulty::Hard && econ_ready
                    && w.tick >= self.diff.first_attack() && !nuclear_queued
                    && count_of("nuke_bomber") == 0
                    && nuclear_cost.iter().enumerate().all(|(r, c)| pl.res[r] - reserve[r] >= c + 200) {
                    out.push(CommandKind::Train { building: b, def: nuclear, count: 1 });
                    for r in 0..5 { reserve[r] += nuclear_cost[r]; }
                    nuclear_queued = true;
                    continue;
                }
                if bkey == "airport" && v.air.len() >= 8 + self.wave as usize * 3 {
                    continue;
                }
                // ideal pick ignoring cost; if unaffordable, save for it
                let total: i32 = mix.iter().map(|(_, wt)| wt).sum();
                let counts: Vec<i32> = mix.iter().map(|(k, _)| count_of(k)).collect();
                let sum: i32 = counts.iter().sum::<i32>().max(1);
                let mut ideal: Option<(i32, DefId)> = None;
                for (i, (k, wt)) in mix.iter().enumerate() {
                    let def = d.id(k);
                    if !trainable.contains(&def) {
                        continue;
                    }
                    let deficit = wt * 1000 / total - counts[i] * 1000 / sum + self.rng.range(0, 60);
                    if ideal.map_or(true, |(bd, _)| deficit > bd) {
                        ideal = Some((deficit, def));
                    }
                }
                let Some((_, u)) = ideal else { continue };
                let cost = d.def(u).data.cost.arr();
                let affordable = cost.iter().enumerate().all(|(r, c)| pl.res[r] - reserve[r] >= *c);
                if affordable {
                    if reserve_ok(u) {
                        out.push(CommandKind::Train { building: b, def: u, count: 1 });
                        for r in 0..5 {
                            reserve[r] += cost[r];
                        }
                    }
                } else {
                    // save for at most one big-ticket item at a time
                    if reserve.iter().all(|&x| x == 0) && v.land_army.len() >= 10 {
                        for r in 0..5 {
                            reserve[r] += cost[r];
                        }
                    }
                    // otherwise train something affordable
                    if let Some(f) = pick(mix, trainable, self) {
                        let fc = d.def(f).data.cost.arr();
                        if fc.iter().enumerate().all(|(r, c)| pl.res[r] - reserve[r] >= *c) && reserve_ok(f) {
                            out.push(CommandKind::Train { building: b, def: f, count: 1 });
                        }
                    }
                }
            }
        }
    }

    fn transports_wanted(&self, v: &View) -> i32 {
        let army = v.land_army.len() as i32;
        let unclaimed = self.islands.iter().any(|i| !i.claimed && !i.mines.is_empty() && i.tiles >= 60) as i32;
        if army < 8 && unclaimed > 0 && v.citizens.len() >= 18 {
            return 1;
        }
        if army < 8 {
            return 0;
        }
        ((army.min(self.diff.wave_size() as i32 + 24) + 11) / 12).clamp(1, 5) + unclaimed
    }

    // ------------------------------------------------------------------ military

    pub(crate) fn military(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let p = self.player;
        let d = data();
        // ---- defense: enemies near our buildings
        let mut threat: Option<(FVec, i32)> = None;
        let mut threat_count = 0;
        for e in &w.entities {
            if !e.alive || !e.on_map() || !w.is_enemy(p, e.owner) || !w.can_see(p, e) {
                continue;
            }
            let ed = d.def(e.def);
            if !ed.is_unit() || ed.layer == Layer::Water {
                continue;
            }
            // near any of our buildings?
            let near = e.pos.within(self.base, Fx::from_int(30)) || v.buildings.values().flatten().take(40).any(|&b| {
                w.get(b).map_or(false, |be| be.pos.within(e.pos, Fx::from_int(14)))
            });
            if near {
                threat_count += 1;
                let dist = e.pos.dist(self.base).0;
                if threat.map_or(true, |(_, bd)| dist < bd) {
                    threat = Some((e.pos, dist));
                }
            }
        }
        self.defending = threat_count >= 3;
        let invading: Vec<EntityId> = self.invasion.as_ref().map(|i| i.units.clone()).unwrap_or_default();
        if let Some((tpos, _)) = threat {
            let defenders: Vec<EntityId> = v
                .land_army
                .iter()
                .copied()
                .filter(|u| !invading.contains(u))
                .filter(|&u| w.get(u).map_or(false, |e| e.inside == 0 && matches!(e.order, Order::Idle | Order::Move { .. })))
                .collect();
            if !defenders.is_empty() && w.tick % 40 < self.diff.think_interval() {
                out.push(CommandKind::Move { units: defenders, to: tpos, attack_move: true, queue: false });
            }
            // air support at home
            let air_idle: Vec<EntityId> = v.air.iter().copied().filter(|&a| w.get(a).map_or(false, |e| e.order == Order::Idle && e.def != d.id("nuke_bomber"))).collect();
            if !air_idle.is_empty() && threat_count >= 3 {
                out.push(CommandKind::Move { units: air_idle, to: tpos, attack_move: true, queue: false });
            }
            // citizens under direct attack flee to the capitol
            if threat_count >= 4 {
                for &c in &v.citizens {
                    if let Some(ce) = w.get(c) {
                        if w.tick.wrapping_sub(ce.last_hit_tick) < 20 && !ce.pos.within(self.base, Fx::from_int(5)) {
                            out.push(CommandKind::Move { units: vec![c], to: self.base, attack_move: false, queue: false });
                        }
                    }
                }
            }
        }

        // ---- rally idle army near the base, toward the coast
        if w.tick % 200 == (self.phase * 11) % 200 && threat.is_none() {
            let idle: Vec<EntityId> = v
                .land_army
                .iter()
                .copied()
                .filter(|u| !invading.contains(u))
                .filter(|&u| w.get(u).map_or(false, |e| e.order == Order::Idle && e.inside == 0 && !e.pos.within(self.base, Fx::from_int(14))))
                .collect();
            if !idle.is_empty() {
                let to = self.rally_point(w);
                out.push(CommandKind::Move { units: idle, to, attack_move: true, queue: false });
            }
        }

        let attack_time = w.tick >= self.diff.first_attack();
        // Nuclear aircraft get an explicit target instead of joining home defense.
        if attack_time {
            if let Some(target) = self.nuclear_target(w) {
                let bombers: Vec<_> = v.air.iter().copied().filter(|&id| w.get(id).is_some_and(|e|
                    e.def == d.id("nuke_bomber") && e.ammo > 0
                    && e.fuel * 10 >= d.def(e.def).fuel_ticks * 9
                    && matches!(e.order, Order::Idle | Order::Patrol { .. })
                )).collect();
                if !bombers.is_empty() {
                    out.push(CommandKind::Attack { units: bombers, target, queue: false });
                }
            }
        }
        // One idle reconnaissance vehicle patrols the home island between waves.
        if threat.is_none() && !v.land_army.iter().any(|&id| w.get(id).is_some_and(|e| matches!(e.order, Order::Scout { .. }))) {
            if let Some(&id) = v.land_army.iter().find(|&&id| !invading.contains(&id)
                && w.get(id).is_some_and(|e| e.def == d.id("recon") && e.inside == 0
                    && e.order == Order::Idle && e.pos.within(self.base, Fx::from_int(30)))) {
                out.push(CommandKind::Scout { units: vec![id] });
            }
        }
        // ---- air strikes
        let strike_every = 20 * 60 * 2;
        if attack_time && w.tick.wrapping_sub(self.last_air_strike) > strike_every {
            let ready: Vec<EntityId> = v
                .air
                .iter()
                .copied()
                .filter(|&a| w.get(a).map_or(false, |e| e.def != d.id("nuke_bomber") && matches!(e.order, Order::Idle | Order::Patrol { .. }) && e.fuel * 10 >= d.def(e.def).fuel_ticks * 9))
                .collect();
            if ready.len() >= 4 {
                self.last_air_strike = w.tick;
                let bombers: Vec<EntityId> = ready.iter().copied().filter(|&a| w.get(a).map_or(false, |e| d.def(e.def).weapons.first().map_or(false, |wp| wp.ammo > 0))).collect();
                let others: Vec<EntityId> = ready.iter().copied().filter(|a| !bombers.contains(a)).collect();
                match self.pick_strike_target(w) {
                    Some(t) => {
                        if !bombers.is_empty() {
                            out.push(CommandKind::Attack { units: bombers, target: t, queue: false });
                        }
                        if let Some(tp) = w.get(t).map(|e| e.pos).or_else(|| self.known.get(&t).map(|k| k.pos)) {
                            if !others.is_empty() {
                                out.push(CommandKind::Move { units: others, to: tp, attack_move: true, queue: false });
                            }
                        }
                    }
                    None => {
                        // scout the enemy start with fighters
                        if let Some(es) = self.enemy_start(w) {
                            out.push(CommandKind::Move { units: ready, to: es, attack_move: true, queue: false });
                        }
                    }
                }
            }
        }

        // ---- naval push
        if attack_time && w.tick.wrapping_sub(self.last_naval_push) > 20 * 60 * 3 && v.navy.len() >= 4 {
            if let Some(es) = self.enemy_start(w) {
                let (ex, ey) = es.tile();
                if let Some((wx, wy)) = self.coast_water_near(w, ex, ey) {
                    self.last_naval_push = w.tick;
                    let ships: Vec<EntityId> = v.navy.iter().copied().filter(|&s| w.get(s).map_or(false, |e| e.order == Order::Idle)).collect();
                    if !ships.is_empty() {
                        out.push(CommandKind::Move { units: ships, to: FVec::tile_center(wx, wy), attack_move: true, queue: false });
                    }
                }
            }
        }

        // ---- hunt: overwhelming advantage or enemy nearly dead -> finish them
        if attack_time && w.tick % 400 == (self.phase * 13) % 400 {
            let my_army = v.land_army.len() + v.air.len() * 2 + v.navy.len() * 2;
            let enemy_buildings = self.known.len();
            if my_army >= 30 && enemy_buildings <= 6 || my_army >= 60 {
                if let Some(t) = self.pick_strike_target(w) {
                    if let Some(tp) = self.known.get(&t).map(|k| k.pos) {
                        let air: Vec<EntityId> = v.air.iter().copied().filter(|&a| w.get(a).map_or(false, |e| e.order == Order::Idle && e.def != d.id("nuke_bomber"))).collect();
                        if !air.is_empty() {
                            out.push(CommandKind::Move { units: air, to: tp, attack_move: true, queue: false });
                        }
                        let ships: Vec<EntityId> = v.navy.iter().copied().filter(|&s| w.get(s).map_or(false, |e| e.order == Order::Idle)).collect();
                        if !ships.is_empty() {
                            let (tx, ty) = tp.tile();
                            if let Some((wx, wy)) = self.coast_water_near(w, tx, ty) {
                                out.push(CommandKind::Move { units: ships, to: FVec::tile_center(wx, wy), attack_move: true, queue: false });
                            }
                        }
                    }
                } else if let Some(es) = self.enemy_start(w) {
                    // nothing known: scout their start with aircraft
                    let air: Vec<EntityId> = v.air.iter().copied().filter(|&a| w.get(a).is_some_and(|e| e.def != d.id("nuke_bomber"))).take(3).collect();
                    if !air.is_empty() {
                        out.push(CommandKind::Move { units: air, to: es, attack_move: true, queue: false });
                    }
                }
            }
        }

        // ---- amphibious invasion
        self.invasion_tick(w, v, out, attack_time);
    }

    /// Score only visible targets: dense enemy bases justify the expensive payload.
    fn nuclear_target(&self, w: &World) -> Option<EntityId> {
        let radius = data().def(data().id("nuke_bomber")).weapons[0].splash;
        w.entities.iter().filter(|e| e.on_map() && w.is_enemy(self.player, e.owner)
            && data().def(e.def).is_building() && w.can_see(self.player, e))
            .map(|e| {
                let score: i32 = w.entities.iter().filter(|o| o.on_map()
                    && w.is_enemy(self.player, o.owner) && w.can_see(self.player, o)
                    && o.pos.within(e.pos, radius)).map(|o| {
                        if data().def(o.def).is_building() { 5 } else { 1 }
                    }).sum();
                (score, std::cmp::Reverse(e.id))
            }).max().filter(|(score, _)| *score >= 10).map(|(_, id)| id.0)
    }

    /// Build a full granary ring once the economy can support eight farmers.
    pub(crate) fn rebuild_fields(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        if v.citizens.len() < 24 || self.last_fields_rebuild.is_some_and(|t| w.tick.wrapping_sub(t) < 600) {
            return;
        }
        let farm = d.id("farm");
        let farms = v.count(farm) + v.sites.iter().filter(|&&id| w.get(id).is_some_and(|e| e.def == farm)).count();
        let wanted = (v.citizens.len() / 3).min(8 * v.count(d.id("granary")));
        let cost = d.def(farm).data.cost.arr();
        if farms >= wanted || cost.iter().enumerate().any(|(r, c)| w.players[self.player as usize].res[r] < c * 8 + if r == 1 { 150 } else { 0 }) {
            return;
        }
        for &building in v.buildings.get(&d.id("granary")).map(Vec::as_slice).unwrap_or(&[]) {
            let (x, y) = w.get(building).unwrap().tile;
            if [(-3, 0), (3, 0), (0, -3), (0, 3), (-3, -3), (3, -3), (-3, 3), (3, 3)]
                .iter().any(|&(dx, dy)| w.can_place(self.player, farm, (x + dx, y + dy)).is_ok()) {
                out.push(CommandKind::RebuildFarms { building });
                self.last_fields_rebuild = Some(w.tick);
                break;
            }
        }
    }

    fn rally_point(&mut self, w: &World) -> FVec {
        if let Some((s, _)) = self.staging_tiles(w) {
            let sp = FVec::tile_center(s.0, s.1);
            // halfway between base and staging
            return FVec::new(Fx((self.base.x.0 + sp.x.0) / 2), Fx((self.base.y.0 + sp.y.0) / 2));
        }
        self.base
    }

    fn enemy_start(&self, w: &World) -> Option<FVec> {
        // nearest living enemy's start position
        let mut best: Option<(i64, FVec)> = None;
        for (i, pl) in w.players.iter().enumerate() {
            if pl.defeated || !w.is_enemy(self.player, i as u8) {
                continue;
            }
            if let Some(&(x, y)) = w.starts.get(i) {
                let p = FVec::tile_center(x, y);
                let dd = p.dist2_raw(self.base);
                if best.map_or(true, |(b, _)| dd < b) {
                    best = Some((dd, p));
                }
            }
        }
        best.map(|b| b.1)
    }

    fn enemy_target_player(&self, w: &World) -> Option<u8> {
        let mut best: Option<(i64, u8)> = None;
        for (i, pl) in w.players.iter().enumerate() {
            if pl.defeated || !w.is_enemy(self.player, i as u8) {
                continue;
            }
            if let Some(&(x, y)) = w.starts.get(i) {
                let dd = FVec::tile_center(x, y).dist2_raw(self.base);
                if best.map_or(true, |(b, _)| dd < b) {
                    best = Some((dd, i as u8));
                }
            }
        }
        best.map(|b| b.1)
    }

    fn pick_strike_target(&self, _w: &World) -> Option<EntityId> {
        let d = data();
        let prio = |def: DefId| -> i32 {
            match d.def(def).data.key.as_str() {
                "airport" => 0,
                "tank_factory" => 1,
                "naval_yard" => 2,
                "barracks" => 3,
                "aa_site" => 9,
                "capitol" => 5,
                "settlement" | "granary" => 6,
                _ => 7,
            }
        };
        let mut best: Option<(i32, i64, EntityId)> = None;
        for (id, k) in &self.known {
            let dist = k.pos.dist2_raw(self.base);
            let key = (prio(k.def), dist, *id);
            if best.map_or(true, |b| key < b) {
                best = Some(key);
            }
        }
        best.map(|b| b.2)
    }

    /// (own staging land tile, adjacent water tile), computed once.
    fn staging_tiles(&mut self, w: &World) -> Option<((i32, i32), (i32, i32))> {
        if self.staging.is_some() {
            return self.staging;
        }
        let target = self.enemy_start(w)?;
        let (tx, ty) = target.tile();
        let reach = land_component(w, self.base_tile);
        let mut best: Option<(i64, ((i32, i32), (i32, i32)))> = None;
        for (i, &r) in reach.iter().enumerate() {
            if !r {
                continue;
            }
            let x = i as i32 % w.map.w;
            let y = i as i32 / w.map.w;
            for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (wx, wy) = (x + ox * 2, y + oy * 2);
                if w.map.in_bounds(wx, wy) && w.map.base_pass[w.map.idx(wx, wy)] & PASS_WATER != 0 && w.map.is_water(x + ox, y + oy) {
                    let dd = ((x - tx) as i64).pow(2) + ((y - ty) as i64).pow(2);
                    // don't stage too far from home either
                    let home = ((x - self.base_tile.0) as i64).pow(2) + ((y - self.base_tile.1) as i64).pow(2);
                    let score = dd + home / 2;
                    if best.map_or(true, |(b, _)| score < b) {
                        best = Some((score, ((x, y), (wx, wy))));
                    }
                }
            }
        }
        self.staging = best.map(|b| b.1);
        self.staging
    }

    fn landing_tiles(&mut self, w: &World, enemy: u8) -> Option<((i32, i32), (i32, i32))> {
        if let Some(l) = self.landings.get(&enemy) {
            return Some(*l);
        }
        let start = *w.starts.get(enemy as usize)?;
        let reach = land_component(w, start);
        let (sx, sy) = self.staging_tiles(w).map(|s| s.0).unwrap_or(self.base_tile);
        let mut best: Option<(i64, ((i32, i32), (i32, i32)))> = None;
        for (i, &r) in reach.iter().enumerate() {
            if !r {
                continue;
            }
            let x = i as i32 % w.map.w;
            let y = i as i32 / w.map.w;
            for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (wx, wy) = (x + ox * 2, y + oy * 2);
                if w.map.in_bounds(wx, wy) && w.map.base_pass[w.map.idx(wx, wy)] & PASS_WATER != 0 && w.map.is_water(x + ox, y + oy) {
                    // close to us (short sail) but not right under their capitol guns
                    let to_us = ((x - sx) as i64).pow(2) + ((y - sy) as i64).pow(2);
                    let to_them = ((x - start.0) as i64).pow(2) + ((y - start.1) as i64).pow(2);
                    let score = to_us + if to_them < 14 * 14 { 1 << 30 } else { to_them };
                    if best.map_or(true, |(b, _)| score < b) {
                        best = Some((score, ((x, y), (wx, wy))));
                    }
                }
            }
        }
        let l = best.map(|b| b.1)?;
        self.landings.insert(enemy, l);
        Some(l)
    }

    fn coast_water_near(&self, w: &World, x: i32, y: i32) -> Option<(i32, i32)> {
        for r in 1i32..40 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let (nx, ny) = (x + dx, y + dy);
                    if w.map.in_bounds(nx, ny) && w.map.base_pass[w.map.idx(nx, ny)] & ee_sim::map::PASS_DEEP != 0 {
                        return Some((nx, ny));
                    }
                }
            }
        }
        None
    }

    fn invasion_tick(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>, attack_time: bool) {
        let d = data();
        let tick = w.tick;
        if self.invasion.is_none() {
            let wave_gap = 20 * 60 * 3;
            let ready = attack_time && tick.wrapping_sub(self.last_wave) > wave_gap && !self.defending;
            let available: Vec<EntityId> = v
                .land_army
                .iter()
                .copied()
                .filter(|&u| w.get(u).map_or(false, |e| e.inside == 0 && e.order == Order::Idle || matches!(e.order, Order::Move { .. })))
                .collect();
            if !ready || available.len() < self.diff.wave_size().min(8 + self.wave as usize * 4) || v.transports.is_empty() {
                return;
            }
            let Some(enemy) = self.enemy_target_player(w) else { return };
            let Some((stage, stage_w)) = self.staging_tiles(w) else { return };
            let Some((land, _)) = self.landing_tiles(w, enemy) else { return };
            let Some(es) = self.enemy_start(w) else { return };
            let colony_ship = self.colony.as_ref().map(|c| c.transport);
            let ships: Vec<EntityId> = v.transports.iter().copied().filter(|t| Some(*t) != colony_ship).collect();
            if ships.is_empty() {
                return;
            }
            let cap: usize = ships.len() * 12;
            let mut units = available;
            units.truncate(cap.min(60));
            let staging = FVec::tile_center(stage.0, stage.1);
            out.push(CommandKind::Move { units: units.clone(), to: staging, attack_move: false, queue: false });
            let staging_water = FVec::tile_center(stage_w.0, stage_w.1);
            out.push(CommandKind::Move { units: ships.clone(), to: staging_water, attack_move: false, queue: false });
            self.invasion = Some(Invasion {
                stage: Stage::Gather,
                units,
                transports: ships.clone(),
                staging,
                staging_water,
                landing: FVec::tile_center(land.0, land.1),
                target: es,
                stage_tick: tick,
            });
            self.wave += 1;
            self.last_wave = tick;
            return;
        }
        let mut inv = self.invasion.take().unwrap();
        inv.units.retain(|&u| w.get(u).is_some());
        inv.transports.retain(|&t| w.get(t).is_some());
        let elapsed = tick.wrapping_sub(inv.stage_tick);
        if inv.units.is_empty() || (inv.transports.is_empty() && inv.stage != Stage::Fight) {
            // wave wiped out or no boats: release survivors
            if !inv.units.is_empty() {
                out.push(CommandKind::Move { units: inv.units.clone(), to: self.base, attack_move: true, queue: false });
            }
            return;
        }
        match inv.stage {
            Stage::Gather => {
                let near = inv.units.iter().filter(|&&u| w.get(u).map_or(false, |e| e.pos.within(inv.staging, Fx::from_int(8)))).count();
                let boats = inv.transports.iter().filter(|&&t| w.get(t).map_or(false, |e| e.pos.within(inv.staging_water, Fx::from_int(5)))).count();
                if (near * 10 >= inv.units.len() * 7 && boats > 0) || elapsed > 20 * 120 {
                    // assign units to transports
                    let mut idx = 0;
                    for &t in &inv.transports {
                        let chunk: Vec<EntityId> = inv.units.iter().skip(idx).take(12).copied().collect();
                        idx += chunk.len();
                        if !chunk.is_empty() {
                            out.push(CommandKind::Target { units: chunk, target: t, queue: false });
                        }
                    }
                    inv.stage = Stage::Load;
                    inv.stage_tick = tick;
                } else if elapsed % 200 == 0 {
                    out.push(CommandKind::Move { units: inv.transports.clone(), to: inv.staging_water, attack_move: false, queue: false });
                }
            }
            Stage::Load => {
                let aboard = inv.units.iter().filter(|&&u| w.get(u).map_or(false, |e| e.inside != 0)).count();
                if aboard == inv.units.len() || elapsed > 20 * 45 {
                    let loaded: Vec<EntityId> = inv.transports.iter().copied().filter(|&t| w.get(t).map_or(false, |e| !e.cargo.is_empty())).collect();
                    if loaded.is_empty() {
                        // nobody got on: abort
                        out.push(CommandKind::Move { units: inv.units.clone(), to: self.base, attack_move: true, queue: false });
                        return;
                    }
                    out.push(CommandKind::Unload { units: loaded, at: inv.landing });
                    // escort with the fleet
                    let escort: Vec<EntityId> = v.navy.iter().copied().filter(|&s| w.get(s).map_or(false, |e| e.order == Order::Idle)).collect();
                    if !escort.is_empty() {
                        out.push(CommandKind::Move { units: escort, to: inv.landing, attack_move: true, queue: false });
                    }
                    // stragglers who never boarded go defend
                    let left: Vec<EntityId> = inv.units.iter().copied().filter(|&u| w.get(u).map_or(false, |e| e.inside == 0)).collect();
                    inv.units.retain(|u| !left.contains(u));
                    inv.stage = Stage::Sail;
                    inv.stage_tick = tick;
                } else if elapsed % 100 == 50 {
                    // re-issue boarding for units that wandered off
                    let mut idx = 0;
                    for &t in &inv.transports {
                        let room = 12usize.saturating_sub(w.get(t).map_or(12, |e| e.cargo.len()));
                        let chunk: Vec<EntityId> = inv.units.iter().copied().filter(|&u| w.get(u).map_or(false, |e| e.inside == 0)).skip(idx).take(room).collect();
                        idx += chunk.len();
                        if !chunk.is_empty() {
                            out.push(CommandKind::Target { units: chunk, target: t, queue: false });
                        }
                    }
                }
            }
            Stage::Sail => {
                let still_aboard = inv.units.iter().filter(|&&u| w.get(u).map_or(false, |e| e.inside != 0)).count();
                if still_aboard == 0 || elapsed > 20 * 150 {
                    let landed: Vec<EntityId> = inv.units.iter().copied().filter(|&u| w.get(u).map_or(false, |e| e.inside == 0)).collect();
                    if !landed.is_empty() {
                        out.push(CommandKind::Move { units: landed, to: inv.target, attack_move: true, queue: false });
                    }
                    out.push(CommandKind::Move { units: inv.transports.clone(), to: inv.staging_water, attack_move: false, queue: false });
                    inv.stage = Stage::Fight;
                    inv.stage_tick = tick;
                } else if elapsed % 300 == 150 {
                    let loaded: Vec<EntityId> = inv.transports.iter().copied().filter(|&t| w.get(t).map_or(false, |e| !e.cargo.is_empty() && e.order == Order::Idle)).collect();
                    if !loaded.is_empty() {
                        out.push(CommandKind::Unload { units: loaded, at: inv.landing });
                    }
                }
            }
            Stage::Fight => {
                // keep pushing toward known buildings; end after a while
                if elapsed % 200 == 0 {
                    let idle: Vec<EntityId> = inv.units.iter().copied().filter(|&u| w.get(u).map_or(false, |e| e.order == Order::Idle && e.inside == 0)).collect();
                    if !idle.is_empty() {
                        let to = self
                            .known
                            .values()
                            .filter(|k| d.def(k.def).is_building())
                            .min_by_key(|k| k.pos.dist2_raw(inv.landing))
                            .map(|k| k.pos)
                            .unwrap_or(inv.target);
                        out.push(CommandKind::Move { units: idle, to, attack_move: true, queue: false });
                    }
                }
                if elapsed > 20 * 60 * 4 {
                    return; // wave over; survivors keep fighting on their own
                }
            }
        }
        self.invasion = Some(inv);
    }
}

/// Flood-fill land tiles connected to `start` (terrain only, ignoring buildings).
fn land_component(w: &World, start: (i32, i32)) -> Vec<bool> {
    let n = (w.map.w * w.map.h) as usize;
    let mut seen = vec![false; n];
    let mut q = VecDeque::new();
    if !w.map.in_bounds(start.0, start.1) {
        return seen;
    }
    seen[w.map.idx(start.0, start.1)] = true;
    q.push_back(start);
    while let Some((x, y)) = q.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if !w.map.in_bounds(nx, ny) {
                continue;
            }
            let i = w.map.idx(nx, ny);
            if seen[i] || w.map.base_pass[i] & ee_sim::map::PASS_LAND == 0 {
                continue;
            }
            seen[i] = true;
            q.push_back((nx, ny));
        }
    }
    seen
}

#[allow(dead_code)]
fn _class(_: Class) {}
