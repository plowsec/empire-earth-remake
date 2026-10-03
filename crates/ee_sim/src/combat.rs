//! Weapons, targeting, projectiles and the damage model:
//! `damage = max(1, attack × mult[type][armor_class] / 100 − armor)`
use crate::defs::{Class, Def, Layer, Projectile, Weapon};
use crate::entity::*;
use crate::fixed::{FVec, Fx};
use crate::mapgen::GAIA;
use crate::world::{data, Proj, SimEvent, World};

#[inline]
pub fn weapon_hits(w: &Weapon, tdef: &Def) -> bool {
    if tdef.is_resource() {
        return false;
    }
    let layer_ok = match tdef.layer {
        Layer::Air => w.vs_air,
        Layer::Water => w.vs_water,
        _ => w.vs_ground,
    };
    layer_ok && data().mult(w.dmg_type, tdef.data.armor_class) > 0
}

pub fn can_attack(_w: &World, e: &Entity, t: &Entity) -> bool {
    if e.id == t.id || !t.alive {
        return false;
    }
    let d = data().def(e.def);
    let td = data().def(t.def);
    d.weapons.iter().any(|w| weapon_hits(w, td))
}

/// Longest range among weapons that can hit `t`, including upgrades.
pub fn attack_range(w: &World, e: &Entity, t: &Entity) -> Fx {
    let d = data().def(e.def);
    let td = data().def(t.def);
    let bonus = Fx::from_ratio(w.mods(e.owner, e.def).range, 100);
    d.weapons.iter().filter(|wp| weapon_hits(wp, td)).map(|wp| wp.range + bonus).max().unwrap_or(Fx::ZERO)
}

/// Best visible enemy in sight for entity at slot `i`.
pub fn find_target(w: &World, i: usize) -> Option<EntityId> {
    let e = &w.entities[i];
    let d = data().def(e.def);
    let owner = e.owner;
    if owner == GAIA {
        return None;
    }
    let mods = w.mods(owner, e.def);
    let sight = Fx::from_int(d.sight_tiles * (100 + mods.sight_pct) / 100);
    let range_bonus = Fx::from_ratio(mods.range, 100);
    // buildings only engage within weapon range; units look as far as they can see
    let scan = if d.is_building() { d.max_range + range_bonus } else { sight.max(d.max_range + range_bonus) };
    let pos = e.pos;
    let mut best: Option<(i64, EntityId)> = None;
    w.spatial.for_each(pos, scan + Fx::from_int(2), |oid, slot| {
        let o = &w.entities[slot];
        if !o.on_map() || !w.is_enemy(owner, o.owner) {
            return;
        }
        let od = data().def(o.def);
        if !d.weapons.iter().any(|wp| weapon_hits(wp, od)) {
            return;
        }
        let dist = w.edge_dist(pos, o);
        if dist > scan {
            return;
        }
        let (tx, ty) = o.pos.tile();
        if !w.visible(owner, tx, ty) {
            return;
        }
        // priority: armed units first, then other units, buildings last
        let factor: i64 = if od.can_attack() && od.is_unit() {
            1
        } else if od.is_unit() {
            3
        } else if od.can_attack() {
            4
        } else {
            9
        };
        let score = (dist.0 as i64 + 4096) * factor;
        if best.map_or(true, |(b, bid)| score < b || (score == b && oid < bid)) {
            best = Some((score, oid));
        }
    });
    best.map(|b| b.1)
}

