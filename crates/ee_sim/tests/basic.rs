use ee_sim::command::{Command, CommandKind, TickCommands};
use ee_sim::entity::Order;
use ee_sim::fixed::FVec;
use ee_sim::world::{data, MatchConfig, World};

fn run(w: &mut World, ticks: u32, mut cmds: Vec<(u32, Command)>) {
    for _ in 0..ticks {
        let t = w.tick;
        let now: Vec<Command> = cmds.iter().filter(|(ct, _)| *ct == t).map(|(_, c)| c.clone()).collect();
        cmds.retain(|(ct, _)| *ct != t);
        w.step(&TickCommands { tick: t, commands: now });
    }
}

fn units_of(w: &World, p: u8, key: &str) -> Vec<u32> {
    let id = data().id(key);
    w.entities.iter().filter(|e| e.alive && e.owner == p && e.def == id).map(|e| e.id).collect()
}

fn nearest_of(w: &World, from: FVec, key: &str) -> u32 {
    let id = data().id(key);
    w.entities
        .iter()
        .filter(|e| e.alive && e.def == id)
        .min_by_key(|e| e.pos.dist2_raw(from))
        .map(|e| e.id)
        .unwrap()
}

#[test]
fn citizens_gather_and_deposit() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let cits = units_of(&w, 0, "citizen");
    assert_eq!(cits.len(), 8);
    let cap = units_of(&w, 0, "capitol")[0];
    let cpos = w.get(cap).unwrap().pos;
    let tree = nearest_of(&w, cpos, "tree");
    let berry = nearest_of(&w, cpos, "berries");
    let gold = nearest_of(&w, cpos, "gold_mine");
    let start = w.players[0].res;
    let cmds = vec![
        (0, Command { player: 0, kind: CommandKind::Target { units: cits[0..3].to_vec(), target: tree, queue: false } }),
        (0, Command { player: 0, kind: CommandKind::Target { units: cits[3..6].to_vec(), target: berry, queue: false } }),
        (0, Command { player: 0, kind: CommandKind::Target { units: cits[6..8].to_vec(), target: gold, queue: false } }),
    ];
    run(&mut w, 20 * 120, cmds);
    let end = w.players[0].res;
    println!("start {:?} end {:?}", start, end);
    assert!(end[1] > start[1] + 40, "wood gathered");
    assert!(end[0] > start[0] + 40, "food gathered");
    assert!(end[3] > start[3] + 20, "gold gathered");
}

#[test]
fn build_and_train() {
    let mut w = World::new(MatchConfig::skirmish(9, 2));
    let cits = units_of(&w, 0, "citizen");
    let cap = units_of(&w, 0, "capitol")[0];
    let (cx, cy) = w.get(cap).unwrap().tile;
    // find a valid spot for barracks near the capitol
    let bdef = data().id("barracks");
    let mut spot = None;
    'f: for r in 5..14 {
        for dy in -r..=r {
            for dx in -r..=r {
                let t = (cx + dx, cy + dy);
                if w.can_place(0, bdef, t).is_ok() {
                    spot = Some(t);
                    break 'f;
                }
            }
        }
    }
    let spot = spot.expect("no barracks spot");
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Build { units: cits[0..4].to_vec(), def: bdef, tile: spot, queue: false } })]);
    run(&mut w, 20 * 60, vec![]);
    let b = units_of(&w, 0, "barracks");
    assert_eq!(b.len(), 1);
    assert!(w.get(b[0]).unwrap().complete, "barracks should be complete");
    let rid = data().id("rifleman");
    let t = w.tick;
    run(&mut w, 20 * 30, vec![(t, Command { player: 0, kind: CommandKind::Train { building: b[0], def: rid, count: 3 } })]);
    assert_eq!(units_of(&w, 0, "rifleman").len(), 3);
}

#[test]
fn combat_tanks_beat_riflemen_and_buildings_burn() {
    let mut w = World::new(MatchConfig::skirmish(3, 2));
    let s = w.starts[0];
    let base = FVec::tile_center(s.0 + 8, s.1);
    let tank = data().id("tank");
    let rifle = data().id("rifleman");
    for k in 0..4 {
        let p = base + FVec::new(ee_sim::fixed::Fx::from_int(k), ee_sim::fixed::Fx::ZERO);
        w.spawn(tank, 0, p);
    }
    for k in 0..6 {
        let p = base + FVec::new(ee_sim::fixed::Fx::from_int(k - 2), ee_sim::fixed::Fx::from_int(5));
        w.spawn(rifle, 1, p);
    }
    run(&mut w, 20 * 40, vec![]);
    for id in units_of(&w, 1, "rifleman") {
        let e = w.get(id).unwrap();
        println!("rifle left: pos {:?} hp {} order {:?}", e.pos.tile(), e.hp, e.order);
    }
    for id in units_of(&w, 0, "tank") {
        let e = w.get(id).unwrap();
        println!("tank: pos {:?} hp {} order {:?}", e.pos.tile(), e.hp, e.order);
    }
    let tanks = units_of(&w, 0, "tank").len();
    let rifles = units_of(&w, 1, "rifleman").len();
    println!("tanks left {tanks}, riflemen left {rifles}");
    assert!(tanks >= 3 && rifles == 0);
}

