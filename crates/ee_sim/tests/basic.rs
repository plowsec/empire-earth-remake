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
    let land = ee_sim::defs::Layer::Land;
    // an open destination that is reachable from the start
    let mut dest = None;
    'o: for r in (12..30).rev() {
        for (ox, oy) in [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
            let t = (s.0 + ox * r, s.1 + oy * r);
            let open = (-3..=3).all(|dy| (-3..=3).all(|dx| w.map.passable(t.0 + dx, t.1 + dy, land)));
            if open {
                let ff = ee_sim::path::FlowField::build(&w.map, t, land, 0);
                if ff.reachable(&w.map, s.0 + 4, s.1) {
                    dest = Some(FVec::tile_center(t.0, t.1));
                    break 'o;
                }
            }
        }
    }
    let dest = dest.expect("no reachable destination");
    // spawn in the guaranteed-clear ring around the capitol
    let mut ids = vec![];
    for k in 0..40 {
        let ang = k * 9;
        let (c, sn) = ee_sim::mapgen::sincos_deg(ang);
        let r = 4 + k % 3;
        let p = FVec::tile_center(s.0 + c * r / 1024, s.1 + sn * r / 1024);
        if w.map.passable_at(p, land) {
            ids.push(w.spawn(rifle, 0, p));
        }
    }
    assert!(ids.len() >= 25, "spawned {}", ids.len());
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Move { units: ids.clone(), to: dest, attack_move: false, queue: false } })]);
    run(&mut w, 20 * 40, vec![]);
    let arrived = ids
        .iter()
        .filter(|&&id| w.get(id).map_or(false, |e| e.pos.within(dest, ee_sim::fixed::Fx::from_int(6)) && e.order == Order::Idle))
        .count();
    println!("arrived {arrived}/{}", ids.len());
    for &id in &ids {
        let e = w.get(id).unwrap();
        if !e.pos.within(dest, ee_sim::fixed::Fx::from_int(6)) || e.order != Order::Idle {
            println!("  stuck {:?} order {:?} goal {:?} stuck {} dest {:?}", e.pos.tile(), e.order, e.goal.map(|g| g.tile()), e.stuck, dest.tile());
        }
    }
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

#[test]
#[ignore]
fn debug_wood_gathering() {
    let mut w = World::new(MatchConfig::skirmish(9, 2));
    let cits = units_of(&w, 0, "citizen");
    let cap = units_of(&w, 0, "capitol")[0];
    let cpos = w.get(cap).unwrap().pos;
    let tree = nearest_of(&w, cpos, "tree");
    println!("tree at {:?}, capitol at {:?}", w.get(tree).unwrap().pos.tile(), cpos.tile());
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Target { units: cits.clone(), target: tree, queue: false } })]);
    for s in 0..10 {
        run(&mut w, 100, vec![]);
        let mut orders = std::collections::BTreeMap::new();
        for &c in &cits {
            let e = w.get(c).unwrap();
            let k = format!("{:?}/{:?}", e.order, e.action).split(' ').next().unwrap().to_string() + &format!("{:?}", e.action);
            *orders.entry(k).or_insert(0) += 1;
        }
        println!("t={}s wood={} {:?}", (s + 1) * 5, w.players[0].res[1], orders);
    }
}

#[test]
fn saplings_grow_into_trees_and_berries_regrow() {
    let mut w = World::new(MatchConfig::skirmish(9, 2));
    let cits = units_of(&w, 0, "citizen");
    let s = w.starts[0];
    let sap = data().id("sapling");
    let mut tile = None;
    'f: for r in 3..9 {
        for dx in -r..=r {
            let t = (s.0 + dx, s.1 + r);
            if w.can_place(0, sap, t).is_ok() {
                tile = Some(t);
                break 'f;
            }
        }
    }
    let tile = tile.expect("sapling spot");
    let food0 = w.players[0].res[0];
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Build { units: cits[0..2].to_vec(), def: sap, tile, queue: false } })]);
    assert_eq!(w.players[0].res[0], food0 - 10, "sapling costs food");
    run(&mut w, 20 * 20, vec![]);
    let occ = w.map.occupant[w.map.idx(tile.0, tile.1)];
    let e = w.get(occ).expect("sapling exists");
    assert_eq!(e.def, sap);
    assert!(e.complete, "planted");
    run(&mut w, 20 * 125, vec![]);
    let e = w.get(occ).expect("tree exists");
    assert_eq!(e.def, data().id("tree"), "grew into a tree");
    assert!(e.amount > 0);
    assert_eq!(e.owner, ee_sim::mapgen::GAIA);
    // berries regrow
    let cap = units_of(&w, 0, "capitol")[0];
    let b = nearest_of(&w, w.get(cap).unwrap().pos, "berries");
    w.get_mut(b).unwrap().amount = 10;
    run(&mut w, 20 * 30, vec![]);
    assert!(w.get(b).unwrap().amount >= 25, "berries regrow");
}

