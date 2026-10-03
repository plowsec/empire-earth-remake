//! Per-unit behaviour (order execution) and movement.
use crate::combat;
use crate::defs::{Class, Layer};
use crate::entity::*;
use crate::fixed::{FVec, Fx, ONE};
use crate::orders::set_order;
use crate::path::{astar, smooth, GoalRect};
use crate::world::{data, SimEvent, World};

/// Interaction reach beyond the unit's own radius.
const REACH: Fx = Fx(ONE * 45 / 100);
const ACQUIRE_INTERVAL: u32 = 8;

impl World {
    pub(crate) fn update_units(&mut self) {
        let n = self.entities.len();
        // alternate iteration direction each tick so no player always moves first
        let forward = self.tick % 2 == 0;
        for k in 1..n {
            let i = if forward { k } else { n - k };
            let e = &self.entities[i];
            if !e.alive {
                continue;
            }
            let d = data().def(e.def);
            if !d.is_unit() {
                continue;
            }
            // cooldowns
            {
                let e = &mut self.entities[i];
                for cd in e.weapon_cd.iter_mut() {
                    if *cd > 0 {
                        *cd -= 1;
                    }
                }
                if e.acquire_cd > 0 {
                    e.acquire_cd -= 1;
                }
            }
            if self.entities[i].inside != 0 {
                self.update_inside(i);
                continue;
            }
            if self.entities[i].knockback != FVec::ZERO {
                let e = &self.entities[i];
                let next = e.pos + e.knockback;
                let clear = self.map.passable_at(next, d.layer);
                let e = &mut self.entities[i];
                if clear { e.pos = next; }
                e.knockback = if clear { e.knockback.scale(Fx::from_ratio(84, 100)) } else { FVec::ZERO };
                if e.knockback.len() < Fx::from_ratio(1, 100) { e.knockback = FVec::ZERO; }
                e.path.clear();
                e.goal = None;
                e.flow = None;
                e.action = Action::Move;
                continue;
            }
            self.behave(i);
            if !self.entities[i].alive {
                continue;
            }
            self.move_unit(i);
            if d.class() == Class::Aircraft {
                self.update_fuel(i);
            }
        }
    }

