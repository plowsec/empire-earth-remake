//! Zone-of-control doctrine.
//!
//! The AI's territory is every island it holds; its zone is that land plus a sea belt
//! around each island. Anything hostile inside the zone - troops on our islands, ships
//! in our waters, aircraft overhead - gets an immediate, superior response from the
//! nearest forces able to reach it. Enemy footholds on islands next to the zone are
//! contested at once. Conquest proceeds island by island (contested, then enemy
//! colonies, then free islands' defenders, then the enemy home), and each conquest
//! extends the zone.
use crate::{Ai, View};
use ee_sim::command::CommandKind;
use ee_sim::defs::{Class, Layer};
use ee_sim::entity::{EntityId, Order};
use ee_sim::fixed::{FVec, Fx};
use ee_sim::world::{data, World};
use std::collections::BTreeMap;

/// Sea belt around held islands, in tiles.
const BELT: i32 = 14;

#[derive(Default)]
struct Cluster {
    sum_x: i64,
    sum_y: i64,
    n: i64,
    strength: i32,
}

impl Cluster {
    fn add(&mut self, p: FVec, s: i32) {
        self.sum_x += p.x.0 as i64;
        self.sum_y += p.y.0 as i64;
        self.n += 1;
        self.strength += s;
    }
    fn center(&self) -> FVec {
        FVec::new(Fx((self.sum_x / self.n.max(1)) as i32), Fx((self.sum_y / self.n.max(1)) as i32))
    }
}

fn value(class: Class, is_building: bool, armed: bool) -> i32 {
    if is_building {
        return if armed { 4 } else { 2 };
    }
    match class {
        Class::Citizen => 1,
        Class::Infantry => 2,
        Class::Vehicle => 4,
        Class::Ship => 5,
        Class::Aircraft => 4,
        _ => 1,
    }
}

impl Ai {
    /// Islands we hold: claimed and with at least one of our buildings on them.
    pub(crate) fn held_islands(&self, w: &World) -> Vec<usize> {
        let mut held: Vec<usize> = w
            .entities
            .iter()
            .filter(|e| e.alive && e.owner == self.player && data().def(e.def).is_building())
            .filter_map(|e| self.island_at(w, e.tile).or_else(|| self.island_at(w, (e.tile.0 - 1, e.tile.1))))
            .collect();
        held.sort();
        held.dedup();
        held
    }

    fn island_radius(&self, isl: usize) -> i32 {
        let t = self.islands.get(isl).map_or(100, |i| i.tiles) as i64;
        (ee_sim::fixed::isqrt_u64((t / 3).max(1) as u64) as i32).max(6)
    }

    /// The held island whose zone (land + sea belt) contains `p`, if any.
    fn zone_of(&self, held: &[usize], p: (i32, i32)) -> Option<usize> {
        held.iter().copied().find(|&i| {
            let c = self.islands[i].center;
            let r = self.island_radius(i) + BELT;
            (c.0 - p.0).pow(2) + (c.1 - p.1).pow(2) <= r * r
        })
    }

    /// Islands the enemy holds or stands on, other than enemy home islands.
    pub(crate) fn enemy_footholds(&self, w: &World) -> Vec<usize> {
        let homes: Vec<usize> = w.starts.iter().enumerate().filter(|(i, _)| w.is_enemy(self.player, *i as u8)).filter_map(|(_, s)| self.island_at(w, *s)).collect();
        let mut out: Vec<usize> = self.known.values().filter_map(|k| self.island_at(w, k.tile).or_else(|| self.island_at(w, (k.tile.0 - 1, k.tile.1)))).filter(|i| !homes.contains(i)).collect();
        out.sort();
        out.dedup();
        out
    }

