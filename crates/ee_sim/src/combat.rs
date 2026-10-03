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
                apply_damage(w, target, dmg * wp.burst, wp.dmg_type, owner, src);
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

    fn impact(&mut self, p: &Proj) {
        self.events.push(SimEvent::Impact { pos: p.pos, dmg_type: p.dmg_type as u8, splash: p.splash, owner: p.owner });
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