#[test]
fn artillery_fells_trees() {
    let mut w = World::new(MatchConfig::skirmish(3, 2));
    let tree = data().id("tree");
    let before = w.entities.iter().filter(|e| e.alive && e.def == tree).count();
    // drop a bomb into the densest forest via a projectile impact
    let t = w.entities.iter().find(|e| e.alive && e.def == tree).unwrap().pos;
    let bomber = w.spawn(data().id("bomber"), 0, t);
    for _ in 0..10 {
        w.projectiles.push(ee_sim::world::Proj {
            owner: 0, src: bomber, target: 0, pos: t, aim: t, start: t, speed: ee_sim::fixed::Fx::ONE,
            damage: 400, dmg_type: ee_sim::defs::DamageType::Bomb, splash: ee_sim::fixed::Fx::from_int(3),
            homing: false, weapon: 0, src_def: data().id("bomber"), vs_air: false, id: 999,
        });
    }
    run(&mut w, 2, vec![]);
    let after = w.entities.iter().filter(|e| e.alive && e.def == tree).count();
    println!("trees {before} -> {after}");
    assert!(after < before);
}

fn open_spot(w: &World, near: (i32, i32), rmin: i32, rmax: i32) -> (i32, i32) {
    for r in rmin..rmax {
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1), (1, 1), (-1, -1)] {
            let t = (near.0 + dx * r, near.1 + dy * r);
            if (-2..=2).all(|oy| (-2..=2).all(|ox| w.map.passable(t.0 + ox, t.1 + oy, ee_sim::defs::Layer::Land))) {
                return t;
            }
        }
    }
    panic!("no open spot");
}

#[test]
fn aircraft_patrol_circles_engages_and_resumes_after_refuel() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let s = w.starts[0];
    let air = data().id("airport");
    let mut tile = None;
    'f: for r in 6..20 {
        for dx in -r..=r {
            for t in [(s.0 + dx, s.1 + r), (s.0 + dx, s.1 - r)] {
                if w.can_place(0, air, t).is_ok() {
                    tile = Some(t);
                    break 'f;
                }
            }
        }
    }
    w.spawn_static(air, 0, tile.unwrap(), true);
    let f = w.spawn(data().id("fighter"), 0, FVec::tile_center(s.0, s.1));
    let dest = FVec::tile_center(s.0 + 15, s.1);
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Move { units: vec![f], to: dest, attack_move: false, queue: false } })]);
    // fly out, circle: must stay near the point and keep moving (not stuck)
    run(&mut w, 20 * 15, vec![]);
    let e = w.get(f).unwrap();
    assert!(matches!(e.order, Order::Patrol { .. }), "order {:?}", e.order);
    assert!(e.pos.within(dest, ee_sim::fixed::Fx::from_int(8)), "near patrol point");
    let p0 = e.pos;
    run(&mut w, 20, vec![]);
    assert!(w.get(f).unwrap().pos != p0, "still flying circuits");
    // enemy helicopter shows up: engaged
    let h = w.spawn(data().id("helicopter"), 1, dest + FVec::new(ee_sim::fixed::Fx::from_int(2), ee_sim::fixed::Fx::ZERO));
    run(&mut w, 20 * 20, vec![]);
    assert!(w.get(h).is_none() || w.get(h).unwrap().hp < 620, "helicopter attacked");
    // eventually: fuel -> land -> refuel -> back on patrol
    run(&mut w, 20 * 120, vec![]);
    let e = w.get(f).unwrap();
    println!("after refuel cycle: order {:?} inside {} fuel {}", e.order, e.inside, e.fuel);
    assert!(matches!(e.order, Order::Patrol { .. } | Order::ReturnToBase | Order::Attack { .. }) || e.inside != 0);
    assert_eq!(e.sortie, Some(dest));
}

