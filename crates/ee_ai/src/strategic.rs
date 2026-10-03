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
        let hard = self.diff >= Difficulty::Hard;
        let mature = v.citizens.len() >= 50 && w.tick >= self.diff.first_attack() * 3 / 2;

        // ---- defense: radar first, then interceptors over the capitol and big towns
        if (hard && mature) || threatened {
            let radar = d.id("radar_station");
            let abm = d.id("abm_site");
            let want_abm = if threatened { (2 + enemy_silos).min(6) } else { 1 };
            if self.count_with_sites(w, v, radar) == 0 {
                self.build_near(w, v, out, radar, self.base);
            } else if self.count_with_sites(w, v, abm) < want_abm {
                // cover the most valuable spot that no interceptor protects yet
                let abms: Vec<FVec> = w.entities.iter().filter(|e| e.alive && e.owner == p && e.def == abm).map(|e| e.pos).collect();
                let mut spots = vec![self.base];
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
        if !hard || !mature {
            return;
        }
        let silos: Vec<EntityId> = v.buildings.get(&silo).cloned().unwrap_or_default();
        let max_silos = if self.diff == Difficulty::Hardest { 2 } else { 1 };
        let rich = me.res[3] >= 2500 && me.res[4] >= 2000 && me.res[2] >= 1200;
        if silos.len() < max_silos && rich && self.count_with_sites(w, v, silo) < max_silos {
            self.build_near(w, v, out, silo, self.base);
        }
        let icbm = d.id("icbm");
        let cost = d.def(icbm).data.cost;
        for &s in &silos {
            let Some(se) = w.get(s) else { continue };
            let queued = se.production.iter().filter(|it| matches!(it, ProdItem::Unit(u) if *u == icbm)).count();
            if se.complete && se.cargo.len() + queued < 3 && queued == 0
                && me.res[2] >= cost.stone + 400 && me.res[3] >= cost.gold + 600 && me.res[4] >= cost.iron + 400 {
                out.push(CommandKind::Train { building: s, def: icbm, count: 1 });
            }
        }
        if w.tick.wrapping_sub(self.last_salvo) < 20 * 90 {
            return;
        }
        let mut ready: Vec<(EntityId, usize)> = silos.iter().filter_map(|&s| w.get(s).map(|e| (s, e.cargo.len()))).filter(|x| x.1 > 0).collect();
        let stock: usize = ready.iter().map(|x| x.1).sum();
        if stock == 0 {
            return;
        }
        let capacity = silos.len() * 3;
        let Some((at, needed)) = self.icbm_target(w, stock, capacity) else { return };
        // salvo: every missile at once so the interceptors can't take them one by one
        let mut left = needed;
        for (s, n) in ready.iter_mut() {
            while *n > 0 && left > 0 {
                out.push(CommandKind::Launch { building: *s, at });
                *n -= 1;
                left -= 1;
            }
        }
        self.last_salvo = w.tick;
    }

    /// Best known enemy cluster: (aim point, missiles needed). Waits (None) when a
    /// worthwhile target needs a bigger salvo than we have but could stockpile.
    pub(crate) fn icbm_target(&self, w: &World, stock: usize, capacity: usize) -> Option<(FVec, usize)> {
        let d = data();
        let abm = d.id("abm_site");
        let radar_known = self.known.values().any(|k| k.def == d.id("radar_station"));
        let blast = Fx::from_int(BLAST);
        let mine: Vec<FVec> = w.entities.iter().filter(|e| e.alive && e.owner == self.player && e.inside == 0).map(|e| e.pos).collect();
        let mut best: Option<(i32, FVec, usize)> = None;
        for k in self.known.values() {
            let value: i32 = self.known.values().filter(|o| o.pos.within(k.pos, blast)).map(|o| {
                match d.def(o.def).data.key.as_str() {
                    "capitol" | "missile_silo" => 12,
                    "airport" | "tank_factory" | "naval_yard" | "barracks" | "settlement" => 6,
                    "abm_site" | "radar_station" => 5,
                    _ => 2,
                }
            }).sum();
            if value < 14 {
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
            let needed = cover + 1;
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