/// Fire every ready weapon of entity `i` that can hit `target` and is in range.
pub fn fire_ready(w: &mut World, i: usize, target: EntityId) {
    let Some(t) = w.get(target) else { return };
    let td = data().def(t.def);
    let tpos = t.pos;
    let e = &w.entities[i];
    let d = data().def(e.def);
    let mods = w.mods(e.owner, e.def);
    let dist = w.edge_dist(e.pos, t);
    let range_bonus = Fx::from_ratio(mods.range, 100);
    for (wi, wp) in d.weapons.iter().enumerate() {
        if wi >= 3 {
            break;
        }
        let e = &w.entities[i];
        if e.weapon_cd[wi] > 0 || !weapon_hits(wp, td) {
            continue;
        }
        if dist > wp.range + range_bonus || dist < wp.min_range {
            continue;
        }
        if wp.ammo > 0 && e.ammo <= 0 {
            continue;
        }
        let dmg = wp.damage * (100 + mods.attack_pct) / 100;
        let owner = e.owner;
        let src = e.id;
        let from = e.pos;
        let src_def = e.def;
        {
            let e = &mut w.entities[i];
            e.weapon_cd[wi] = wp.reload_ticks;
            e.last_fire_tick = w.tick;
            if wp.ammo > 0 {
                e.ammo -= 1;
            }
        }
        w.events.push(SimEvent::Shot { from: src, to: target, from_pos: from, to_pos: tpos, weapon: wi as u8 });
        match wp.projectile {
            Projectile::Instant => {
                w.pending_damage.push((target, dmg * wp.burst, wp.dmg_type, owner, src));
            }
            Projectile::Ballistic { .. } | Projectile::Homing { .. } => {
                let homing = matches!(wp.projectile, Projectile::Homing { .. });
                let id = w.next_proj;
                w.next_proj = w.next_proj.wrapping_add(1);
                w.projectiles.push(Proj {
                    owner,
                    src,
                    target,
                    pos: from,
                    aim: tpos,
                    start: from,
                    speed: wp.proj_speed.max(Fx::from_ratio(5, 100)),
                    damage: dmg * wp.burst,
                    dmg_type: wp.dmg_type,
                    splash: wp.splash,
                    homing,
                    weapon: wi as u8,
                    src_def,
                    vs_air: wp.vs_air,
                    id,
                });
            }
        }
    }
}

/// Raw damage → armor-adjusted HP loss on the target.
pub fn apply_damage(w: &mut World, target: EntityId, raw: i32, dt: crate::defs::DamageType, attacker_owner: u8, attacker: EntityId) {
    let Some(t) = w.get(target) else { return };
    let td = data().def(t.def);
    if td.is_resource() {
        return;
    }
    let mult = data().mult(dt, td.data.armor_class);
    if mult <= 0 {
        return;
    }
    let armor = td.data.armor + w.mods(t.owner, t.def).armor;
    let mut dmg = raw * mult / 100 - armor;
    if dmg < 1 {
        dmg = 1;
    }
    let tick = w.tick;
    let towner = t.owner;
    let tpos = t.pos;
    let t = w.get_mut(target).unwrap();
    t.hp -= dmg;
    t.last_hit_tick = tick;
    let dead = t.hp <= 0;
    w.events.push(SimEvent::Damaged { id: target, owner: towner, attacker_owner, pos: tpos });
    if dead {
        if let Some(a) = w.get_mut(attacker) {
            a.kills = a.kills.saturating_add(1);
        }
        w.kill(target, attacker_owner);
        return;
    }
    call_for_help(w, target, attacker);
    // retaliate: idle armed units turn on their attacker
    let t = w.get(target).unwrap();
    if t.order == Order::Idle && td.is_unit() && td.class() != Class::Citizen {
        if let Some(a) = w.get(attacker) {
            if can_attack(w, t, a) {
                let slot = slot_of(target);
                crate::orders::set_order(w, target, Order::Attack { target: attacker });
                w.entities[slot].forced_target = false;
            }
        }
    }
}