    /// Defend the zone and contest nearby islands. Called every think.
    pub(crate) fn hold_zone(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let p = self.player;
        let d = data();
        let held = self.held_islands(w);
        if held.is_empty() {
            return;
        }
        // ---- what's inside our zone (or on islands right next to it)?
        let mut land: BTreeMap<usize, Cluster> = BTreeMap::new();
        let mut sea: BTreeMap<usize, Cluster> = BTreeMap::new();
        let mut air: BTreeMap<usize, Cluster> = BTreeMap::new();
        let mut contest: BTreeMap<usize, Cluster> = BTreeMap::new();
        let homes: Vec<usize> = w.starts.iter().enumerate().filter(|(i, _)| w.is_enemy(p, *i as u8)).filter_map(|(_, s)| self.island_at(w, *s)).collect();
        for e in &w.entities {
            if !e.alive || !e.on_map() || !w.is_enemy(p, e.owner) || !w.can_see(p, e) {
                continue;
            }
            let ed = d.def(e.def);
            if ed.is_resource() || ed.data.icbm {
                continue;
            }
            let t = e.pos.tile();
            let s = value(ed.class(), ed.is_building(), ed.can_attack());
            match ed.layer {
                Layer::Air => {
                    if let Some(z) = self.zone_of(&held, t) {
                        air.entry(z).or_default().add(e.pos, s);
                    }
                }
                Layer::Water => {
                    if let Some(z) = self.zone_of(&held, t) {
                        sea.entry(z).or_default().add(e.pos, s);
                    }
                }
                _ => {
                    let isl = self.island_at(w, t).or_else(|| self.island_at(w, (e.tile.0 - 1, e.tile.1)));
                    let Some(isl) = isl else { continue };
                    if held.contains(&isl) {
                        land.entry(isl).or_default().add(e.pos, s);
                    } else if !homes.contains(&isl) {
                        // a foothold on an island next to our zone: contest it
                        let c = self.islands[isl].center;
                        let near = held.iter().any(|&h| {
                            let hc = self.islands[h].center;
                            let r = self.island_radius(h) + self.island_radius(isl) + 45;
                            (hc.0 - c.0).pow(2) + (hc.1 - c.1).pow(2) <= r * r
                        });
                        if near {
                            contest.entry(isl).or_default().add(e.pos, s);
                        }
                    }
                }
            }
        }
        self.defending = land.get(&self.home_island).map_or(false, |c| c.strength >= 6);
        // contested islands become the next invasion targets (biggest presence first)
        let mut c: Vec<(i32, usize)> = contest.iter().map(|(i, cl)| (-cl.strength, *i)).collect();
        c.sort();
        self.contested = c.into_iter().map(|x| x.1).collect();
        if w.tick % 40 >= self.diff.think_interval() {
            return; // respond every 2 s
        }
        let invading: Vec<EntityId> = self.invasion.as_ref().map(|i| i.units.clone()).unwrap_or_default();
        let nuke = d.id("nuke_bomber");
        // ---- intruders on our islands: everyone there who can fight goes, bombers help
        for (isl, cl) in &land {
            let at = cl.center();
            let ours: Vec<EntityId> = v
                .land_army
                .iter()
                .copied()
                .filter(|u| !invading.contains(u) || self.island_at(w, w.get(*u).map_or((0, 0), |e| e.pos.tile())) == Some(*isl))
                .filter(|&u| w.get(u).map_or(false, |e| e.inside == 0 && self.island_at(w, e.pos.tile()) == Some(*isl) && !matches!(e.order, Order::Attack { .. })))
                .collect();
            if !ours.is_empty() {
                out.push(CommandKind::Move { units: ours, to: at, attack_move: true, queue: false });
            }
            let strikers: Vec<EntityId> = v.air.iter().copied().filter(|&a| w.get(a).map_or(false, |e| e.def != nuke && (e.inside != 0 || matches!(e.order, Order::Idle | Order::Patrol { .. })) && d.def(e.def).weapons.iter().any(|wp| wp.vs_ground))).collect();
            if !strikers.is_empty() && cl.strength >= 4 {
                out.push(CommandKind::Move { units: strikers, to: at, attack_move: true, queue: false });
            }
            // not enough troops on that island: the next wave goes there
            let our_strength: i32 = v.land_army.iter().filter_map(|&u| w.get(u)).filter(|e| self.island_at(w, e.pos.tile()) == Some(*isl)).map(|e| value(d.def(e.def).class(), false, true)).sum();
            if our_strength * 2 < cl.strength * 3 && *isl != self.home_island && !self.contested.contains(isl) {
                self.contested.insert(0, *isl);
            }
        }
        // ---- hostile ships in our waters: the nearest warships, outnumbering them
        for cl in sea.values() {
            let at = cl.center();
            let mut navy: Vec<(i64, EntityId)> = v.navy.iter().filter_map(|&s| w.get(s).map(|e| (e.pos.dist2_raw(at), s))).collect();
            navy.sort();
            let want = ((cl.strength * 3 / 2) / 5 + 2).max(3) as usize;
            let ships: Vec<EntityId> = navy.into_iter().take(want).map(|x| x.1).collect();
            if !ships.is_empty() {
                out.push(CommandKind::Move { units: ships, to: at, attack_move: true, queue: false });
            }
        }
        // ---- enemy aircraft overhead: fighters go up and the airfields keep sending them
        for cl in air.values() {
            let at = cl.center();
            let fighters: Vec<EntityId> = v.air.iter().copied().filter(|&a| w.get(a).map_or(false, |e| d.def(e.def).weapons.iter().any(|wp| wp.vs_air) && !matches!(e.order, Order::Attack { .. }))).collect();
            if !fighters.is_empty() {
                out.push(CommandKind::Move { units: fighters, to: at, attack_move: true, queue: false });
            }
            let fields: Vec<EntityId> = v.buildings.get(&d.id("airport")).cloned().unwrap_or_default();
            if !fields.is_empty() {
                out.push(CommandKind::SetRally { buildings: fields, to: at, target: 0 });
            }
        }
        // ---- contested islands next door: bomb and shell the foothold now, the
        // invasion machinery lands troops (see Ai::invasion_target)
        if let Some(&isl) = self.contested.first() {
            if let Some(cl) = contest.get(&isl) {
                let at = cl.center();
                let bombers: Vec<EntityId> = v.air.iter().copied().filter(|&a| w.get(a).map_or(false, |e| e.def != nuke && (e.inside != 0 || e.order == Order::Idle) && d.def(e.def).weapons.iter().any(|wp| wp.vs_ground))).collect();
                if !bombers.is_empty() {
                    out.push(CommandKind::Move { units: bombers, to: at, attack_move: true, queue: false });
                }
                let idle_navy: Vec<EntityId> = v.navy.iter().copied().filter(|&s| w.get(s).map_or(false, |e| e.order == Order::Idle)).take(6).collect();
                if !idle_navy.is_empty() {
                    out.push(CommandKind::Move { units: idle_navy, to: at, attack_move: true, queue: false });
                }
            }
        }
        // ---- quiet zone: fighters circle over our islands in turn (air superiority)
        if air.is_empty() && w.tick % 2400 < self.diff.think_interval() {
            let k = (w.tick / 2400) as usize % held.len();
            let c = self.islands[held[k]].center;
            let fields: Vec<EntityId> = v.buildings.get(&d.id("airport")).cloned().unwrap_or_default();
            if !fields.is_empty() {
                out.push(CommandKind::SetRally { buildings: fields, to: FVec::tile_center(c.0, c.1), target: 0 });
            }
        }
    }

