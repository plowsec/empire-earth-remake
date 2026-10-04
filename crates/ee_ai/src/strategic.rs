//! Nuclear deterrence: early warning radar + ABM umbrella at home, missile silos and
//! ICBM salvos sized to saturate the enemy's interceptors.
use crate::{Ai, Difficulty, View};
use ee_sim::command::CommandKind;
use ee_sim::entity::{EntityId, ProdItem};
use ee_sim::fixed::{FVec, Fx};
use ee_sim::world::{data, World};

/// ABM engagement radius and ICBM blast radius, in tiles.
const ABM_RANGE: i32 = 22;
const BLAST: i32 = 10;

impl Ai {
    pub(crate) fn strategic(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        if self.diff == Difficulty::Easy {
            return;
        }
        let d = data();
        let p = self.player;
        let me = &w.players[p as usize];
        // what the enemy is up to: silos spotted, missiles in the air (radar sees them anywhere)
        let silo = d.id("missile_silo");
        let enemy_silos = self.known.values().filter(|k| k.def == silo).count();
        for e in &w.entities {
            if e.alive && e.inside == 0 && w.is_enemy(p, e.owner) && d.def(e.def).data.icbm && (me.radar || w.can_see(p, e)) {
                self.missile_alert = w.tick;
            }
        }
        let threatened = enemy_silos > 0 || (self.missile_alert > 0 && w.tick.wrapping_sub(self.missile_alert) < 20 * 60 * 10);
        let naval = self.personality == crate::Personality::NavalNuke;
        let hard = self.diff >= Difficulty::Hard || naval;
        let mature = v.citizens.len() >= 50 && w.tick >= self.diff.first_attack() * 3 / 2;

        // ---- defense: radar first, then interceptors over the capitol and big towns
        if (hard && mature) || threatened {
            let radar = d.id("radar_station");
            let abm = d.id("abm_site");
            // one interceptor per enemy silo seen (each silo can fire 3), more if they've
            // already shot at us; at most 12
            let towns = v.count(d.id("settlement"));
            let recent_attack = self.missile_alert > 0 && w.tick.wrapping_sub(self.missile_alert) < 20 * 60 * 10;
            let want_abm = if naval { 9 } else if threatened { (2 + enemy_silos * 2 + if recent_attack { towns / 2 + 2 } else { 0 }).min(16) } else { 1 };
            if self.count_with_sites(w, v, radar) == 0 {
                self.build_near(w, v, out, radar, self.base);
            } else if self.count_with_sites(w, v, abm) < want_abm {
                // cover the most valuable spot that no interceptor protects yet
                let abms: Vec<FVec> = w.entities.iter().filter(|e| e.alive && e.owner == p && e.def == abm).map(|e| e.pos).collect();
                // the capitol, the invasion staging coast (troops bunch up there), the towns
                let mut spots = vec![self.base];
                if let Some(inv) = &self.invasion {
                    spots.push(inv.staging);
                }
                for &s in v.buildings.get(&d.id("settlement")).map(|x| x.as_slice()).unwrap_or(&[]) {
                    if let Some(e) = w.get(s) {
                        spots.push(e.pos);
                    }
                }
                let uncovered = spots.into_iter().find(|s| !abms.iter().any(|a| a.within(*s, Fx::from_int(ABM_RANGE - 6))));
                let at = uncovered.unwrap_or(self.base);
                self.build_near(w, v, out, abm, at);
            }
        }

        // ---- offense (Hard+): silos, missiles, salvos
        self.nuke_reserve = [0; 5];
        if !hard || !mature {
            return;
        }
        let silos: Vec<EntityId> = v.buildings.get(&silo).cloned().unwrap_or_default();
        // a big enemy ABM umbrella takes more silos to saturate
        let enemy_abms = self.known.values().filter(|k| k.def == d.id("abm_site")).count();
        let max_silos = if naval { 6 } else if self.diff == Difficulty::Hardest { 2 + (enemy_abms >= 4) as usize + (enemy_abms >= 8) as usize } else { 1 };
        let icbm = d.id("icbm");
        let cost = d.def(icbm).data.cost;
        // save up for the next piece of the program (the army spends only above this),
        // but never at the expense of having an army at all
        let army = v.land_army.len() + v.navy.len() + v.air.len();
        let silo_cost = d.def(silo).data.cost.arr();
        let have_silos = self.count_with_sites(w, v, silo);
        if army < if naval { 20 } else { 30 } {
            // build up forces first
        } else if have_silos < max_silos {
            self.nuke_reserve = silo_cost;
            if w.can_afford(p, &d.def(silo).data.cost) {
                self.build_near(w, v, out, silo, self.base);
            }
        } else {
            let stock: usize = silos.iter().filter_map(|&s| w.get(s)).map(|e| e.cargo.len() + e.production.len()).sum();
            if stock < silos.len() * 3 {
                self.nuke_reserve = cost.arr();
            }
        }
        for &s in &silos {
            let Some(se) = w.get(s) else { continue };
            let queued = se.production.iter().filter(|it| matches!(it, ProdItem::Unit(u) if *u == icbm)).count();
            if se.complete && se.cargo.len() + queued < 3 && queued == 0 && w.can_afford(p, &cost) {
                out.push(CommandKind::Train { building: s, def: icbm, count: 1 });
            }
        }
        // did the last salvo work? if the target area still stands, assume more interceptors
        if let Some((at, value_before, tick)) = self.last_strike {
            if w.tick.wrapping_sub(tick) > 20 * 30 {
                let blast = Fx::from_int(BLAST);
                let value_now = self.known.values().filter(|o| o.pos.within(at, blast)).count();
                if value_now * 10 >= value_before * 7 {
                    self.extra_cover.push(at);
                    if self.extra_cover.len() > 24 {
                        self.extra_cover.remove(0);
                    }
                }
                self.last_strike = None;
            }
        }
        if w.tick.wrapping_sub(self.last_salvo) < 20 * 150 {
            return;
        }
        let mut ready: Vec<(EntityId, usize)> = silos.iter().filter_map(|&s| w.get(s).map(|e| (s, e.cargo.len()))).filter(|x| x.1 > 0).collect();
        let stock: usize = ready.iter().map(|x| x.1).sum();
        if stock == 0 {
            return;
        }
        let capacity = silos.len() * 3;
        let Some((at, needed)) = self.icbm_target(w, stock, capacity) else {
            self.nuke_debug = format!("stock {stock}, holding fire (no target worth a salvo we can afford)");
            return;
        };
        self.nuke_debug = format!("stock {stock}, salvo of {needed} at {:?}", at.tile());
        let value_before = self.known.values().filter(|o| o.pos.within(at, Fx::from_int(BLAST))).count();
        self.last_strike = Some((at, value_before, w.tick));
        // salvo: every missile at once so the interceptors can't take them one by one;
        // with missiles to spare, hit more targets in the same salvo
        let mut plan = vec![(at, needed)];
        let mut spare = stock.saturating_sub(needed);
        while spare >= 3 && plan.len() < 4 {
            let used: Vec<FVec> = plan.iter().map(|p| p.0).collect();
            match self.icbm_target_excluding(w, spare, capacity, &used) {
                Some((a, n)) => {
                    plan.push((a, n));
                    spare -= n;
                }
                None => break,
            }
        }
        for (at, needed) in plan {
            let mut left = needed;
            for (s, n) in ready.iter_mut() {
                while *n > 0 && left > 0 {
                    out.push(CommandKind::Launch { building: *s, at });
                    *n -= 1;
                    left -= 1;
                }
            }
        }
        self.last_salvo = w.tick;
    }