impl World {
    pub(crate) fn update_projectiles(&mut self) {
        let mut projs = std::mem::take(&mut self.projectiles);
        let mut keep = Vec::with_capacity(projs.len());
        for mut p in projs.drain(..) {
            if p.homing {
                if let Some(t) = self.get(p.target) {
                    if t.on_map() {
                        p.aim = t.pos;
                    }
                }
            }
            let (np, arrived) = p.pos.step_toward(p.aim, p.speed);
            p.pos = np;
            if !arrived {
                keep.push(p);
                continue;
            }
            self.impact(&p);
        }
        // projectiles spawned during impacts (none today) would be in self.projectiles
        keep.append(&mut self.projectiles);
        self.projectiles = keep;
    }

    pub(crate) fn update_shockwaves(&mut self) {
        const DURATION: u32 = 30; // 1.5 seconds at 20 Hz, matching the visible dust front.
        let mut waves = std::mem::take(&mut self.shockwaves);
        for wave in &mut waves {
            wave.age += 1;
            let front = Fx((wave.radius.0 as i64 * wave.age.min(DURATION) as i64 / DURATION as i64) as i32);
            let hits: Vec<_> = self.entities.iter().filter(|e| e.on_map() && !wave.hit.contains(&e.id))
                .filter(|e| data().def(e.def).layer != Layer::Air && self.edge_dist(wave.pos, e) <= front)
                .map(|e| (e.id, e.def, e.pos, e.owner, self.edge_dist(wave.pos, e))).collect();
            for (id, def, pos, owner, dist) in hits {
                wave.hit.push(id);
                let d = data().def(def);
                let away = (pos - wave.pos).normalized();
                if d.data.resource == Some(crate::defs::Res::Wood) || d.data.plantable {
                    self.kill(id, GAIA);
                    self.events.push(SimEvent::TreeFelled { id, def, pos, away });
                    continue;
                }
                if d.is_resource() { continue; }
                if self.is_enemy(wave.owner, owner) {
                    let falloff = 100 - (dist.0 as i64 * 50 / wave.radius.0.max(1) as i64) as i32;
                    apply_damage(self, id, wave.damage * falloff / 100, crate::defs::DamageType::Nuclear, wave.owner, wave.src);
                }
                // Pressure moves surviving friendly and enemy units. Damage follows
                // the game's existing splash-team rules; buildings stay anchored.
                if d.is_unit() {
                    if let Some(e) = self.get_mut(id) {
                        let strength = if d.class() == Class::Ship { 9 } else if d.class() == Class::Vehicle { 16 } else { 28 };
                        e.knockback = away.scale(Fx::from_ratio(strength, 100));
                    }
                }
            }
        }
        waves.retain(|wave| wave.age < DURATION);
        self.shockwaves = waves;
    }