    /// Estimated enemy strength on an island: known buildings + units seen there.
    pub(crate) fn island_strength(&self, w: &World, isl: usize) -> i32 {
        let d = data();
        let on = |t: (i32, i32)| self.island_at(w, t).or_else(|| self.island_at(w, (t.0 - 1, t.1))) == Some(isl);
        let b: i32 = self.known.values().filter(|k| on(k.tile)).map(|k| value(d.def(k.def).class(), true, d.def(k.def).can_attack())).sum();
        let u: i32 = w.entities.iter()
            .filter(|e| e.alive && e.on_map() && w.is_enemy(self.player, e.owner) && w.can_see(self.player, e) && d.def(e.def).is_unit() && d.def(e.def).layer == Layer::Land && on(e.pos.tile()))
            .map(|e| value(d.def(e.def).class(), false, true))
            .sum();
        b + u
    }

    /// Next island to take, in order: contested footholds next to our zone, enemy
    /// colonies, then the enemy home. Returns (island, aim point).
    pub(crate) fn invasion_target(&self, w: &World) -> Option<(usize, FVec)> {
        let held = self.held_islands(w);
        let near_zone = |isl: usize| -> i64 {
            let c = self.islands[isl].center;
            held.iter().map(|&h| {
                let hc = self.islands[h].center;
                ((hc.0 - c.0) as i64).pow(2) + ((hc.1 - c.1) as i64).pow(2)
            }).min().unwrap_or(i64::MAX)
        };
        let aim = |isl: usize| -> FVec {
            // their buildings on that island (nearest to us), else its centre
            let c = self.islands[isl].center;
            self.known.values()
                .filter(|k| self.island_at(w, k.tile).or_else(|| self.island_at(w, (k.tile.0 - 1, k.tile.1))) == Some(isl))
                .min_by_key(|k| k.pos.dist2_raw(self.base))
                .map(|k| k.pos)
                .unwrap_or(FVec::tile_center(c.0, c.1))
        };
        if let Some(&isl) = self.contested.first() {
            return Some((isl, aim(isl)));
        }
        let mut footholds = self.enemy_footholds(w);
        footholds.sort_by_key(|&i| near_zone(i));
        if let Some(&isl) = footholds.first() {
            return Some((isl, aim(isl)));
        }
        let enemy = self.enemy_target_player(w)?;
        let s = *w.starts.get(enemy as usize)?;
        let isl = self.island_at(w, s)?;
        Some((isl, aim(isl)))
    }
}

