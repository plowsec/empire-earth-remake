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
    w.players[0].res = [5000; 5];
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