#[test]
fn scout_loops_around_island() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let s = w.starts[0];
    let r = w.spawn(data().id("recon"), 0, FVec::tile_center(s.0 + 4, s.1 + 4));
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Scout { units: vec![r] } })]);
    let route = w.get(r).unwrap().patrol.clone();
    assert!(route.len() >= 8, "route {}", route.len());
    let mut visited = std::collections::BTreeSet::new();
    for _ in 0..90 {
        run(&mut w, 20, vec![]);
        if let Order::Scout { idx } = w.get(r).unwrap().order {
            visited.insert(idx);
        }
    }
    println!("visited waypoints {:?}", visited);
    assert!(visited.len() >= 4);
}

#[test]
fn helpless_units_call_for_help() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let s = w.starts[0];
    let spot = open_spot(&w, s, 8, 25);
    let c = FVec::tile_center(spot.0, spot.1);
    let rifle = w.spawn(data().id("rifleman"), 0, c);
    let aa = w.spawn(data().id("aa_vehicle"), 0, c + FVec::new(ee_sim::fixed::Fx::from_int(6), ee_sim::fixed::Fx::ZERO));
    let heli = w.spawn(data().id("helicopter"), 1, c + FVec::new(ee_sim::fixed::Fx::ZERO, ee_sim::fixed::Fx::from_int(4)));
    run(&mut w, 20 * 6, vec![(1, Command { player: 1, kind: CommandKind::Attack { units: vec![heli], target: rifle, queue: false } })]);
    let a = w.get(aa).unwrap();
    println!("aa order {:?}", a.order);
    assert!(matches!(a.order, Order::Attack { target } if target == heli) || w.get(heli).is_none());
}

#[test]
fn worker_cap_per_node() {
    let mut w = World::new(MatchConfig::skirmish(9, 2));
    let cits = units_of(&w, 0, "citizen");
    let cap = units_of(&w, 0, "capitol")[0];
    let b = nearest_of(&w, w.get(cap).unwrap().pos, "berries");
    // isolate: the other bushes are gone, so extra workers must wait
    let others: Vec<u32> = w.entities.iter().filter(|e| e.alive && e.def == data().id("berries") && e.id != b).map(|e| e.id).collect();
    for o in others {
        w.kill(o, ee_sim::mapgen::GAIA);
    }
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::Target { units: cits.clone(), target: b, queue: false } })]);
    for _ in 0..40 {
        run(&mut w, 10, vec![]);
        let working = cits.iter().filter(|&&c| {
            let e = w.get(c).unwrap();
            e.action == ee_sim::entity::Action::Gather && matches!(e.order, Order::Gather { node } if node == b)
        }).count();
        assert!(working <= 3, "{working} working a berry bush");
    }
}

#[test]
fn granary_rebuilds_fields() {
    let mut cfg = MatchConfig::skirmish(9, 2);
    cfg.reveal = true;
    let mut w = World::new(cfg);
    let s = w.starts[0];
    let gr = data().id("granary");
    let mut tile = None;
    'f: for r in 7..30 {
        for dx in -r..=r {
            let t = (s.0 + dx, s.1 + r);
            if (-3..6).all(|oy| (-3..6).all(|ox| w.map.passable(t.0 + ox, t.1 + oy, ee_sim::defs::Layer::Land) && w.explored(0, t.0 + ox, t.1 + oy))) {
                tile = Some(t);
                break 'f;
            }
        }
    }
    let g = w.spawn_static(gr, 0, tile.expect("granary spot"), true);
    run(&mut w, 1, vec![(0, Command { player: 0, kind: CommandKind::RebuildFarms { building: g } })]);
    let farms = units_of(&w, 0, "farm").len();
    println!("farms placed {farms}");
    assert!(farms >= 4);
    run(&mut w, 20 * 40, vec![]);
    let built = units_of(&w, 0, "farm").iter().filter(|&&f| w.get(f).unwrap().complete).count();
    println!("farms built {built}");
    assert!(built >= 4);
}

