//! "What would my game look like against today's AI?"
//!
//! Plays a recorded game back twice in lockstep: the original (exact playback of every
//! recorded command) and a new game where the computer players are re-created from the
//! current AI code. The human's recorded commands are translated from the original world
//! into the new one: own units/buildings by "n-th <kind> I created", map resources by
//! tile, enemy targets by kind and position, building plots to the same or nearest free
//! tile. The human's economy and development therefore track the original closely until
//! combat makes the two games genuinely different.
use ee_ai::{Ai, Difficulty};
use ee_net::{Replay, Session};
use ee_sim::command::{Command, CommandKind, TickCommands};
use ee_sim::entity::EntityId;
use ee_sim::fixed::{FVec, Fx};
use ee_sim::mapgen::GAIA;
use ee_sim::world::{data, World};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Key {
    /// owner, def, n-th of that kind created by that owner
    Own(u8, u16, u32),
    /// map resources: def + tile
    Gaia(u16, i32, i32),
}

/// Stable identities for entities, independent of slot allocation.
#[derive(Default)]
struct Births {
    slot_id: Vec<EntityId>,
    key_of: HashMap<EntityId, Key>,
    id_of: HashMap<Key, EntityId>,
    seq: HashMap<(u8, u16), u32>,
}

impl Births {
    fn update(&mut self, w: &World) {
        if self.slot_id.len() < w.entities.len() {
            self.slot_id.resize(w.entities.len(), u32::MAX);
        }
        for (i, e) in w.entities.iter().enumerate() {
            if !e.alive || self.slot_id[i] == e.id {
                continue;
            }
            self.slot_id[i] = e.id;
            let key = if e.owner == GAIA {
                Key::Gaia(e.def, e.tile.0, e.tile.1)
            } else {
                let n = self.seq.entry((e.owner, e.def)).or_insert(0);
                *n += 1;
                Key::Own(e.owner, e.def, *n)
            };
            self.key_of.insert(e.id, key);
            self.id_of.insert(key, e.id);
        }
    }
}

#[derive(Default, Debug)]
struct Stats {
    commands: u32,
    exact: u32,
    partial: u32,
    dropped: u32,
}

struct Translator<'a> {
    orig: &'a World,
    new: &'a World,
    ob: &'a Births,
    nb: &'a Births,
    me: u8,
}

impl Translator<'_> {
    fn entity(&self, id: EntityId) -> Option<EntityId> {
        let key = self.ob.key_of.get(&id)?;
        match key {
            Key::Own(owner, ..) if *owner == self.me => self.nb.id_of.get(key).copied().filter(|&n| self.new.get(n).is_some()),
            Key::Gaia(..) => self.nb.id_of.get(key).copied().filter(|&n| self.new.get(n).is_some()),
            Key::Own(owner, def, _) => {
                // someone else's entity: the same kind near the same place, else anything
                // of theirs close by
                let pos = self.orig.get(id).map(|e| e.pos)?;
                let near = |r: i32, same_def: bool| {
                    self.new
                        .entities
                        .iter()
                        .filter(|e| e.alive && e.inside == 0 && e.owner == *owner && (!same_def || e.def == *def) && e.pos.within(pos, Fx::from_int(r)))
                        .min_by_key(|e| e.pos.dist2_raw(pos))
                        .map(|e| e.id)
                };
                near(30, true).or_else(|| near(10, false))
            }
        }
    }

    fn list(&self, ids: &[EntityId]) -> (Vec<EntityId>, bool) {
        let out: Vec<EntityId> = ids.iter().filter_map(|&i| self.entity(i)).collect();
        let complete = out.len() == ids.len();
        (out, complete)
    }

    /// Same tile if it can be built there, else the nearest tile that can.
    fn plot(&self, def: u16, t: (i32, i32)) -> Option<((i32, i32), bool)> {
        if self.new.can_place(self.me, def, t).is_ok() {
            return Some((t, true));
        }
        for r in 1i32..10 {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs() != r && dy.abs() != r {
                        continue;
                    }
                    let c = (t.0 + dx, t.1 + dy);
                    if self.new.can_place(self.me, def, c).is_ok() {
                        return Some((c, false));
                    }
                }
            }
        }
        None
    }

    /// (translated command, exact?) or None if nothing of it survives.
    fn command(&self, kind: &CommandKind) -> Option<(CommandKind, bool)> {
        use CommandKind as K;
        let units = |v: &[EntityId]| -> Option<(Vec<EntityId>, bool)> {
            let (u, ok) = self.list(v);
            if u.is_empty() { None } else { Some((u, ok)) }
        };
        Some(match kind {
            K::Move { units: u, to, attack_move, queue } => {
                let (u, ok) = units(u)?;
                (K::Move { units: u, to: *to, attack_move: *attack_move, queue: *queue }, ok)
            }
            K::Target { units: u, target, queue } | K::Attack { units: u, target, queue } => {
                let (u, ok) = units(u)?;
                let force = matches!(kind, K::Attack { .. });
                match self.entity(*target) {
                    Some(t) if force => (K::Attack { units: u, target: t, queue: *queue }, ok),
                    Some(t) => (K::Target { units: u, target: t, queue: *queue }, ok),
                    None => {
                        // the target doesn't exist here: head for where it was
                        let to = self.orig.get(*target).map(|e| e.pos)?;
                        let hostile = self.orig.get(*target).map_or(false, |e| e.owner != self.me && e.owner != GAIA);
                        (K::Move { units: u, to, attack_move: hostile, queue: *queue }, false)
                    }
                }
            }
            K::Build { units: u, def, tile, queue } => {
                let (u, ok) = units(u)?;
                let (t, same) = self.plot(*def, *tile)?;
                (K::Build { units: u, def: *def, tile: t, queue: *queue }, ok && same)
            }
            K::Train { building, def, count } => (K::Train { building: self.entity(*building)?, def: *def, count: *count }, true),
            K::Research { building, tech } => (K::Research { building: self.entity(*building)?, tech: *tech }, true),
            K::CancelProduction { building, index } => {
                let b = self.entity(*building)?;
                let len = self.new.get(b)?.production.len();
                if len == 0 {
                    return None;
                }
                (K::CancelProduction { building: b, index: (*index as usize).min(len - 1) as u8 }, (*index as usize) < len)
            }
            K::SetRally { buildings, to, target } => {
                let (b, ok) = units(buildings)?;
                let t = if *target == 0 { 0 } else { self.entity(*target).unwrap_or(0) };
                (K::SetRally { buildings: b, to: *to, target: t }, ok)
            }
            K::Stop { units: u } => { let (u, ok) = units(u)?; (K::Stop { units: u }, ok) }
            K::Unload { units: u, at } => { let (u, ok) = units(u)?; (K::Unload { units: u, at: *at }, ok) }
            K::ReturnToBase { units: u } => { let (u, ok) = units(u)?; (K::ReturnToBase { units: u }, ok) }
            K::Delete { units: u } => { let (u, ok) = units(u)?; (K::Delete { units: u }, ok) }
            K::Scout { units: u } => { let (u, ok) = units(u)?; (K::Scout { units: u }, ok) }
            K::RebuildFarms { building } => (K::RebuildFarms { building: self.entity(*building)? }, true),
            K::Launch { building, at } => (K::Launch { building: self.entity(*building)?, at: *at }, true),
            K::Diplomacy { target, ally } => (K::Diplomacy { target: *target, ally: *ally }, true),
            K::Tribute { to, res } => (K::Tribute { to: *to, res: *res }, true),
            K::Resign => (K::Resign, true),
        })
    }
}

