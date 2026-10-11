use super::*;
use ee_sim::{command::{Command, TickCommands}, world::MatchConfig};

fn world() -> World {
    let mut cfg = MatchConfig::skirmish(9, 2);
    cfg.reveal = true;
    World::new(cfg)
}

#[test]
fn scout_survives_economy_assignment_and_returns_to_work() {
    let mut w = world();
    let mut ai = Ai::new(0, Difficulty::Hard, 9);
    w.tick = 40;
    let cmds = ai.think(&w);
    let scout = cmds.iter().find_map(|c| match c { CommandKind::Scout { units } => Some(units[0]), _ => None }).unwrap();
    w.step(&TickCommands { tick: w.tick, commands: cmds.into_iter().map(|kind| Command { player: 0, kind }).collect() });
    assert!(matches!(w.get(scout).unwrap().order, Order::Scout { .. }));
    w.tick = 2400;
    for kind in ai.think(&w) { w.apply_command(&Command { player: 0, kind }); }
    assert_eq!(w.get(scout).unwrap().order, Order::Idle);
    w.tick += 10;
    for kind in ai.think(&w) { w.apply_command(&Command { player: 0, kind }); }
    assert!(!matches!(w.get(scout).unwrap().order, Order::Idle | Order::Scout { .. }));
}

#[test]
fn fields_are_built_with_regrowing_berries_and_spending_is_throttled() {
    let mut w = world();
    let mut ai = Ai::new(0, Difficulty::Hard, 9);
    ai.init(&w);
    let spot = ai.find_site(&w, data().id("granary")).unwrap();
    let granary = w.spawn_static(data().id("granary"), 0, spot, true);
    for _ in 0..20 { w.spawn(data().id("citizen"), 0, ai.base); }
    w.players[0].res = [2000, 5000, 5000, 5000, 5000];
    assert!(w.nearest_resource(0, ai.base, 20, false).is_some());
    let mut cmds = vec![];
    ai.rebuild_fields(&w, &ai.view(&w), &mut cmds);
    assert_eq!(cmds, vec![CommandKind::RebuildFarms { building: granary }]);
    for kind in cmds { w.apply_command(&Command { player: 0, kind }); }
    assert!(w.entities.iter().any(|e| e.alive && e.def == data().id("farm")));
    let mut cmds = vec![];
    ai.rebuild_fields(&w, &ai.view(&w), &mut cmds);
    assert!(cmds.is_empty());
}

#[test]
fn late_hard_ai_produces_nukes_and_targets_visible_clusters() {
    let mut w = world();
    let mut ai = Ai::new(0, Difficulty::Hard, 9);
    ai.init(&w);
    for _ in 0..40 { w.spawn(data().id("citizen"), 0, ai.base); }
    let airport = w.spawn_static(data().id("airport"), 0, (ai.base_tile.0 + 8, ai.base_tile.1), true);
    for key in ["fighter", "bomber", "strike_fighter", "helicopter"] {
        for _ in 0..6 { w.spawn(data().id(key), 0, ai.base); }
    }
    ai.wave = 10;
    w.tick = ai.diff.first_attack();
    w.players[0].res = [10000; 5];
    w.players[0].pop_cap = 300;
    let mut cmds = vec![];
    ai.produce(&w, &ai.view(&w), &mut cmds);
    assert!(cmds.contains(&CommandKind::Train { building: airport, def: data().id("nuke_bomber"), count: 1 }));
    let bomber = w.spawn(data().id("nuke_bomber"), 0, ai.base);
    let s = w.starts[1];
    w.spawn_static(data().id("barracks"), 1, (s.0 + 5, s.1), true);
    let mut cmds = vec![];
    ai.military(&w, &ai.view(&w), &mut cmds);
    assert!(cmds.iter().any(|c| matches!(c, CommandKind::Attack { units, .. } if units.contains(&bomber))));
    w.config.reveal = false;
    for fog in &mut w.vision { fog.fill(0); }
    let mut cmds = vec![];
    ai.military(&w, &ai.view(&w), &mut cmds);
    assert!(!cmds.iter().any(|c| matches!(c, CommandKind::Attack { units, .. } if units.contains(&bomber))));
}

#[test]
fn patrol_aircraft_can_launch_another_strike() {
    let mut w = world();
    let mut ai = Ai::new(0, Difficulty::Hard, 9);
    ai.init(&w);
    ai.observe(&w);
    for _ in 0..4 {
        let id = w.spawn(data().id("fighter"), 0, ai.base);
        w.get_mut(id).unwrap().order = Order::Patrol { at: ai.base };
    }
    w.tick = ai.diff.first_attack();
    let mut cmds = vec![];
    ai.military(&w, &ai.view(&w), &mut cmds);
    assert!(cmds.iter().any(|c| matches!(c, CommandKind::Move { units, .. } if units.len() == 4)));
}