#[allow(dead_code)]
fn _unused(_: Layer) {}

impl Ai {
    /// Rough strength of a player: population plus buildings.
    fn might(w: &World, p: u8) -> i64 {
        let b = w.entities.iter().filter(|e| e.alive && e.owner == p && data().def(e.def).is_building()).count() as i64;
        w.players[p as usize].pop as i64 + b * 2
    }

    /// Balance of power: accept alliances against the leader, court allies when someone
    /// pulls far ahead. Never an alliance that would leave nobody to fight.
    pub(crate) fn diplomacy(&mut self, w: &World, out: &mut Vec<CommandKind>) {
        let me = self.player;
        if w.tick % 600 != (self.phase * 17) % 600 {
            return;
        }
        let standing: Vec<u8> = w.players.iter().filter(|p| !p.defeated).map(|p| p.id).collect();
        if standing.len() < 3 {
            return;
        }
        let leader = *standing.iter().max_by_key(|&&p| (Self::might(w, p), p)).unwrap();
        let mine = Self::might(w, me);
        // would allying with `q` still leave someone to fight?
        let leaves_enemy = |q: u8| standing.iter().any(|&o| o != me && o != q && w.is_enemy(me, o) && w.is_enemy(q, o));
        // answer offers
        for &q in &standing {
            if q == me || !w.is_enemy(me, q) {
                continue;
            }
            let offered = w.players[q as usize].proposals.get(me as usize).copied().unwrap_or(false);
            if offered && q != leader && leaves_enemy(q) {
                out.push(CommandKind::Diplomacy { target: q, ally: true });
            }
        }
        // court a partner against a runaway leader
        if leader != me && w.is_enemy(me, leader) && Self::might(w, leader) * 10 > mine * 14 {
            let partner = standing.iter().copied()
                .filter(|&q| q != me && q != leader && w.is_enemy(me, q) && w.is_enemy(q, leader))
                .filter(|&q| !w.players[me as usize].proposals.get(q as usize).copied().unwrap_or(false))
                .max_by_key(|&q| (Self::might(w, q), q));
            if let Some(q) = partner {
                out.push(CommandKind::Diplomacy { target: q, ally: true });
            }
        }
    }
}