fn summary(w: &World, p: u8) -> String {
    let pl = &w.players[p as usize];
    let mut cits = 0;
    let mut army = 0;
    let mut buildings = 0;
    for e in &w.entities {
        if !e.alive || e.owner != p {
            continue;
        }
        let d = data().def(e.def);
        if d.is_building() {
            buildings += 1;
        } else if d.data.key == "citizen" {
            cits += 1;
        } else if d.is_unit() {
            army += 1;
        }
    }
    let g = pl.stats.gathered;
    format!("cit {cits:>3} army {army:>3} bld {buildings:>3} gathered {:>6} k{} l{}", g.iter().sum::<i64>(), pl.stats.kills, pl.stats.lost)
}

pub fn run(path: &str, every_min: u32, reveal_at: Option<u32>) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let rep: Replay = ron::from_str(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"));
    let Some(cfg) = rep.config.clone() else {
        eprintln!("replay has no match config");
        return;
    };
    crate::sim_version_check(&rep);
    let ai_players: Vec<u8> = rep.ais.iter().map(|a| a.player).collect();
    let me = (0..cfg.players.len() as u8).find(|p| !ai_players.contains(p)).unwrap_or(0);
    let mut orig = World::new(cfg.clone());
    let mut s = Session::single_player(cfg.clone());
    s.input_delay = rep.input_delay;
    for a in &rep.ais {
        s.add_controller(Box::new(Ai::new(a.player, Difficulty::from_index(a.difficulty), a.seed)));
    }
    let mut ob = Births::default();
    let mut nb = Births::default();
    let mut stats = Stats::default();
    let by_tick: HashMap<u32, &TickCommands> = rep.ticks.iter().map(|t| (t.tick, t)).collect();
    println!("robust replay of {path}: you are P{me}; P{:?} re-played by the current AI", ai_players);
    println!("   time | original game (you)                           | vs current AI (you)                           | current AI");
    for t in 0..rep.end_tick {
        if reveal_at == Some(t) {
            orig.config.reveal = true;
            s.world.config.reveal = true;
        }
        ob.update(&orig);
        nb.update(&s.world);
        let empty = TickCommands { tick: t, commands: vec![] };
        let tc = by_tick.get(&t).copied().unwrap_or(&empty);
        // translate the human's commands into the new game
        let mut translated = Vec::new();
        {
            let tr = Translator { orig: &orig, new: &s.world, ob: &ob, nb: &nb, me };
            for c in tc.commands.iter().filter(|c| c.player == me) {
                stats.commands += 1;
                match tr.command(&c.kind) {
                    Some((k, exact)) => {
                        if exact { stats.exact += 1 } else { stats.partial += 1 }
                        translated.push(Command { player: me, kind: k });
                    }
                    None => stats.dropped += 1,
                }
            }
        }
        for c in translated {
            s.schedule(t, c);
        }
        orig.step(tc);
        s.step_once();
        if t % (1200 * every_min.max(1)) == 0 || s.world.game_over || orig.game_over {
            let ai = ai_players.first().copied().unwrap_or(1);
            println!("[{:>3}m] | {} | {} | {}", t / 1200, summary(&orig, me), summary(&s.world, me), summary(&s.world, ai));
            println!("        commands: {} replayed exactly, {} adapted, {} dropped (of {})", stats.exact, stats.partial, stats.dropped, stats.commands);
        }
        if s.world.game_over {
            let won = !s.world.players[me as usize].defeated;
            println!("GAME OVER vs current AI at {}m{}s: {}", t / 1200, (t / 20) % 60, if won { "you still win" } else { "the current AI wins" });
            break;
        }
    }
    for line in s.controller_debug() {
        println!("   {line}");
    }
}

#[allow(dead_code)]
fn _unused(_: FVec) {}