fn nuclear_setup(abms: usize) -> (World, Ai, EntityId) {
    let mut w = world();
    let mut ai = Ai::new(0, Difficulty::Hardest, 9);
    ai.init(&w);
    for _ in 0..60 { w.spawn(data().id("citizen"), 0, ai.base); }
    w.players[0].res = [20000; 5];
    w.tick = ai.diff.first_attack() * 2;
    let silo = w.spawn_static(data().id("missile_silo"), 0, (ai.base_tile.0 + 9, ai.base_tile.1 + 9), true);
    for _ in 0..3 {
        let m = w.spawn(data().id("icbm"), 0, w.get(silo).unwrap().pos);
        w.get_mut(m).unwrap().inside = silo;
        w.get_mut(silo).unwrap().cargo.push(m);
    }
    // defenses already up so the AI goes straight to offense
    w.spawn_static(data().id("radar_station"), 0, (ai.base_tile.0 - 10, ai.base_tile.1 + 8), true);
    w.spawn_static(data().id("abm_site"), 0, (ai.base_tile.0 - 8, ai.base_tile.1 - 8), true);
    let s = w.starts[1];
    for (k, key) in ["barracks", "tank_factory", "airport", "house"].iter().enumerate() {
        w.spawn_static(data().id(key), 1, (s.0 + 4 + k as i32 * 4, s.1 + 6), true);
    }
    w.spawn_static(data().id("radar_station"), 1, (s.0 - 8, s.1 - 6), true);
    for k in 0..abms {
        w.spawn_static(data().id("abm_site"), 1, (s.0 - 4 + k as i32 * 3, s.1 + 12), true);
    }
    w.recount_pop();
    ai.observe(&w);
    (w, ai, silo)
}

#[test]
fn ai_fires_icbm_salvos_sized_to_saturate_interceptors() {
    for abms in [0usize, 2] {
        let (w, mut ai, silo) = nuclear_setup(abms);
        let mut cmds = vec![];
        ai.strategic(&w, &ai.view(&w), &mut cmds);
        let launches = cmds.iter().filter(|c| matches!(c, CommandKind::Launch { building, .. } if *building == silo)).count();
        assert_eq!(launches, abms + 1, "with {abms} enemy ABMs: {cmds:?}");
    }
}

#[test]
fn ai_stockpiles_when_the_target_is_too_well_defended() {
    let (mut w, mut ai, silo) = nuclear_setup(2);
    // only one missile left: a 3-missile salvo is needed, so hold fire
    let extra: Vec<EntityId> = w.get(silo).unwrap().cargo[1..].to_vec();
    for m in extra { w.kill(m, 255); }
    let mut cmds = vec![];
    ai.strategic(&w, &ai.view(&w), &mut cmds);
    assert!(!cmds.iter().any(|c| matches!(c, CommandKind::Launch { .. })), "{cmds:?}");
}

#[test]
fn intruders_anywhere_on_our_island_get_an_immediate_response() {
    let mut w = world();
    let mut ai = Ai::new(1, Difficulty::Hard, 9);
    ai.init(&w);
    // our troops near the capitol, enemy landing party far from it but on our island
    let rifle = data().id("rifleman");
    let ours: Vec<EntityId> = (0..6).map(|k| w.spawn(rifle, 1, ai.base + FVec::new(Fx::from_int(k), Fx::from_int(3)))).collect();
    let (bx, by) = ai.base_tile;
    let far = (0i32..360).step_by(15).find_map(|a| {
        let (c, s) = ee_sim::mapgen::sincos_deg(a);
        (18..40).rev().map(|r| (bx + c * r / 1024, by + s * r / 1024)).find(|&t| w.map.passable(t.0, t.1, ee_sim::defs::Layer::Land) && ai.island_at(&w, t) == Some(ai.home_island))
    }).expect("land far from the capitol");
    for k in 0..4 {
        w.spawn(rifle, 0, FVec::tile_center(far.0 + k % 2, far.1 + k / 2));
    }
    w.tick = 400;
    let mut cmds = vec![];
    ai.hold_zone(&w, &ai.view(&w), &mut cmds);
    let responders = cmds.iter().find_map(|c| match c {
        CommandKind::Move { units, to, attack_move: true, .. } if units.iter().any(|u| ours.contains(u)) => Some(*to),
        _ => None,
    });
    let to = responders.expect("our troops should move against the intruders");
    assert!(to.within(FVec::tile_center(far.0, far.1), Fx::from_int(6)), "response aimed at the intruders");
}