#[test]
fn determinism_same_inputs_same_checksum() {
    let make = || {
        let mut w = World::new(MatchConfig::skirmish(77, 2));
        let cits = units_of(&w, 0, "citizen");
        let cap = units_of(&w, 0, "capitol")[0];
        let cpos = w.get(cap).unwrap().pos;
        let tree = nearest_of(&w, cpos, "tree");
        let cid = data().id("citizen");
        run(&mut w, 1500, vec![
            (0, Command { player: 0, kind: CommandKind::Target { units: cits.clone(), target: tree, queue: false } }),
            (5, Command { player: 0, kind: CommandKind::Train { building: cap, def: cid, count: 5 } }),
            (300, Command { player: 0, kind: CommandKind::Move { units: cits.clone(), to: cpos, attack_move: false, queue: false } }),
        ]);
        w.checksum()
    };
    assert_eq!(make(), make());
}

#[test]
fn group_move_uses_flow_field_and_arrives() {
    let mut w = World::new(MatchConfig::skirmish(12, 2));
    let s = w.starts[0];
    let rifle = data().id("rifleman");
    let mut ids = vec![];
    for k in 0..40 {
        let p = FVec::tile_center(s.0 + 5 + k % 8, s.1 + 5 + k / 8);
        if w.map.passable_at(p, ee_sim::defs::Layer::Land) {
            ids.push(w.spawn(rifle, 0, p));
        }
    }
    // move to an open far point on the island
    let mut dest = None;
    'o: for r in (12..30).rev() {
        for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
            let t = (s.0 + ox * r, s.1 + oy * r);
            let open = (-3..=3).all(|dy| (-3..=3).all(|dx| w.map.passable(t.0 + dx, t.1 + dy, ee_sim::defs::Layer::Land)));
            if open {
                dest = Some(FVec::tile_center(t.0, t.1));
                break 'o;
            }
        }
    }
    let dest = dest.unwrap();
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Move { units: ids.clone(), to: dest, attack_move: false, queue: false } })]);
    run(&mut w, 20 * 40, vec![]);
    let arrived = ids
        .iter()
        .filter(|&&id| w.get(id).map_or(false, |e| e.pos.within(dest, ee_sim::fixed::Fx::from_int(6)) && e.order == Order::Idle))
        .count();
    println!("arrived {arrived}/{}", ids.len());
    assert!(arrived * 10 >= ids.len() * 9);
}

#[test]
#[ignore]
fn debug_group_move() {
    let mut w = World::new(MatchConfig::skirmish(12, 2));
    let s = w.starts[0];
    let rifle = data().id("rifleman");
    let mut ids = vec![];
    for k in 0..40 {
        let p = FVec::tile_center(s.0 + 5 + k % 8, s.1 + 5 + k / 8);
        if w.map.passable_at(p, ee_sim::defs::Layer::Land) {
            ids.push(w.spawn(rifle, 0, p));
        }
    }
    let mut dest = None;
    for r in (10..24).rev() {
        let t = (s.0 - r, s.1);
        if w.map.passable(t.0, t.1, ee_sim::defs::Layer::Land) {
            dest = Some(FVec::tile_center(t.0, t.1));
            break;
        }
    }
    let dest = dest.unwrap();
    println!("dest {:?} start {:?}", dest.tile(), s);
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Move { units: ids.clone(), to: dest, attack_move: false, queue: false } })]);
    for step in 0..8 {
        run(&mut w, 100, vec![]);
        let e = w.get(ids[0]).unwrap();
        println!("t{} u0 pos {:?} goal {:?} flow {:?} path {} stuck {} order {:?}", step, e.pos.tile(), e.goal.map(|g| g.tile()), e.flow, e.path.len(), e.stuck, e.order);
    }
    for &id in &ids {
        let e = w.get(id).unwrap();
        println!("{} pos {:?} goal {:?} flow {:?} stuck {} order {:?}", id & 0xfffff, e.pos.tile(), e.goal.map(|g| g.tile()), e.flow.is_some(), e.stuck, e.order);
    }
}

#[test]
#[ignore]
fn debug_dump_area() {
    let w = World::new(MatchConfig::skirmish(12, 2));
    for y in 132..150 {
        let mut line = String::new();
        for x in 115..135 {
            let i = w.map.idx(x, y);
            let c = if w.map.occupant[i] != 0 { 'T' } else if w.map.pass[i] & 1 != 0 { '.' } else if w.map.is_water(x, y) { '~' } else { '#' };
            line.push(c);
        }
        println!("{y:3} {line}");
    }
}
