//! Building updates: unit production, research, defensive fire, healing.
use crate::combat;
use crate::defs::{Class, Layer, Stat, NUM_RES};
use crate::entity::*;
use crate::fixed::{FVec, Fx};
use crate::mapgen::GAIA;
use crate::orders::{give, set_order};
use crate::world::{data, SimEvent, UnitMods, World};

impl World {
    pub(crate) fn update_buildings(&mut self) {
        let n = self.entities.len();
        let periodic = self.tick % 20 == 0;
        if periodic {
            self.recount_gatherers();
        }
        for i in 1..n {
            let e = &self.entities[i];
            if !e.alive || e.owner == GAIA {
                continue;
            }
            let d = data().def(e.def);
            if !d.is_building() || !e.complete {
                continue;
            }
            for cd in self.entities[i].weapon_cd.iter_mut() {
                if *cd > 0 {
                    *cd -= 1;
                }
            }
            if !self.entities[i].production.is_empty() {
                self.update_production(i);
            }
            if d.can_attack() {
                self.building_fire(i);
            }
            if d.data.heal > 0 && periodic {
                self.building_heal(i);
            }
        }
    }

    fn update_production(&mut self, i: usize) {
        let e = &self.entities[i];
        let owner = e.owner;
        let item = e.production[0];
        let ticks = match item {
            ProdItem::Unit(u) => data().def(u).build_ticks,
            ProdItem::Tech(t) => data().techs[t as usize].research_ticks,
        };
        if let ProdItem::Unit(_) = item {
            let p = &self.players[owner as usize];
            // pop includes this unit already (see recount_pop)
            if p.pop > p.pop_cap {
                if self.tick % 100 == 0 {
                    self.events.push(SimEvent::Notice { owner, text: "Population limit reached: build houses" });
                }
                return;
            }
        }
        let e = &mut self.entities[i];
        e.prod_progress += 1;
        if e.prod_progress < ticks {
            return;
        }
        e.prod_progress = 0;
        e.production.remove(0);
        match item {
            ProdItem::Unit(u) => self.finish_unit(i, u),
            ProdItem::Tech(t) => {
                let p = &mut self.players[owner as usize];
                p.techs[t as usize] = true;
                p.researching[t as usize] = false;
                self.recompute_mods(owner);
                self.events.push(SimEvent::ResearchComplete { owner, tech: t });
            }
        }
        self.recount_pop();
    }

    fn finish_unit(&mut self, i: usize, u: crate::defs::DefId) {
        let e = &self.entities[i];
        let owner = e.owner;
        let bid = e.id;
        let rally = e.rally;
        let rally_target = e.rally_target;
        let bpos = e.pos;
        let ud = data().def(u);
        let spawn_at = if ud.layer == Layer::Air {
            Some(bpos)
        } else {
            self.exit_tile(e, ud.layer, rally)
        };
        let Some(pos) = spawn_at else {
            // nowhere to put it (naval yard landlocked?) – refund
            let cost = ud.data.cost;
            self.refund(owner, &cost);
            self.events.push(SimEvent::Notice { owner, text: "No room to deploy unit" });
            return;
        };
        let id = self.spawn(u, owner, pos);
        self.players[owner as usize].stats.trained += 1;
        if ud.layer == Layer::Air {
            if let Some(a) = self.get_mut(id) {
                a.home = bid;
            }
            if ud.data.needs_airport {
                match rally {
                    Some(r) => {
                        set_order(self, id, Order::Patrol { at: r });
                        if let Some(a) = self.get_mut(id) {
                            a.sortie = Some(r);
                        }
                    }
                    None => {
                        // park in the hangar until given a mission
                        if let Some(a) = self.get_mut(id) {
                            a.inside = bid;
                            a.action = Action::Landed;
                        }
                        if let Some(b) = self.get_mut(bid) {
                            b.cargo.push(id);
                        }
                    }
                }
                return;
            }
        }
        // rally: gather if pointed at a resource, else move there
        if rally_target != 0 {
            if self.get(rally_target).is_some() {
                let cmd = crate::command::Command {
                    player: owner,
                    kind: crate::command::CommandKind::Target { units: vec![id], target: rally_target, queue: false },
                };
                crate::orders::apply(self, &cmd);
                return;
            }
        }
        if let Some(r) = rally {
            give(self, id, Order::Move { to: r, attack_move: false }, false);
        } else if ud.layer != Layer::Air {
            // step clear of the door
            let out = pos + FVec::new(Fx::ZERO, Fx::from_ratio(80, 100));
            if self.map.passable_at(out, ud.layer) {
                set_order(self, id, Order::Move { to: out, attack_move: false });
            }
        }
    }

