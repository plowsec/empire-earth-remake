//! War footing. A human doesn't attack an island with a raid: they push a fleet into
//! its waters, bomb it, ring it with ships and keep coming. The AI keeps a memory of the
//! enemy forces around each island it holds; when that pressure becomes a siege it
//! mobilizes - more factories in parallel, military production first, every new unit
//! rallied to the threatened island - and it concentrates its forces at a staging point
//! and strikes once it outweighs the threat, instead of feeding units in piecemeal.
use crate::{Ai, View};
use ee_sim::command::CommandKind;
use ee_sim::defs::{Class, DefId, Layer};
use ee_sim::entity::{EntityId, Order};
use ee_sim::fixed::{FVec, Fx};
use ee_sim::world::{data, World};
use std::collections::BTreeMap;

/// How long an enemy unit seen near our islands is remembered (ticks).
const MEMORY: u32 = 20 * 180;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Seen {
    pos: FVec,
    value: i32,
    layer: u8,
    tick: u32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct War {
    pub island: usize,
    /// enemy value around the island, and how much of it is at sea / in the air
    pub threat: i32,
    pub sea: i32,
    pub air: i32,
    pub center: FVec,
    pub since: u32,
    pub quiet_since: u32,
    /// factories we had when the war started: mobilization adds to these
    pub base_yards: usize,
    pub base_fields: usize,
    pub base_factories: usize,
    pub staging: FVec,
    pub staging_water: FVec,
    pub committed: bool,
}

/// What a unit is worth in a fight: roughly its price, metal counting double.
pub(crate) fn unit_value(def: DefId) -> i32 {
    let c = data().def(def).data.cost;
    (((c.gold + c.iron) * 2 + c.wood + c.food) / 100).max(1)
}

fn layer_code(l: Layer) -> u8 {
    match l {
        Layer::Water => 1,
        Layer::Air => 2,
        _ => 0,
    }
}

impl Ai {
    /// Our fighting strength (same scale as the threat).
    fn our_value(&self, w: &World, v: &View) -> i32 {
        v.land_army.iter().chain(v.navy.iter()).chain(v.air.iter()).filter_map(|&u| w.get(u)).map(|e| unit_value(e.def)).sum()
    }

    /// Remember the enemy forces seen around our islands; decide whether we're at war.
    pub(crate) fn assess_threat(&mut self, w: &World, v: &View) {
        let d = data();
        let p = self.player;
        let tick = w.tick;
        let held = self.held_islands(w);
        // refresh what we see now
        for e in &w.entities {
            if !e.alive || !e.on_map() || !w.is_enemy(p, e.owner) || !w.can_see(p, e) {
                continue;
            }
            let dd = d.def(e.def);
            if !dd.is_unit() || dd.class() == Class::Citizen || dd.data.icbm || !dd.can_attack() {
                continue;
            }
            if self.zone_of(&held, e.pos.tile()).is_none() {
                self.threat_memory.remove(&e.id);
                continue;
            }
            self.threat_memory.insert(e.id, Seen { pos: e.pos, value: unit_value(e.def), layer: layer_code(dd.layer), tick });
        }
        // forget the old and the dead
        self.threat_memory.retain(|id, s| tick.wrapping_sub(s.tick) < MEMORY && w.get(*id).is_some());
        // pressure per island
        let mut per: BTreeMap<usize, (i32, i32, i32, i64, i64)> = BTreeMap::new();
        for s in self.threat_memory.values() {
            if let Some(z) = self.zone_of(&held, s.pos.tile()) {
                let x = per.entry(z).or_default();
                x.0 += s.value;
                if s.layer == 1 { x.1 += s.value; }
                if s.layer == 2 { x.2 += s.value; }
                x.3 += s.pos.x.0 as i64 * s.value as i64;
                x.4 += s.pos.y.0 as i64 * s.value as i64;
            }
        }
        let worst = per.iter().max_by_key(|(i, x)| (x.0, std::cmp::Reverse(**i))).map(|(i, x)| (*i, *x));
        let ours = self.our_value(w, v);
        match worst {
            Some((isl, (threat, sea, air, sx, sy))) if threat >= 12 && threat * 10 >= ours * 3 => {
                let center = FVec::new(Fx((sx / threat as i64) as i32), Fx((sy / threat as i64) as i32));
                let same = self.war.as_ref().map_or(false, |wr| wr.island == isl);
                if same {
                    let wr = self.war.as_mut().unwrap();
                    wr.threat = threat;
                    wr.sea = sea;
                    wr.air = air;
                    wr.center = center;
                    wr.quiet_since = tick;
                } else {
                    // staging: our side of the threatened island, toward home
                    let home = self.base;
                    let (stage, water) = self.coast_toward(w, isl, if isl == self.home_island { center } else { home }).unwrap_or((center.tile(), center.tile()));
                    let id = |k: &str| d.id(k);
                    self.war = Some(crate::defense::War {
                        island: isl,
                        threat,
                        sea,
                        air,
                        center,
                        since: tick,
                        quiet_since: tick,
                        base_yards: v.count(id("naval_yard")),
                        base_fields: v.count(id("airport")),
                        base_factories: v.count(id("tank_factory")) + v.count(id("barracks")),
                        staging: FVec::tile_center(stage.0, stage.1),
                        staging_water: FVec::tile_center(water.0, water.1),
                        committed: false,
                    });
                }
            }
            _ => {
                // the pressure is gone: stand down after a minute and a half of quiet
                if let Some(wr) = &self.war {
                    if tick.wrapping_sub(wr.quiet_since) > 20 * 90 {
                        self.war = None;
                    }
                }
            }
        }
    }

    /// Extra production buildings while at war (front of the build list).
    pub(crate) fn war_buildings(&self, v: &View) -> Vec<DefId> {
        let d = data();
        let Some(wr) = &self.war else { return vec![] };
        let mut out = Vec::new();
        // a factory per ~40 of threat, of the kind that answers it
        let extra = (1 + wr.threat / 40).min(4) as usize;
        let sea_heavy = wr.sea * 10 >= wr.threat * 3;
        let air_heavy = wr.air * 10 >= wr.threat * 3;
        if sea_heavy && v.count(d.id("naval_yard")) < wr.base_yards + extra {
            out.push(d.id("naval_yard"));
        }
        // bombers break fleets and fighters break air raids: airfields either way
        if (sea_heavy || air_heavy) && v.count(d.id("airport")) < wr.base_fields + extra {
            out.push(d.id("airport"));
        }
        if !sea_heavy && !air_heavy && v.count(d.id("tank_factory")) + v.count(d.id("barracks")) < wr.base_factories + extra {
            out.push(d.id("tank_factory"));
        }
        if air_heavy {
            out.push(d.id("aa_site"));
        }
        out
    }

    /// Mobilized forces: rally production to the threatened island, mass, then strike.
    pub(crate) fn wage_war(&mut self, w: &World, v: &View, out: &mut Vec<CommandKind>) {
        let d = data();
        let Some(wr) = self.war.clone() else { return };
        let tick = w.tick;
        if tick % 200 >= self.diff.think_interval() {
            return;
        }
        let isl_center = { let c = self.islands[wr.island].center; FVec::tile_center(c.0, c.1) };
        // every new ship and plane goes to the threatened island
        let yards: Vec<EntityId> = v.buildings.get(&d.id("naval_yard")).cloned().unwrap_or_default();
        if !yards.is_empty() {
            out.push(CommandKind::SetRally { buildings: yards, to: wr.staging_water, target: 0 });
        }
        let fields: Vec<EntityId> = v.buildings.get(&d.id("airport")).cloned().unwrap_or_default();
        if !fields.is_empty() {
            out.push(CommandKind::SetRally { buildings: fields, to: isl_center, target: 0 });
        }
        // the force: every warship and combat aircraft not busy with a landing
        let invading: Vec<EntityId> = self.invasion.as_ref().map(|i| i.units.clone()).unwrap_or_default();
        let nuke = d.id("nuke_bomber");
        let ships: Vec<EntityId> = v.navy.iter().copied().filter(|s| !invading.contains(s)).collect();
        let planes: Vec<EntityId> = v.air.iter().copied().filter(|&a| w.get(a).map_or(false, |e| e.def != nuke)).collect();
        let r = Fx::from_int(14);
        let near = |id: EntityId, at: FVec| w.get(id).map_or(false, |e| e.pos.within(at, r));
        let massed: i32 = ships.iter().filter(|&&s| near(s, wr.staging_water) || near(s, wr.center)).chain(planes.iter().filter(|&&a| near(a, isl_center) || near(a, wr.center)))
            .filter_map(|&u| w.get(u)).map(|e| unit_value(e.def)).sum();
        // are they hitting the island itself?
        let hit = w.entities.iter().any(|e| e.alive && e.owner == self.player && d.def(e.def).is_building()
            && tick.wrapping_sub(e.last_hit_tick) < 100 && self.island_at(w, e.tile).or_else(|| self.island_at(w, (e.tile.0 - 1, e.tile.1))) == Some(wr.island));
        let strike = wr.committed || massed * 10 >= wr.threat * 12 || (hit && massed * 10 >= wr.threat * 6);
        if strike {
            if let Some(x) = self.war.as_mut() {
                x.committed = true;
            }
            // everyone in: ships at the enemy fleet, aircraft over it
            if !ships.is_empty() {
                out.push(CommandKind::Move { units: ships, to: wr.center, attack_move: true, queue: false });
            }
            if !planes.is_empty() {
                out.push(CommandKind::Move { units: planes, to: wr.center, attack_move: true, queue: false });
            }
        } else {
            // mass at the staging point (out of their reach), don't trickle in
            let stray_ships: Vec<EntityId> = ships.iter().copied().filter(|&s| !near(s, wr.staging_water) && w.get(s).map_or(false, |e| !matches!(e.order, Order::Attack { .. }))).collect();
            if !stray_ships.is_empty() {
                out.push(CommandKind::Move { units: stray_ships, to: wr.staging_water, attack_move: false, queue: false });
            }
            let stray_planes: Vec<EntityId> = planes.iter().copied().filter(|&a| !near(a, isl_center) && w.get(a).map_or(false, |e| !matches!(e.order, Order::Attack { .. }))).collect();
            if !stray_planes.is_empty() {
                out.push(CommandKind::Move { units: stray_planes, to: isl_center, attack_move: true, queue: false });
            }
        }
    }

    pub(crate) fn war_debug(&self) -> String {
        match &self.war {
            Some(wr) => format!("WAR isl{} threat {} (sea {} air {}) {}", wr.island, wr.threat, wr.sea, wr.air, if wr.committed { "striking" } else { "massing" }),
            None => "peace".into(),
        }
    }
}