    /// Best known enemy cluster: (aim point, missiles needed). Waits (None) when a
    /// worthwhile target needs a bigger salvo than we have but could stockpile.
    pub(crate) fn icbm_target(&self, w: &World, stock: usize, capacity: usize) -> Option<(FVec, usize)> {
        self.icbm_target_excluding(w, stock, capacity, &[])
    }

    /// Like `icbm_target`, skipping clusters within 15 tiles of `skip` (already targeted).
    pub(crate) fn icbm_target_excluding(&self, w: &World, stock: usize, capacity: usize, skip: &[FVec]) -> Option<(FVec, usize)> {
        let d = data();
        let abm = d.id("abm_site");
        let radar_known = self.known.values().any(|k| k.def == d.id("radar_station"));
        let blast = Fx::from_int(BLAST);
        let mine: Vec<FVec> = w.entities.iter().filter(|e| e.alive && e.owner == self.player && e.inside == 0).map(|e| e.pos).collect();
        let mut best: Option<(i32, FVec, usize)> = None;
        for k in self.known.values() {
            let value: i32 = self.known.values().filter(|o| o.pos.within(k.pos, blast)).map(|o| {
                match d.def(o.def).data.key.as_str() {
                    "missile_silo" => 30,
                    "radar_station" => 20,
                    "capitol" => 12,
                    "airport" | "tank_factory" | "naval_yard" | "barracks" | "settlement" => 6,
                    "abm_site" | "radar_station" => 5,
                    _ => 2,
                }
            }).sum();
            if value < 14 || skip.iter().any(|p| p.within(k.pos, Fx::from_int(15))) {
                continue;
            }
            // never nuke our own people
            if mine.iter().any(|m| m.within(k.pos, blast + Fx::from_int(3))) {
                continue;
            }
            let cover = if radar_known {
                self.known.values().filter(|o| o.def == abm && o.pos.within(k.pos, Fx::from_int(ABM_RANGE))).count()
            } else {
                0
            };
            // failed strikes nearby: the enemy has interceptors we haven't seen
            let learned = self.extra_cover.iter().filter(|p| p.within(k.pos, Fx::from_int(ABM_RANGE))).count();
            let needed = cover + learned + 1;
            if needed > capacity {
                continue;
            }
            let score = value * 10 / needed as i32;
            if best.map_or(true, |b| score > b.0) {
                best = Some((score, k.pos, needed));
            }
        }
        let (_, at, needed) = best?;
        if needed > stock {
            return None; // stockpile for a saturating salvo
        }
        Some((at, needed))
    }

    fn count_with_sites(&self, w: &World, v: &View, def: ee_sim::defs::DefId) -> usize {
        v.count(def)
            + v.sites.iter().filter(|&&s| w.get(s).map_or(false, |e| e.def == def)).count()
            + self.pending.iter().filter(|(k, _)| *k == def).count()
    }

    fn build_near(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>, def: ee_sim::defs::DefId, near: FVec) {
        let d = data();
        if !w.can_afford(self.player, &d.def(def).data.cost) {
            return;
        }
        let key = d.def(def).data.key.clone();
        let tile = self.defense_plot(w, near, &key).or_else(|| self.find_site(w, def));
        let Some(t) = tile else { return };
        let who = self.nearest_citizens(w, v, FVec::tile_center(t.0, t.1), 2, None);
        if who.is_empty() {
            return;
        }
        out.push(CommandKind::Build { units: who, def, tile: t, queue: false });
        self.pending.push((def, w.tick));
    }
}