#[test]
fn conquered_islands_are_fortified_and_garrisoned() {
    let mut cfg = MatchConfig::skirmish(29241, 2);
    cfg.reveal = true;
    cfg.map_size = 2;
    let mut w = World::new(cfg);
    let mut ai = Ai::new(0, Difficulty::Hardest, 3);
    ai.init(&w);
    // a decent island that isn't anyone's home, with room for a town center
    let homes: Vec<usize> = w.starts.iter().filter_map(|s| ai.island_at(&w, *s)).collect();
    let sett = data().id("settlement");
    let (isl, tile) = (0..ai.islands.len())
        .filter(|i| !homes.contains(i) && ai.islands[*i].tiles >= 300)
        .find_map(|i| { let c = ai.islands[i].center; ai.plot_near(&w, sett, c, i).map(|t| (i, t)) })
        .expect("an island to settle");
    let tc = w.spawn_static(sett, 0, tile, true);
    let at = w.get(tc).unwrap().pos;
    for _ in 0..4 { w.spawn(data().id("citizen"), 0, at); }
    w.players[0].res = [50_000; 5];
    for _ in 0..20 * 60 * 5 {
        let cmds: Vec<Command> = if w.tick % 4 == 0 { ai.think(&w).into_iter().map(|kind| Command { player: 0, kind }).collect() } else { vec![] };
        let t = w.tick;
        w.step(&ee_sim::command::TickCommands { tick: t, commands: cmds });
        w.players[0].res = [50_000; 5];
    }
    let count = |key: &str| w.entities.iter().filter(|e| e.alive && e.owner == 0 && e.def == data().id(key) && ai.island_at(&w, e.tile).or_else(|| ai.island_at(&w, (e.tile.0 - 1, e.tile.1))) == Some(isl)).count();
    let locals = w.entities.iter().filter(|e| e.alive && e.owner == 0 && e.def == data().id("citizen") && ai.island_at(&w, e.pos.tile()) == Some(isl)).count();
    let mines = w.entities.iter().filter(|e| e.alive && e.amount > 0 && data().def(e.def).data.key.ends_with("_mine") && ai.island_at(&w, e.tile).or_else(|| ai.island_at(&w, (e.tile.0 - 1, e.tile.1))) == Some(isl)).count();
    eprintln!("island {isl}: mines {mines} citizens {locals} towers {} aa {} abm {} yards {} airports {}", count("guard_tower"), count("aa_site"), count("abm_site"), count("naval_yard"), count("airport"));
    assert!(locals >= (mines * 8).max(12) * 8 / 10, "eight settlers per mine ({locals} for {mines} mines)");
    assert!(count("guard_tower") >= 4, "towers around the coast");
    assert!(count("aa_site") >= 2, "anti-air among the towers");
    assert!(count("abm_site") >= 2, "anti-ICBM cover");
    assert!(count("naval_yard") >= 1 && count("airport") >= 1, "a yard and an airfield");
}

#[test]
fn strategic_ai_reloads_empty_interceptors_and_respects_its_budget() {
    let (mut w, mut ai, _) = nuclear_setup(0);
    let abm = data().id("abm_site");
    let ammo = data().id("abm_ammo");
    let site = w.entities.iter().find(|e| e.alive && e.owner == 0 && e.def == abm).unwrap().id;
    w.players[0].res = data().def(ammo).data.cost.arr();
    let v = ai.view(&w);
    let mut cmds = vec![];
    ai.strategic(&w, &v, &mut cmds);
    assert!(cmds.iter().any(|c| matches!(c, CommandKind::Train { building, def, count: 1 } if *building == site && *def == ammo)));
    w.get_mut(site).unwrap().ammo = 3;
    cmds.clear();
    ai.strategic(&w, &v, &mut cmds);
    assert!(!cmds.iter().any(|c| matches!(c, CommandKind::Train { def, .. } if *def == ammo)));
}

#[test]
fn ai_adds_asw_aircraft_when_submarines_are_seen() {
    let mut w = world();
    let mut ai = Ai::new(0, Difficulty::Hard, 9);
    ai.init(&w);
    ai.personality = Personality::Standard;
    w.players[0].res = [10000; 5];
    w.players[0].pop_cap = 3000;
    w.tick = 20000;
    let airport = w.spawn(data().id("airport"), 0, ai.base);
    for (i, key) in ["fighter", "bomber", "strike_fighter", "helicopter"].iter().enumerate() {
        for _ in 0..[5, 5, 3, 4][i] { w.spawn(data().id(key), 0, ai.base); }
    }
    let mut cmds = vec![];
    ai.produce(&w, &ai.view(&w), &mut cmds);
    let asw = data().id("asw_helicopter");
    assert!(!cmds.iter().any(|c| matches!(c, CommandKind::Train { def, .. } if *def == asw)));
    for i in 0..4 { ai.sightings.insert(100000 + i, (3, w.tick)); }
    cmds.clear();
    ai.produce(&w, &ai.view(&w), &mut cmds);
    assert!(cmds.iter().any(|c| matches!(c, CommandKind::Train { building, def, .. } if *building == airport && *def == asw)), "{cmds:?}");
}