    fn impact(&mut self, p: &Proj) {
        self.events.push(SimEvent::Impact { pos: p.pos, dmg_type: p.dmg_type as u8, splash: p.splash, owner: p.owner });
        if p.dmg_type == crate::defs::DamageType::Nuclear && p.splash > Fx::ZERO {
            self.shockwaves.push(crate::world::Shockwave { pos: p.pos, radius: p.splash, age: 0,
                damage: p.damage, owner: p.owner, src: p.src, hit: Vec::new() });
            return;
        }
        // heavy ordnance flattens forests: wood is the resource wars consume
        use crate::defs::DamageType as D;
        if p.splash.0 > 0 && matches!(p.dmg_type, D::Explosive | D::Bomb | D::NavalGun | D::Nuclear) {
            let r = p.splash.floor_int().max(1);
            let (cx, cy) = p.pos.tile();
            let mut felled = Vec::new();
            for y in cy - r..=cy + r {
                for x in cx - r..=cx + r {
                    if !self.map.in_bounds(x, y) || (x - cx).pow(2) + (y - cy).pow(2) > r * r {
                        continue;
                    }
                    let occ = self.map.occupant[self.map.idx(x, y)];
                    if let Some(e) = self.get(occ) {
                        let dd = data().def(e.def);
                        if dd.data.resource == Some(crate::defs::Res::Wood) || dd.data.plantable {
                            felled.push(occ);
                        }
                    }
                }
            }
            let pct = if p.dmg_type == D::Nuclear { 92 } else { 40 };
            for t in felled {
                if self.rng.chance(pct) {
                    self.kill(t, GAIA);
                }
            }
        }
        if p.splash.0 > 0 {
            // area damage: full at the center, 50% at the edge; spares allies
            let mut hits: Vec<(EntityId, i32)> = Vec::new();
            let r = p.splash;
            self.spatial.for_each(p.pos, r + Fx::from_int(3), |oid, slot| {
                let o = &self.entities[slot];
                if !o.on_map() || !self.is_enemy(p.owner, o.owner) {
                    return;
                }
                let od = data().def(o.def);
                if od.layer == Layer::Air && !p.vs_air {
                    return;
                }
                let dist = self.edge_dist(p.pos, o);
                if dist > r {
                    return;
                }
                let falloff = 100 - (dist.0 as i64 * 50 / r.0.max(1) as i64) as i32;
                hits.push((oid, p.damage * falloff / 100));
            });
            for (id, dmg) in hits {
                apply_damage(self, id, dmg, p.dmg_type, p.owner, p.src);
            }
        } else if let Some(t) = self.get(p.target) {
            // single-target shells can miss a unit that moved away
            if t.on_map() && (p.homing || self.edge_dist(p.pos, t) <= Fx::from_ratio(35, 100)) {
                apply_damage(self, p.target, p.damage, p.dmg_type, p.owner, p.src);
            }
        }
    }
}

#[allow(dead_code)]
fn _unused(_: FVec) {}

/// A unit under fire it can't answer (e.g. infantry vs helicopter) calls nearby
/// friends that can hit the attacker and are within their own reach.
pub fn call_for_help(w: &mut World, victim: EntityId, attacker: EntityId) {
    let tick = w.tick;
    let Some(v) = w.get(victim) else { return };
    let Some(a) = w.get(attacker) else { return };
    if tick.wrapping_sub(v.help_tick) < 30 && v.help_tick != 0 {
        return;
    }
    let can_answer = can_attack(w, v, a) && data().def(v.def).class() != Class::Citizen;
    if can_answer {
        return;
    }
    let owner = v.owner;
    let vpos = v.pos;
    let apos = a.pos;
    let vslot = slot_of(victim);
    w.entities[vslot].help_tick = tick;
    let mut helpers: Vec<(i64, EntityId)> = Vec::new();
    w.spatial.for_each(vpos, Fx::from_int(14), |oid, slot| {
        let o = &w.entities[slot];
        if oid == victim || o.owner != owner || !o.on_map() {
            return;
        }
        let od = data().def(o.def);
        if !od.is_unit() || od.class() == Class::Citizen {
            return;
        }
        let free = matches!(o.order, Order::Idle | Order::Patrol { .. } | Order::Scout { .. } | Order::Move { attack_move: true, .. });
        if !free {
            return;
        }
        let Some(a) = w.get(attacker) else { return };
        if !can_attack(w, o, a) {
            return;
        }
        // only if the attacker is within this unit's reach (what it can see/chase)
        let reach = Fx::from_int(od.sight_tiles) + od.max_range;
        let dist = o.pos.dist(apos);
        if dist > reach {
            return;
        }
        helpers.push((dist.0 as i64, oid));
    });
    helpers.sort();
    for (_, h) in helpers.into_iter().take(8) {
        let slot = slot_of(h);
        let cur = w.entities[slot].order;
        if matches!(cur, Order::Patrol { .. } | Order::Scout { .. } | Order::Move { .. }) {
            w.entities[slot].queue.insert(0, cur);
        }
        let q = std::mem::take(&mut w.entities[slot].queue);
        crate::orders::set_order(w, h, Order::Attack { target: attacker });
        w.entities[slot].queue = q;
        w.entities[slot].forced_target = false;
    }
}