    fn building_fire(&mut self, i: usize) {
        let e = &self.entities[i];
        let id = e.id;
        // keep current target while valid and in range
        let mut target = e.target;
        let valid = match self.get(target) {
            Some(t) => {
                t.on_map()
                    && self.is_enemy(e.owner, t.owner)
                    && combat::can_attack(self, e, t)
                    && self.edge_dist(e.pos, t) <= combat::attack_range(self, e, t)
            }
            None => false,
        };
        if !valid {
            target = 0;
            if (self.tick + id) % 8 == 0 {
                if let Some(t) = combat::find_target(self, i) {
                    target = t;
                }
            }
            self.entities[i].target = target;
        }
        if target != 0 {
            combat::fire_ready(self, i, target);
        }
    }

    fn building_heal(&mut self, i: usize) {
        let e = &self.entities[i];
        let d = data().def(e.def);
        let owner = e.owner;
        let pos = e.pos;
        let r = Fx::from_ratio(d.data.heal_range, 100);
        let heal = d.data.heal;
        let mut targets = Vec::new();
        self.spatial.for_each(pos, r, |oid, slot| {
            let o = &self.entities[slot];
            if o.owner == owner && o.on_map() && data().def(o.def).is_unit() && o.pos.within(pos, r) {
                targets.push(oid);
            }
        });
        for t in targets {
            let mh = self.max_hp(self.get(t).unwrap());
            let o = self.get_mut(t).unwrap();
            o.hp = (o.hp + heal).min(mh);
        }
    }

    fn recount_gatherers(&mut self) {
        for e in self.entities.iter_mut() {
            if e.alive {
                e.gatherers = 0;
                e.miners = 0;
            }
        }
        let n = self.entities.len();
        for i in 1..n {
            let e = &self.entities[i];
            if !e.alive {
                continue;
            }
            let node = match e.order {
                Order::Gather { node } => node,
                _ => continue,
            };
            let working = e.action == Action::Gather;
            if let Some(nd) = self.get_mut(node) {
                nd.gatherers = nd.gatherers.saturating_add(1);
                if working {
                    nd.miners = nd.miners.saturating_add(1);
                }
            }
        }
    }

    /// Fold researched technologies into per-def modifiers.
    pub fn recompute_mods(&mut self, owner: u8) {
        let d = data();
        let p = &self.players[owner as usize];
        let mut mods = vec![UnitMods::default(); d.defs.len()];
        for (ti, done) in p.techs.iter().enumerate() {
            if !*done {
                continue;
            }
            for eff in &d.techs[ti].data.effects {
                for def in &d.defs {
                    let cls = format!("{:?}", def.data.class);
                    let ac = format!("{:?}", def.data.armor_class);
                    let hit = eff.target == "*" || eff.target == def.data.key || eff.target == cls || eff.target == ac;
                    if !hit || def.is_resource() {
                        continue;
                    }
                    let m = &mut mods[def.id as usize];
                    match eff.stat {
                        Stat::Attack => m.attack_pct += eff.value,
                        Stat::Armor => m.armor += eff.value,
                        Stat::Hp => m.hp_pct += eff.value,
                        Stat::Speed => m.speed_pct += eff.value,
                        Stat::Range => m.range += eff.value,
                        Stat::Sight => m.sight_pct += eff.value,
                        Stat::Gather(r) => m.gather_pct[r.idx()] += eff.value,
                        Stat::BuildSpeed => {}
                    }
                }
            }
        }
        // hp upgrades raise current hp proportionally
        let old = std::mem::replace(&mut self.players[owner as usize].mods, mods);
        let new = self.players[owner as usize].mods.clone();
        for e in self.entities.iter_mut() {
            if !e.alive || e.owner != owner {
                continue;
            }
            let o = old[e.def as usize].hp_pct;
            let nn = new[e.def as usize].hp_pct;
            if o != nn {
                e.hp = e.hp * (100 + nn) / (100 + o);
            }
        }
        let _ = (Class::Building, NUM_RES);
    }
}