    /// Units in transports idle; aircraft at an airfield refuel and rearm.
    fn update_inside(&mut self, i: usize) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        if d.class() != Class::Aircraft || d.data.icbm {
            return;
        }
        let max_fuel = d.fuel_ticks;
        let max_ammo = d.weapons.first().map(|w| w.ammo).unwrap_or(0);
        let maxhp = self.max_hp(e);
        let e = &mut self.entities[i];
        e.action = Action::Landed;
        e.fuel = (e.fuel + max_fuel / 160 + 1).min(max_fuel);
        if e.fuel >= max_fuel {
            e.ammo = max_ammo;
        }
        if self.tick % 20 == 0 {
            e.hp = (e.hp + maxhp / 40 + 1).min(maxhp);
        }
        // resume queued orders once ready, else fly the standing sortie
        if e.fuel >= max_fuel {
            let id = e.id;
            if !e.queue.is_empty() {
                let next = e.queue.remove(0);
                set_order(self, id, next);
            } else if e.order == Order::Idle || e.order == Order::ReturnToBase {
                let home = e.inside;
                let patrol = e.sortie.or_else(|| self.get(home).and_then(|h| h.rally));
                if let Some(at) = patrol {
                    self.entities[i].sortie = Some(at);
                    set_order(self, id, Order::Patrol { at });
                }
            }
        }
    }

    pub(crate) fn next_order(&mut self, i: usize) {
        let e = &mut self.entities[i];
        let id = e.id;
        let next = if e.queue.is_empty() { Order::Idle } else { e.queue.remove(0) };
        set_order(self, id, next);
        let e = &mut self.entities[i];
        if e.order == Order::Idle {
            e.action = if e.carry > 0 { Action::Idle } else { Action::Idle };
        }
    }

    fn behave(&mut self, i: usize) {
        let order = self.entities[i].order;
        match order {
            Order::Idle => self.behave_idle(i),
            Order::Move { to, attack_move } => {
                if attack_move && self.try_acquire(i, true) {
                    return;
                }
                let e = &self.entities[i];
                let is_air = data().def(e.def).layer == Layer::Air;
                if e.goal.is_none() && e.pos != to && e.stuck == 0 && e.path.is_empty() {
                    self.set_goal(i, to, None);
                }
                let e = &self.entities[i];
                let arrived = e.goal.is_none() || e.pos.within(to, Fx::from_ratio(10, 100));
                if arrived {
                    if is_air {
                        // aircraft: stay on station (orbit/hover) until the next order
                        let e = &mut self.entities[i];
                        if !e.queue.is_empty() {
                            self.next_order(i);
                        }
                    } else {
                        self.next_order(i);
                    }
                }
            }
            Order::Attack { target } => self.behave_attack(i, target),
            Order::Gather { node } => self.behave_gather(i, node),
            Order::ReturnCargo => self.behave_return(i),
            Order::Build { site } => self.behave_build(i, site, false),
            Order::Repair { target } => self.behave_build(i, target, true),
            Order::Board { transport } => self.behave_board(i, transport),
            Order::Unload { at } => self.behave_unload(i, at),
            Order::ReturnToBase => self.behave_rtb(i),
            Order::Patrol { at } => self.behave_patrol(i, at),
            Order::Scout { idx } => self.behave_scout(i, idx),
            Order::Strike { at } => self.behave_strike(i, at),
        }
    }

    // ------------------------------------------------------------------ patrol / scout

    fn behave_patrol(&mut self, i: usize, at: FVec) {
        if self.try_acquire(i, true) {
            return;
        }
        let e = &self.entities[i];
        let d = data().def(e.def);
        let id = e.id;
        // bombers out of bombs go rearm (the sortie brings them back)
        if d.weapons.first().map_or(false, |w| w.ammo > 0) && e.ammo <= 0 {
            set_order(self, id, Order::ReturnToBase);
            return;
        }
        if d.layer != Layer::Air {
            // ground units: walk there, then stand guard
            if e.goal.is_none() && !e.pos.within(at, Fx::from_int(2)) && e.stuck == 0 {
                self.set_goal(i, at, None);
            }
            return;
        }
        let station = Fx::from_int(5);
        if !e.pos.within(at, station) {
            if e.goal.map_or(true, |g| !g.within(at, Fx::ONE)) {
                self.set_goal(i, at, None);
            }
        } else if d.data.hover {
            // helicopters hover over the point
            if e.goal.is_some() && e.pos.within(at, Fx::ONE) {
                let e = &mut self.entities[i];
                e.goal = None;
                e.path.clear();
            } else if e.goal.is_none() && !e.pos.within(at, Fx::ONE) {
                self.set_goal(i, at, None);
            }
        } else if e.goal.is_none() || e.goal.map_or(false, |g| e.pos.within(g, Fx::ONE)) {
            // jets: fly a circuit of waypoints around the point
            let rel = e.pos - at;
            let r = Fx::from_ratio(380, 100);
            // next waypoint = current direction rotated ~50 degrees around the point
            let dir = if rel.len2_raw() > 0 { rel.normalized() } else { FVec::new(Fx::ONE, Fx::ZERO) };
            let c = Fx::from_ratio(643, 1000);
            let s = Fx::from_ratio(766, 1000);
            let rot = FVec::new(dir.x.mul(c) - dir.y.mul(s), dir.x.mul(s) + dir.y.mul(c));
            let wp = at + rot.scale(r);
            self.set_goal(i, wp, None);
        }
        self.entities[i].action = Action::Move;
    }

    fn behave_scout(&mut self, i: usize, idx: u8) {
        if self.try_acquire(i, true) {
            return;
        }
        let e = &self.entities[i];
        let id = e.id;
        if e.patrol.is_empty() {
            self.next_order(i);
            return;
        }
        let k = (idx as usize) % e.patrol.len();
        let wp = e.patrol[k];
        let reach = Fx::from_ratio(250, 100);
        let arrived = e.pos.within(wp, reach) || (e.goal.is_none() && e.stuck > 0);
        if arrived {
            let n = e.patrol.len();
            let q = std::mem::take(&mut self.entities[i].queue);
            set_order(self, id, Order::Scout { idx: ((k + 1) % n) as u8 });
            self.entities[i].queue = q;
            return;
        }
        if e.goal.is_none() && e.stuck == 0 {
            self.set_goal(i, wp, None);
            if self.entities[i].goal.is_none() {
                // unreachable waypoint: skip it
                self.entities[i].stuck = 1;
            }
        }
        self.entities[i].action = Action::Move;
    }

    // ------------------------------------------------------------------ idle

    fn behave_idle(&mut self, i: usize) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        let id = e.id;
        if d.data.heal > 0 {
            self.medic_tick(i);
            return;
        }
        if d.class() == Class::Aircraft {
            if self.try_acquire(i, false) {
                return;
            }
            // airborne with nothing to do: resume the sortie or go home and land
            if d.data.needs_airport {
                let e = &self.entities[i];
                match e.sortie {
                    Some(at) => set_order(self, id, Order::Patrol { at }),
                    None => set_order(self, id, Order::ReturnToBase),
                }
            }
            return;
        }
        if d.can_attack() && d.class() != Class::Citizen {
            self.try_acquire(i, false);
        } else if d.class() == Class::Citizen {
            // citizens fight back only if attacked recently
            if self.tick.wrapping_sub(e.last_hit_tick) < 40 {
                self.try_acquire(i, false);
            }
        }
        let e = &mut self.entities[i];
        if e.order == Order::Idle && e.goal.is_none() {
            e.action = Action::Idle;
        }
    }

    /// Look for an enemy in sight; on success switch to Attack (pushing the current
    /// order back onto the queue when `resume`).
    pub(crate) fn try_acquire(&mut self, i: usize, resume: bool) -> bool {
        let e = &self.entities[i];
        if e.acquire_cd > 0 {
            return false;
        }
        let d = data().def(e.def);
        if !d.can_attack() {
            return false;
        }
        let id = e.id;
        let phase = (id % ACQUIRE_INTERVAL) as i32;
        self.entities[i].acquire_cd = ACQUIRE_INTERVAL as i32 + phase % 3;
        let Some(t) = combat::find_target(self, i) else { return false };
        let e = &mut self.entities[i];
        let cur = e.order;
        if resume && !matches!(cur, Order::Idle | Order::Attack { .. }) {
            e.queue.insert(0, cur);
        }
        let q = std::mem::take(&mut e.queue);
        set_order(self, id, Order::Attack { target: t });
        let e = &mut self.entities[i];
        e.queue = q;
        e.forced_target = false;
        true
    }

    fn medic_tick(&mut self, i: usize) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        let owner = e.owner;
        let pos = e.pos;
        let range = Fx::from_ratio(d.data.heal_range, 100);
        let sight = Fx::from_int(d.sight_tiles);
        // find the most hurt friendly infantry/citizen in sight
        let mut best: Option<(i64, usize)> = None;
        self.spatial.for_each(pos, sight, |_, slot| {
            let o = &self.entities[slot];
            if slot == i || o.owner != owner || !o.on_map() {
                return;
            }
            let od = data().def(o.def);
            if !matches!(od.class(), Class::Infantry | Class::Citizen) {
                return;
            }
            let maxhp = od.data.hp * (100 + self.players[owner as usize].mods[o.def as usize].hp_pct) / 100;
            if o.hp >= maxhp || !o.pos.within(pos, sight) {
                return;
            }
            let score = o.pos.dist2_raw(pos);
            if best.map_or(true, |(b, _)| score < b) {
                best = Some((score, slot));
            }
        });
        let Some((_, slot)) = best else {
            let e = &mut self.entities[i];
            if e.goal.is_none() {
                e.action = Action::Idle;
            }
            return;
        };
        let tpos = self.entities[slot].pos;
        if pos.within(tpos, range) {
            let e = &mut self.entities[i];
            e.goal = None;
            e.path.clear();
            e.action = Action::Attack;
            e.facing = (tpos - pos).normalized();
            if self.tick % 20 == (e.id % 20) {
                let heal = d.data.heal;
                let t = &mut self.entities[slot];
                let maxhp = data().def(t.def).data.hp;
                t.hp = (t.hp + heal).min(maxhp * 2); // clamp below by real max next line
                let mh = self.max_hp(&self.entities[slot]);
                let t = &mut self.entities[slot];
                t.hp = t.hp.min(mh);
            }
        } else if self.entities[i].repath_cd == 0 {
            self.set_goal(i, tpos, None);
            self.entities[i].repath_cd = 20;
        } else {
            self.entities[i].repath_cd -= 1;
        }
    }

    // ------------------------------------------------------------------ attack

    fn behave_attack(&mut self, i: usize, target: EntityId) {
        let e = &self.entities[i];
        let owner = e.owner;
        let d = data().def(e.def);
        let valid = match self.get(target) {
            Some(t) => t.on_map() && self.can_see(owner, t) && combat::can_attack(self, e, t),
            None => false,
        };
        if !valid {
            self.entities[i].target = 0;
            // bombers that lost the target still go home if out of bombs
            self.next_order(i);
            if d.class() != Class::Citizen && d.class() != Class::Aircraft {
                // look around for the next victim right away
                self.entities[i].acquire_cd = 0;
                if self.entities[i].order == Order::Idle {
                    self.try_acquire(i, false);
                }
            }
            return;
        }
        // drop auto-acquired targets for something more urgent every so often
        if !e.forced_target && self.tick % 16 == (e.id % 16) {
            if let Some(better) = combat::find_target(self, i) {
                if better != target {
                    let cur_d = self.get(target).map(|t| self.edge_dist(e.pos, t)).unwrap_or(Fx(i32::MAX));
                    let new_d = self.get(better).map(|t| self.edge_dist(e.pos, t)).unwrap_or(Fx(i32::MAX));
                    let range = d.max_range;
                    if cur_d > range && new_d <= range {
                        let e = &mut self.entities[i];
                        e.order = Order::Attack { target: better };
                        e.target = better;
                        return;
                    }
                }
            }
        }
        let t = self.get(target).unwrap();
        let tpos = t.pos;
        let dist = self.edge_dist(e.pos, t);
        let range = combat::attack_range(self, e, t);
        let min_range = d.weapons.iter().map(|w| w.min_range).max().unwrap_or(Fx::ZERO);
        let air = d.layer == Layer::Air;
        let is_bomber = d.weapons.first().map_or(false, |w| w.ammo > 0);
        if is_bomber {
            let e = &self.entities[i];
            if e.ammo <= 0 {
                set_order(self, e.id, Order::ReturnToBase);
                return;
            }
            // fly straight over the target and release
            if e.pos.within(tpos, Fx::from_ratio(60, 100)) || dist <= Fx::from_ratio(30, 100) {
                combat::fire_ready(self, i, target);
                let e = &mut self.entities[i];
                if e.ammo <= 0 {
                    let id = e.id;
                    set_order(self, id, Order::ReturnToBase);
                }
                return;
            }
            let e = &self.entities[i];
            if e.goal.map_or(true, |g| !g.within(tpos, Fx::from_ratio(50, 100))) {
                self.set_goal(i, tpos, None);
            }
            return;
        }
        if dist <= range && dist >= min_range {
            if air && !d.data.hover {
                // jets keep flying: strafe through and loop back
                combat::fire_ready(self, i, target);
                let e = &self.entities[i];
                let close = e.pos.within(tpos, Fx::from_int(2));
                if e.goal.is_none() || close {
                    let dir = if e.facing.len2_raw() > 0 { e.facing } else { FVec::new(Fx::ONE, Fx::ZERO) };
                    let beyond = tpos + dir.with_len(Fx::from_int(5));
                    self.set_goal(i, beyond, None);
                }
                return;
            }
            let e = &mut self.entities[i];
            e.goal = None;
            e.path.clear();
            e.flow = None;
            let dir = tpos - e.pos;
            if dir.len2_raw() > 0 {
                e.facing = dir.normalized();
            }
            e.action = Action::Attack;
            combat::fire_ready(self, i, target);
        } else if dist < min_range {
            // too close for artillery: back off
            let e = &self.entities[i];
            let away = e.pos + (e.pos - tpos).with_len(Fx::from_int(3));
            if e.goal.is_none() {
                self.set_goal(i, away, None);
            }
        } else {
            let e = &self.entities[i];
            let need = match e.goal {
                None => true,
                Some(g) => e.repath_cd == 0 && !g.within(tpos, Fx::from_int(2)),
            };
            if need {
                let rect = self.goal_rect_for(target, range);
                self.set_goal(i, tpos, rect);
                self.entities[i].repath_cd = 15;
            } else if self.entities[i].repath_cd > 0 {
                self.entities[i].repath_cd -= 1;
            }
            let e = &mut self.entities[i];
            e.action = Action::Move;
        }
    }

    /// Already next to the target area but not in reach: walk straight at it
    /// (passability stops us at its edge, which is in reach).
    pub(crate) fn approach_direct(&mut self, i: usize, target: FVec) {
        let e = &mut self.entities[i];
        e.goal = Some(target);
        e.path.clear();
        e.path.push(target);
        e.flow = None;
        e.goal_rect = true;
    }

    /// Goal rect for reaching something: its footprint grown by `reach` tiles.
    pub(crate) fn goal_rect_for(&self, target: EntityId, reach: Fx) -> Option<GoalRect> {
        let t = self.get(target)?;
        let d = data().def(t.def);
        let r = reach.floor_int().max(0);
        if d.is_building() || d.is_resource() {
            let (sw, sh) = d.size();
            Some(GoalRect { x0: t.tile.0 - 1 - r, y0: t.tile.1 - 1 - r, x1: t.tile.0 + sw + r, y1: t.tile.1 + sh + r })
        } else {
            let (x, y) = t.pos.tile();
            Some(GoalRect { x0: x - r, y0: y - r, x1: x + r, y1: y + r })
        }
    }

    // ------------------------------------------------------------------ economy

    fn behave_gather(&mut self, i: usize, node: EntityId) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        let owner = e.owner;
        let layer = d.layer;
        let Some(n) = self.get(node) else {
            // depleted: find another of the same kind nearby
            let res = e.last_res;
            let from = e.last_node_pos;
            let carry = e.carry;
            let found = if res != 255 { self.nearest_resource(res, from, 18, layer == Layer::Water) } else { None };
            let id = e.id;
            let found = if found.is_none() && carry == 0 { self.find_work(i) } else { found };
            match found {
                Some(f) => {
                    // claim the slot now so the rest of the crew picks other trees
                    if let Some(n) = self.get_mut(f) {
                        n.gatherers = n.gatherers.saturating_add(1);
                    }
                    set_order(self, id, Order::Gather { node: f })
                }
                None if carry > 0 => set_order(self, id, Order::ReturnCargo),
                None => self.next_order(i),
            }
            return;
        };
        let nd = data().def(n.def);
        let (n_owner, n_complete, npos) = (n.owner, n.complete, n.pos);
        let Some(res) = nd.data.resource else {
            self.next_order(i);
            return;
        };
        let res = res as u8;
        if nd.is_building() && (n_owner != owner || !n_complete) {
            self.next_order(i);
            return;
        }
        let farm = nd.data.walkable;
        let id = e.id;
        // carrying something else, or full: drop it off first
        let cap = d.data.carry;
        if e.carry > 0 && e.carry_res != res {
            let e = &mut self.entities[i];
            e.carry = 0;
            e.carry_res = 255;
        }
        let e = &self.entities[i];
        if e.carry >= cap {
            let e = &mut self.entities[i];
            e.last_node = node;
            e.last_node_pos = npos;
            e.last_res = res;
            e.queue.insert(0, Order::Gather { node });
            set_order(self, id, Order::ReturnCargo);
            return;
        }
        let n = self.get(node).unwrap();
        let crowd_cap = if farm { 1 } else if nd.size() == (1, 1) { 3 } else { 9 };
        let crowded = n.gatherers as i32 > crowd_cap;
        let n = self.get(node).unwrap();
        let in_reach = if farm {
            // stand on the farm field
            self.edge_dist(e.pos, n) == Fx::ZERO
        } else {
            self.edge_dist(e.pos, n) <= d.radius + REACH
        };
        if !in_reach && crowded && (id % 3 != 0 || farm) && self.tick % 10 == id % 10 {
            if let Some(alt) = self.nearest_resource(res, npos, 7, layer == Layer::Water).filter(|&a| a != node) {
                set_order(self, id, Order::Gather { node: alt });
                return;
            }
        }
        if !in_reach {
            if e.goal.is_none() {
                if farm {
                    // pick a spot in the field based on id so farmers spread out
                    let off = FVec::new(Fx(((id % 3) as i32 - 1) * ONE * 2 / 3), Fx((((id / 3) % 3) as i32 - 1) * ONE * 2 / 3));
                    self.set_goal(i, npos + off, None);
                } else {
                    let rect = self.goal_rect_for(node, Fx::ZERO);
                    // aim for the side of the node facing us
                    self.set_goal(i, npos, rect);
                }
                if self.entities[i].goal.is_none() {
                    self.approach_direct(i, npos);
                }
            }
            let e = &mut self.entities[i];
            e.action = if e.carry > 0 { Action::Carry } else { Action::Move };
            if e.stuck > 60 {
                // unreachable: try another node
                let e = &mut self.entities[i];
                e.stuck = 0;
                let found = self.nearest_resource(res, npos, 14, layer == Layer::Water).filter(|&f| f != node);
                match found {
                    Some(f) => set_order(self, id, Order::Gather { node: f }),
                    None => self.next_order(i),
                }
            }
            return;
        }
        // worker cap: wait for a free spot (or try another node)
        if e.action != Action::Gather {
            let n = self.get(node).unwrap();
            if n.miners as i32 >= crate::world::gather_cap(nd) {
                if self.tick % 10 == id % 10 {
                    if let Some(alt) = self.nearest_resource(res, npos, 8, layer == Layer::Water).filter(|&a| a != node) {
                        set_order(self, id, Order::Gather { node: alt });
                        return;
                    }
                }
                let e = &mut self.entities[i];
                e.goal = None;
                e.action = Action::Idle;
                return;
            }
            if let Some(n) = self.get_mut(node) {
                n.miners = n.miners.saturating_add(1);
            }
        }
        // gathering
        let e = &self.entities[i];
        let mods = self.mods(owner, e.def);
        let rate = d.gather_rate[res as usize] * (100 + mods.gather_pct[res as usize]) / 100;
        let e = &mut self.entities[i];
        e.goal = None;
        e.path.clear();
        e.flow = None;
        e.action = Action::Gather;
        e.last_node = node;
        e.last_node_pos = npos;
        e.last_res = res;
        e.carry_res = res;
        let dir = npos - e.pos;
        if dir.len2_raw() > 0 && !farm {
            e.facing = dir.normalized();
        }
        e.gather_acc += rate;
        let mut got = 0;
        while e.gather_acc >= 10000 && e.carry < cap {
            e.gather_acc -= 10000;
            e.carry += 1;
            got += 1;
        }
        if got > 0 && !farm {
            let regrows = nd.data.regrow > 0;
            let n = self.get_mut(node).unwrap();
            n.amount -= got;
            if n.amount <= 0 {
                if regrows {
                    n.amount = 0;
                    // exhausted for now: go home with what we have, then find another
                    let id = self.entities[i].id;
                    if self.entities[i].carry > 0 {
                        set_order(self, id, Order::ReturnCargo);
                    } else {
                        set_order(self, id, Order::Idle);
                    }
                } else {
                    self.kill(node, crate::mapgen::GAIA);
                }
            }
        }
    }

    fn behave_return(&mut self, i: usize) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        let id = e.id;
        if e.carry <= 0 {
            self.next_order(i);
            return;
        }
        let Some(drop) = self.nearest_dropsite(e.owner, e.carry_res, e.pos, d.layer) else {
            let e = &mut self.entities[i];
            e.action = Action::Idle;
            e.goal = None;
            if self.tick % 40 == 0 {
                let owner = self.entities[i].owner;
                self.events.push(SimEvent::Notice { owner, text: "No drop-off site" });
            }
            return;
        };
        let t = self.get(drop).unwrap();
        let dist = self.edge_dist(e.pos, t);
        if dist <= d.radius + REACH + Fx::from_ratio(20, 100) {
            let e = &mut self.entities[i];
            let r = e.carry_res as usize;
            let amt = e.carry;
            e.carry = 0;
            e.gather_acc = 0;
            let owner = e.owner;
            let p = &mut self.players[owner as usize];
            p.res[r] += amt;
            p.stats.gathered[r] += amt as i64;
            self.events.push(SimEvent::Gathered { owner, res: r as u8, amount: amt });
            // resume gathering (queued Gather order) or find more
            let e = &self.entities[i];
            if e.queue.is_empty() {
                let last = e.last_node;
                if self.get(last).is_some() {
                    set_order(self, id, Order::Gather { node: last });
                } else {
                    let res = e.last_res;
                    let from = e.last_node_pos;
                    let found = self.nearest_resource(res, from, 18, d.layer == Layer::Water).or_else(|| self.find_work(i));
                    match found {
                        Some(f) => set_order(self, id, Order::Gather { node: f }),
                        None => self.next_order(i),
                    }
                }
            } else {
                self.next_order(i);
            }
            return;
        }
        let e = &self.entities[i];
        let need = match e.goal {
            None => true,
            Some(_) => e.stuck > 40,
        };
        if need {
            let rect = self.goal_rect_for(drop, Fx::ZERO);
            let tpos = t.pos;
            self.set_goal(i, tpos, rect);
            if self.entities[i].goal.is_none() {
                self.approach_direct(i, tpos);
            }
            self.entities[i].stuck = 0;
        }
        self.entities[i].action = Action::Carry;
    }

    fn behave_build(&mut self, i: usize, site: EntityId, repair: bool) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        let id = e.id;
        let Some(s) = self.get(site) else {
            self.next_order(i);
            return;
        };
        if s.owner != e.owner {
            self.next_order(i);
            return;
        }
        let sd = data().def(s.def);
        let maxhp = self.max_hp(s);
        if (!repair && s.complete) || (repair && (!s.complete || s.hp >= maxhp)) {
            // finished: farmers start farming their farm
            let walk = sd.data.walkable;
            if walk && d.gather_rate[0] > 0 && self.entities[i].queue.is_empty() {
                set_order(self, id, Order::Gather { node: site });
            } else if self.entities[i].queue.is_empty() {
                // done: help with the nearest unfinished or damaged building around
                match self.nearby_build_job(i, site) {
                    Some((t, false)) => set_order(self, id, Order::Build { site: t }),
                    Some((t, true)) => set_order(self, id, Order::Repair { target: t }),
                    None => match self.find_work(i) {
                        // nothing left to build: go gather the nearest resource
                        Some(n) => set_order(self, id, Order::Gather { node: n }),
                        None => self.next_order(i),
                    },
                }
            } else {
                self.next_order(i);
            }
            return;
        }
        let dist = self.edge_dist(e.pos, s);
        if dist > d.radius + REACH {
            if e.goal.is_none() {
                let rect = self.goal_rect_for(site, Fx::ZERO);
                let spos = s.pos;
                self.set_goal(i, spos, rect);
                if self.entities[i].goal.is_none() {
                    self.approach_direct(i, spos);
                }
            }
            self.entities[i].action = Action::Move;
            if self.entities[i].stuck > 80 {
                self.next_order(i);
            }
            return;
        }
        let spos = s.pos;
        let e = &mut self.entities[i];
        e.goal = None;
        e.path.clear();
        e.action = Action::Build;
        let dir = spos - e.pos;
        if dir.len2_raw() > 0 {
            e.facing = dir.normalized();
        }
        let bt = sd.build_ticks;
        let s = self.get_mut(site).unwrap();
        if repair {
            let add = (maxhp / bt.max(1) / 2).max(1);
            s.hp = (s.hp + add).min(maxhp);
        } else {
            s.progress += 1;
            // hp grows with progress from 10% to 100%
            let target_hp = maxhp / 10 + (maxhp - maxhp / 10) * s.progress.min(bt) / bt;
            let prev_target = maxhp / 10 + (maxhp - maxhp / 10) * (s.progress - 1).min(bt) / bt;
            s.hp = (s.hp + (target_hp - prev_target)).min(maxhp);
            if s.progress >= bt {
                s.complete = true;
                let owner = s.owner;
                let def = s.def;
                if sd.data.plantable {
                    return;
                }
                self.players[owner as usize].stats.built += 1;
                self.events.push(SimEvent::BuildingComplete { id: site, def, owner });
                self.recount_pop();
                if sd.data.airport || sd.data.coastal || !sd.trains.is_empty() {
                    // rally defaults to the front door
                }
            }
        }
    }

    /// A citizen with nothing left to do: the nearest resource of the kind it last
    /// gathered (wider search), else the nearest resource of any kind. Never someone
    /// else's farm. Fishing boats keep to fish.
    pub(crate) fn find_work(&self, i: usize) -> Option<EntityId> {
        let e = &self.entities[i];
        let d = data().def(e.def);
        if d.data.gather.is_none() {
            return None;
        }
        let water = d.layer == Layer::Water;
        let pos = e.pos;
        let owner = e.owner;
        let usable = |n: EntityId| self.get(n).map_or(false, |x| {
            let nd = data().def(x.def);
            !nd.is_building() || (x.owner == owner && x.complete && x.gatherers == 0)
        });
        if e.last_res != 255 {
            if let Some(n) = self.nearest_resource(e.last_res, pos, 28, water).filter(|&n| usable(n)) {
                return Some(n);
            }
        }
        let mut best: Option<(i64, EntityId)> = None;
        for r in 0..crate::defs::NUM_RES as u8 {
            if let Some(n) = self.nearest_resource(r, pos, 16, water).filter(|&n| usable(n)) {
                let dd = self.get(n).unwrap().pos.dist2_raw(pos);
                if best.map_or(true, |b| dd < b.0) {
                    best = Some((dd, n));
                }
            }
        }
        best.map(|b| b.1)
    }

    /// Nearest own construction site (or damaged building) within 12 tiles: (id, is_repair).
    fn nearby_build_job(&self, i: usize, except: EntityId) -> Option<(EntityId, bool)> {
        let e = &self.entities[i];
        let (owner, pos) = (e.owner, e.pos);
        let r = Fx::from_int(12);
        let mut best: Option<(i64, EntityId, bool)> = None;
        for o in &self.entities {
            if !o.alive || o.owner != owner || o.id == except || !o.pos.within(pos, r) {
                continue;
            }
            let od = data().def(o.def);
            if !od.is_building() {
                continue;
            }
            let job = if !o.complete {
                Some(false)
            } else if o.hp < self.max_hp(o) && !od.data.walkable {
                Some(true)
            } else {
                None
            };
            if let Some(rep) = job {
                // finish construction before repairs
                let dd = o.pos.dist2_raw(pos) + if rep { 1 << 40 } else { 0 };
                if best.map_or(true, |b| dd < b.0) {
                    best = Some((dd, o.id, rep));
                }
            }
        }
        best.map(|b| (b.1, b.2))
    }

    /// ICBM: straight to the target point, then detonate there.
    fn behave_strike(&mut self, i: usize, at: FVec) {
        let e = &self.entities[i];
        if e.pos.within(at, Fx::from_ratio(60, 100)) {
            let d = data().def(e.def);
            let Some(wp) = d.weapons.first() else { return };
            let p = crate::world::Proj {
                owner: e.owner,
                src: e.id,
                target: 0,
                pos: at,
                aim: at,
                start: at,
                speed: Fx::ONE,
                damage: wp.damage,
                dmg_type: wp.dmg_type,
                splash: wp.splash,
                homing: false,
                weapon: 0,
                src_def: e.def,
                vs_air: false,
                id: self.next_proj,
            };
            self.next_proj = self.next_proj.wrapping_add(1);
            let id = e.id;
            self.impact(&p);
            self.kill(id, crate::mapgen::GAIA);
            return;
        }
        let e = &mut self.entities[i];
        if e.goal.is_none() {
            e.goal = Some(at);
            e.path = vec![at];
        }
        e.action = Action::Move;
    }

    // ------------------------------------------------------------------ transport

    fn behave_board(&mut self, i: usize, transport: EntityId) {
        let e = &self.entities[i];
        let size = data().def(e.def).data.cargo_size;
        let id = e.id;
        let Some(t) = self.get(transport) else {
            self.next_order(i);
            return;
        };
        let td = data().def(t.def);
        let used: i32 = t.cargo.iter().filter_map(|&c| self.get(c)).map(|c| data().def(c.def).data.cargo_size).sum();
        if t.owner != e.owner || used + size > td.data.cargo {
            self.next_order(i);
            return;
        }
        let tpos = t.pos;
        let reach = td.radius + Fx::from_ratio(260, 100);
        let building = td.is_building();
        let close = if building { self.edge_dist(e.pos, t) <= data().def(e.def).radius + Fx::from_ratio(120, 100) } else { e.pos.within(tpos, reach) };
        if building && !close {
            // walk up to any side of the fortress
            if self.entities[i].goal.is_none() {
                let rect = self.goal_rect_for(transport, Fx::ZERO);
                self.set_goal(i, tpos, rect);
                if self.entities[i].goal.is_none() {
                    self.approach_direct(i, tpos);
                }
            }
            self.entities[i].action = Action::Move;
            if self.entities[i].stuck > 120 {
                self.next_order(i);
            }
            return;
        }
        if close {
            let t = self.get_mut(transport).unwrap();
            t.cargo.push(id);
            let e = &mut self.entities[i];
            e.inside = transport;
            e.order = Order::Idle;
            if building {
                self.recount_pop();
            }
            let e = &mut self.entities[i];
            e.queue.clear();
            e.goal = None;
            e.path.clear();
            e.action = Action::Idle;
            return;
        }
        // walk to the shore tile nearest the ship
        let need = match e.goal {
            None => true,
            Some(g) => e.repath_cd == 0 && !g.within(tpos, Fx::from_int(3)),
        };
        if need {
            let (tx, ty) = tpos.tile();
            if let Some((lx, ly)) = self.map.nearest_passable(tx, ty, Layer::Land, 4) {
                self.set_goal(i, FVec::tile_center(lx, ly), None);
            } else {
                self.set_goal(i, tpos, None);
            }
            self.entities[i].repath_cd = 20;
        } else if self.entities[i].repath_cd > 0 {
            self.entities[i].repath_cd -= 1;
        }
        self.entities[i].action = Action::Move;
    }

    fn behave_unload(&mut self, i: usize, at: FVec) {
        let e = &self.entities[i];
        if e.cargo.is_empty() {
            self.next_order(i);
            return;
        }
        // find the water tile closest to `at` that borders land
        if e.goal.is_none() && e.stuck == 0 {
            let (ax, ay) = at.tile();
            let mut best: Option<(i32, (i32, i32))> = None;
            for r in 0i32..12 {
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() != r && dy.abs() != r {
                            continue;
                        }
                        let (x, y) = (ax + dx, ay + dy);
                        if !self.map.passable(x, y, Layer::Water) {
                            continue;
                        }
                        let mut shore = false;
                        for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                            if self.map.passable(x + ox, y + oy, Layer::Land) {
                                shore = true;
                            }
                        }
                        if shore {
                            let dd = dx * dx + dy * dy;
                            if best.map_or(true, |(b, _)| dd < b) {
                                best = Some((dd, (x, y)));
                            }
                        }
                    }
                }
                if best.is_some() {
                    break;
                }
            }
            match best {
                Some((_, (x, y))) => self.set_goal(i, FVec::tile_center(x, y), None),
                None => {
                    self.next_order(i);
                    return;
                }
            }
        }
        let e = &self.entities[i];
        let arrived = e.goal.is_none() || e.stuck > 30;
        if !arrived {
            // close enough to some land? unload early
            let (tx, ty) = e.pos.tile();
            let mut near_land = false;
            for (ox, oy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, -1), (1, -1), (-1, 1)] {
                if self.map.passable(tx + ox, ty + oy, Layer::Land) {
                    near_land = true;
                }
            }
            if !(near_land && e.pos.within(at, Fx::from_int(6))) {
                return;
            }
        }
        // unload: place cargo on nearby land tiles
        let tpos = e.pos;
        let cargo = std::mem::take(&mut self.entities[i].cargo);
        let (tx, ty) = tpos.tile();
        let mut spots: Vec<(i32, i32)> = Vec::new();
        for r in 1i32..6 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    if self.map.passable(tx + dx, ty + dy, Layer::Land) {
                        spots.push((tx + dx, ty + dy));
                    }
                }
            }
            if spots.len() >= cargo.len() {
                break;
            }
        }
        let mut left = Vec::new();
        for (k, c) in cargo.into_iter().enumerate() {
            if spots.is_empty() {
                left.push(c);
                continue;
            }
            let (sx, sy) = spots[k % spots.len()];
            let off = Fx(((k / spots.len().max(1)) as i32 % 3 - 1) * ONE / 4);
            let p = FVec::new(Fx(sx * ONE + ONE / 2) + off, Fx(sy * ONE + ONE / 2) - off);
            if let Some(u) = self.get_mut(c) {
                u.inside = 0;
                u.pos = p;
                u.prev_pos = p;
                u.order = Order::Idle;
                u.goal = None;
            }
            // walk toward the clicked spot if it's on land
            let (ax, ay) = at.tile();
            if self.map.passable(ax, ay, Layer::Land) {
                set_order(self, c, Order::Move { to: at, attack_move: true });
            }
        }
        self.entities[i].cargo = left;
        if self.entities[i].cargo.is_empty() {
            self.next_order(i);
        }
    }

    // ------------------------------------------------------------------ aircraft

    fn behave_rtb(&mut self, i: usize) {
        let e = &self.entities[i];
        let owner = e.owner;
        // land at the nearest friendly airfield (forward bases extend reach),
        // unless the assigned home is nearly as close
        let mut nearest: Option<(i64, EntityId)> = None;
        for o in &self.entities {
            if o.alive && o.owner == owner && o.complete && data().def(o.def).data.airport {
                let dd = o.pos.dist2_raw(e.pos);
                if nearest.map_or(true, |(b, _)| dd < b) {
                    nearest = Some((dd, o.id));
                }
            }
        }
        let home = match (self.get(e.home), nearest) {
            (Some(h), Some((nd, _))) if h.owner == owner && h.complete && data().def(h.def).data.airport
                && h.pos.dist2_raw(e.pos) <= nd * 9 / 4 => Some(e.home),
            (_, n) => n.map(|b| b.1),
        };
        let Some(home) = home else {
            // nowhere to land: loiter until fuel runs out
            self.entities[i].action = Action::Move;
            return;
        };
        let hpos = self.get(home).unwrap().pos;
        let e = &mut self.entities[i];
        e.home = home;
        if e.pos.within(hpos, Fx::from_ratio(60, 100)) {
            e.inside = home;
            e.pos = hpos;
            e.goal = None;
            e.path.clear();
            e.action = Action::Landed;
            e.order = Order::Idle;
            let id = e.id;
            if let Some(h) = self.get_mut(home) {
                if !h.cargo.contains(&id) {
                    // airports track landed planes in cargo for the UI
                    h.cargo.push(id);
                }
            }
            return;
        }
        if e.goal.map_or(true, |g| g != hpos) {
            self.set_goal(i, hpos, None);
        }
    }

    fn update_fuel(&mut self, i: usize) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        if d.fuel_ticks == 0 {
            return;
        }
        let e = &mut self.entities[i];
        e.fuel -= 1;
        if e.fuel <= 0 {
            let id = e.id;
            self.kill(id, crate::mapgen::GAIA);
            return;
        }
        // head home when fuel only covers the trip back (+20% margin)
        if !matches!(e.order, Order::ReturnToBase) && self.tick % 10 == (e.id % 10) {
            let owner = e.owner;
            let pos = e.pos;
            let speed = d.speed.0.max(1) as i64;
            let mut best: Option<i64> = None;
            for o in &self.entities {
                if o.alive && o.owner == owner && o.complete && data().def(o.def).data.airport {
                    let dist = o.pos.dist(pos).0 as i64;
                    best = Some(best.map_or(dist, |b: i64| b.min(dist)));
                }
            }
            let e = &self.entities[i];
            if let Some(dist) = best {
                let need = dist / speed * 12 / 10 + 40;
                if (e.fuel as i64) < need {
                    let id = e.id;
                    let q = std::mem::take(&mut self.entities[i].queue);
                    set_order(self, id, Order::ReturnToBase);
                    self.entities[i].queue = q;
                }
            }
        }
    }

    // ------------------------------------------------------------------ movement

    /// Plan a path for unit `i` to `to` (optionally any tile of `rect`).
    pub(crate) fn set_goal(&mut self, i: usize, to: FVec, rect: Option<GoalRect>) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        let layer = d.layer;
        let from = e.pos;
        if layer == Layer::Air {
            let e = &mut self.entities[i];
            e.goal = Some(to);
            e.path.clear();
            e.path.push(to);
            return;
        }
        let start = from.tile();
        let (gx, gy) = to.tile();
        let goal = rect.unwrap_or(GoalRect::tile(gx, gy));
        // flow field following for group moves
        if let Some((li, fx, fy)) = e.flow {
            let tick = self.tick;
            if let Some(f) = self.flows.iter_mut().find(|f| f.layer as u8 == li && f.goal == (fx, fy) && f.version == self.map.version) {
                f.last_used = tick;
                let e = &mut self.entities[i];
                e.goal = Some(to);
                e.path.clear();
                return;
            }
        }
        let tiles = astar(&self.map, &mut self.scratch, start, goal, layer, 6000);
        let mut pts = smooth(&self.map, from, &tiles, Some(to), layer);
        let reached_goal_tile = tiles.last().map_or(goal.x0 <= start.0 && start.0 <= goal.x1 && goal.y0 <= start.1 && start.1 <= goal.y1, |&(x, y)| {
            x >= goal.x0 && x <= goal.x1 && y >= goal.y0 && y <= goal.y1
        });
        // final approach to the exact point when it's walkable
        if reached_goal_tile && rect.is_none() && self.map.passable(gx, gy, layer) {
            if let Some(first) = pts.first_mut() {
                *first = to;
            } else if from != to {
                pts.push(to);
            }
        }
        let e = &mut self.entities[i];
        e.flow = None;
        e.goal_rect = rect.is_some();
        if pts.is_empty() {
            if rect.is_some() || from == to {
                // already there
                e.goal = None;
            } else {
                e.goal = Some(to);
                e.path.push(to);
            }
        } else {
            e.goal = Some(to);
            e.path = pts;
        }
    }

    fn move_unit(&mut self, i: usize) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        let layer = d.layer;
        let pos = e.pos;
        let id = e.id;
        let radius = d.radius;
        let speed = self.unit_speed(e);
        let mut new_pos = pos;
        let moving = e.goal.is_some();

        if moving {
            let goal = e.goal.unwrap();
            // next waypoint
            let wp = if let Some((li, fx, fy)) = e.flow.filter(|_| e.path.is_empty()) {
                let tick = self.tick;
                let ver = self.map.version;
                match self.flows.iter_mut().find(|f| f.layer as u8 == li && f.goal == (fx, fy) && f.version == ver) {
                    Some(f) => {
                        f.last_used = tick;
                        let (tx, ty) = pos.tile();
                        let near_goal = (tx - fx).abs() <= 6 && (ty - fy).abs() <= 6;
                        let direct = (tick + id) % 8 == 0 && self.map.line_clear(pos, goal, layer);
                        if near_goal && !direct {
                            // close to the shared goal: path to our own formation slot
                            self.entities[i].flow = None;
                            self.set_goal(i, goal, None);
                            self.entities[i].path.last().copied().unwrap_or(goal)
                        } else if direct {
                            self.entities[i].flow = None;
                            self.entities[i].path = vec![goal];
                            goal
                        } else {
                            match f.next(&self.map, tx, ty) {
                                Some((nx, ny)) => FVec::tile_center(nx, ny),
                                None => {
                                    // at the shared goal tile: finish to own formation slot
                                    self.entities[i].flow = None;
                                    goal
                                }
                            }
                        }
                    }
                    None => {
                        // field was invalidated: replan individually
                        self.entities[i].flow = None;
                        self.set_goal(i, goal, None);
                        self.entities[i].path.last().copied().unwrap_or(goal)
                    }
                }
            } else {
                e.path.last().copied().unwrap_or(goal)
            };
            let (np, reached) = pos.step_toward(wp, speed);
            new_pos = np;
            let e = &mut self.entities[i];
            if reached {
                if !e.path.is_empty() {
                    e.path.pop();
                }
                if e.path.is_empty() && e.flow.is_none() && np == goal {
                    e.goal = None;
                }
            }
            let dir = np - pos;
            if dir.len2_raw() > 0 {
                // smooth turning for render: blend facing
                let nd = dir.normalized();
                e.facing = if layer == Layer::Air {
                    (e.facing.scale(Fx::from_ratio(3, 4)) + nd.scale(Fx::from_ratio(1, 4))).normalized()
                } else {
                    nd
                };
            }
            if e.action == Action::Idle || e.action == Action::Attack || e.action == Action::Gather || e.action == Action::Build {
                e.action = if e.carry > 0 { Action::Carry } else { Action::Move };
            }
        } else if layer == Layer::Air && d.data.needs_airport && !d.data.hover {
            // jets can't hover: orbit the current position
            let e = &mut self.entities[i];
            let f = if e.facing.len2_raw() > 0 { e.facing } else { FVec::new(Fx::ONE, Fx::ZERO) };
            // rotate facing by ~6 degrees per tick (cos≈0.9945, sin≈0.1045)
            let c = Fx::from_ratio(9945, 10000);
            let s = Fx::from_ratio(1045, 10000);
            let rx = f.x.mul(c) - f.y.mul(s);
            let ry = f.x.mul(s) + f.y.mul(c);
            e.facing = FVec::new(rx, ry).normalized();
            new_pos = pos + e.facing.scale(speed).scale(Fx::from_ratio(1, 2));
            e.action = Action::Move;
        }

        // aircraft keep a loose spacing so formations don't fly through each other
        if layer == Layer::Air && !d.data.icbm {
            let mut push = FVec::ZERO;
            self.spatial.for_each(new_pos, radius.mul_int(4), |oid, slot| {
                if oid == id {
                    return;
                }
                let o = &self.entities[slot];
                let od = data().def(o.def);
                if od.layer != Layer::Air || !o.on_map() {
                    return;
                }
                // models are far bigger than their hit radius: keep ~2.5x apart
                let rs = Fx((radius + od.radius).0 * 5 / 2);
                let delta = new_pos - o.pos;
                let d2 = delta.len2_raw();
                if d2 >= rs.0 as i64 * rs.0 as i64 {
                    return;
                }
                let dir = if d2 == 0 {
                    if id > oid { FVec::new(Fx::ONE, Fx::ZERO) } else { FVec::new(-Fx::ONE, Fx::ZERO) }
                } else {
                    delta.with_len(Fx::ONE)
                };
                let dist = Fx(crate::fixed::isqrt_u64(d2 as u64) as i32);
                push += dir.scale((rs - dist).mul(Fx::from_ratio(1, 4)));
            });
            if push.len2_raw() > 0 {
                let lim = speed.mul(Fx::from_ratio(1, 3)).max(Fx::from_ratio(2, 100));
                let pl = push.len();
                new_pos += if pl > lim { push.with_len(lim) } else { push };
            }
        }
        // ships keep a wide berth (their hulls are much longer than the hit radius)
        if layer == Layer::Water {
            let mut push = FVec::ZERO;
            self.spatial.for_each(new_pos, radius.mul_int(3) + Fx::ONE, |oid, slot| {
                if oid == id {
                    return;
                }
                let o = &self.entities[slot];
                let od = data().def(o.def);
                if od.layer != Layer::Water || !o.on_map() {
                    return;
                }
                let rs = Fx((radius + od.radius).0 * 2);
                let delta = new_pos - o.pos;
                let d2 = delta.len2_raw();
                if d2 >= rs.0 as i64 * rs.0 as i64 {
                    return;
                }
                let dir = if d2 == 0 {
                    if id > oid { FVec::new(Fx::ONE, Fx::ZERO) } else { FVec::new(-Fx::ONE, Fx::ZERO) }
                } else {
                    delta.with_len(Fx::ONE)
                };
                let dist = Fx(crate::fixed::isqrt_u64(d2 as u64) as i32);
                push += dir.scale((rs - dist).mul(Fx::from_ratio(1, 8)));
            });
            if push.len2_raw() > 0 {
                let lim = speed.mul(Fx::from_ratio(1, 3)).max(Fx::from_ratio(2, 100));
                let pl = push.len();
                let cand = new_pos + if pl > lim { push.with_len(lim) } else { push };
                if self.map.passable_at(cand, layer) {
                    new_pos = cand;
                }
            }
        }
        // separation for ground and naval units
        if layer != Layer::Air {
            let mut push = FVec::ZERO;
            let mut count = 0;
            let me_moving = moving;
            self.spatial.for_each(new_pos, radius + Fx::ONE, |oid, slot| {
                if oid == id || count > 8 {
                    return;
                }
                let o = &self.entities[slot];
                let od = data().def(o.def);
                if od.layer != layer || !o.on_map() {
                    return;
                }
                let rs = radius + od.radius;
                let delta = new_pos - o.pos;
                let d2 = delta.len2_raw();
                let rr = rs.0 as i64 * rs.0 as i64;
                if d2 >= rr {
                    return;
                }
                let dist = Fx(crate::fixed::isqrt_u64(d2 as u64) as i32);
                let overlap = rs - dist;
                let dir = if d2 == 0 {
                    // deterministic tie-break by id
                    if id > oid { FVec::new(Fx::ONE, Fx::ZERO) } else { FVec::new(-Fx::ONE, Fx::ZERO) }
                } else {
                    delta.with_len(Fx::ONE)
                };
                // moving units shove idle ones more than the reverse
                let other_moving = o.goal.is_some();
                let share = match (me_moving, other_moving) {
                    (true, false) => Fx::from_ratio(1, 5),
                    (false, true) => Fx::from_ratio(4, 5),
                    _ => Fx::HALF,
                };
                push += dir.scale(overlap.mul(share));
                count += 1;
            });
            let max_push = speed.max(Fx::from_ratio(3, 100));
            if push.len2_raw() > 0 {
                let pl = push.len();
                let push = if pl > max_push { push.with_len(max_push) } else { push };
                new_pos += push;
            }
            // stay on passable ground: slide along obstacles
            if !self.map.passable_at(new_pos, layer) {
                let try_x = FVec::new(new_pos.x, pos.y);
                let try_y = FVec::new(pos.x, new_pos.y);
                if self.map.passable_at(try_x, layer) {
                    new_pos = try_x;
                } else if self.map.passable_at(try_y, layer) {
                    new_pos = try_y;
                } else if self.map.passable_at(pos, layer) {
                    new_pos = pos;
                } else {
                    // pushed into a wall somehow: pop out to the nearest open tile
                    let (tx, ty) = pos.tile();
                    if let Some((nx, ny)) = self.map.nearest_passable(tx, ty, layer, 4) {
                        new_pos = FVec::tile_center(nx, ny);
                    }
                }
            }
        }
        let max = Fx::from_int(self.map.w) - Fx::from_ratio(1, 100);
        new_pos.x = new_pos.x.max(Fx::ZERO).min(max);
        new_pos.y = new_pos.y.max(Fx::ZERO).min(Fx::from_int(self.map.h) - Fx::from_ratio(1, 100));

        let e = &mut self.entities[i];
        // stuck detection
        if moving {
            let progressed = new_pos.dist2_raw(pos) * 16 > (speed.0 as i64 * speed.0 as i64);
            if progressed {
                if e.stuck > 0 {
                    e.stuck -= 1;
                }
            } else {
                e.stuck += 2;
                if e.stuck == 40 || e.stuck == 120 {
                    // replan around whatever is in the way
                    if let Some(g) = e.goal {
                        e.path.clear();
                        e.flow = None;
                        self.set_goal(i, g, None);
                    }
                } else if e.stuck > 200 {
                    let e = &mut self.entities[i];
                    e.goal = None;
                    e.path.clear();
                }
            }
        }
        let e = &mut self.entities[i];
        e.vel = new_pos - pos;
        e.pos = new_pos;
        if !moving && e.vel.len2_raw() == 0 && matches!(e.action, Action::Move | Action::Carry) && layer != Layer::Air {
            e.action = Action::Idle;
        }
    }
}