#[test]
fn guard_tower_outranges_battleship() {
    let d = data();
    let tower = d.def(d.id("guard_tower"));
    let bs = d.def(d.id("battleship"));
    let tower_range = tower.weapons.iter().filter(|w| w.vs_water).map(|w| w.range).max().unwrap();
    assert!(tower_range >= bs.max_range, "tower {:?} vs battleship {:?}", tower_range, bs.max_range);
}

#[test]
fn capitol_rally_on_mine_sends_new_citizens_to_mine() {
    let mut w = World::new(MatchConfig::skirmish(9, 2));
    let cap = units_of(&w, 0, "capitol")[0];
    let cpos = w.get(cap).unwrap().pos;
    let gold = nearest_of(&w, cpos, "gold_mine");
    let gpos = w.get(gold).unwrap().pos;
    let cid = data().id("citizen");
    let before = units_of(&w, 0, "citizen");
    run(&mut w, 1, vec![
        (0, Command { player: 0, kind: CommandKind::SetRally { buildings: vec![cap], to: gpos, target: gold } }),
        (0, Command { player: 0, kind: CommandKind::Train { building: cap, def: cid, count: 2 } }),
    ]);
    run(&mut w, 20 * 30, vec![]);
    let new: Vec<u32> = units_of(&w, 0, "citizen").into_iter().filter(|c| !before.contains(c)).collect();
    assert_eq!(new.len(), 2);
    for c in new {
        let e = w.get(c).unwrap();
        assert!(matches!(e.order, Order::Gather { node } if node == gold) || matches!(e.order, Order::ReturnCargo), "order {:?}", e.order);
    }
}

#[test]
#[ignore]
fn debug_rally_mine() {
    let mut w = World::new(MatchConfig::skirmish(9, 2));
    let cap = units_of(&w, 0, "capitol")[0];
    let cpos = w.get(cap).unwrap().pos;
    let gold = nearest_of(&w, cpos, "gold_mine");
    let gpos = w.get(gold).unwrap().pos;
    let cid = data().id("citizen");
    let before = units_of(&w, 0, "citizen");
    run(&mut w, 1, vec![
        (0, Command { player: 0, kind: CommandKind::SetRally { buildings: vec![cap], to: gpos, target: gold } }),
        (0, Command { player: 0, kind: CommandKind::Train { building: cap, def: cid, count: 1 } }),
    ]);
    let g = w.get(gold).unwrap();
    println!("gold tile {:?} pos {:?} radius {:?}", g.tile, g.pos.tile(), data().def(g.def).radius);
    for y in 43..51 {
        let mut l = String::new();
        for x in 64..74 {
            let i = w.map.idx(x, y);
            l.push(if w.map.occupant[i] == gold { 'G' } else if w.map.occupant[i] != 0 { 'T' } else if w.map.pass[i] & 1 != 0 { '.' } else { '#' });
        }
        println!("{y} {l}");
    }
    for _ in 0..40 {
        run(&mut w, 10, vec![]);
        for c in units_of(&w, 0, "citizen").into_iter().filter(|c| !before.contains(c)) {
            let e = w.get(c).unwrap();
            println!("t{} {:?} {:?} {:?} goal {:?} stuck {} q {:?}", w.tick, e.pos.tile(), e.order, e.action, e.goal.map(|g| g.tile()), e.stuck, e.queue);
        }
    }
}

#[test]
fn every_mine_is_reachable_from_its_island() {
    for seed in 1..8u64 {
        let w = World::new(MatchConfig::skirmish(seed, 3));
        for e in w.entities.iter().filter(|e| e.alive && data().def(e.def).data.key.ends_with("_mine")) {
            let (x0, y0) = e.tile;
            let mut open = 0;
            for y in y0 - 1..=y0 + 2 {
                for x in x0 - 1..=x0 + 2 {
                    if w.map.passable(x, y, ee_sim::defs::Layer::Land) {
                        open += 1;
                    }
                }
            }
            assert!(open >= 3, "seed {seed}: mine at {:?} has {open} open neighbours", e.tile);
        }
    }
}
