use ee_sim::{Command, CommandKind, TickCommands, World, MatchConfig, data};
use ee_sim::defs::{ArmorClass, DamageType, Layer};
use ee_sim::entity::{Order, slot_of};
use ee_sim::fixed::{FVec, Fx};

fn step(w: &mut World) {
    w.step(&TickCommands { tick: w.tick, commands: vec![] });
}
fn train(w: &mut World, building: u32, key: &str, count: u8) {
    w.apply_command(&Command { player: 0, kind: CommandKind::Train { building, def: data().id(key), count } });
}
fn finish(w: &mut World, building: u32, key: &str) {
    w.get_mut(building).unwrap().prod_progress = data().def(data().id(key)).build_ticks - 1;
    step(w);
}

#[test]
fn matching_facilities_stack_repairs_and_clamp_hp() {
    for (building, unit, other) in [("naval_yard", "submarine", "tank"), ("tank_factory", "tank", "rifleman"),
        ("airport", "helicopter", "tank"), ("barracks", "rifleman", "tank")] {
        let mut w = World::new(MatchConfig::skirmish(5, 2));
        let pos = FVec::tile_center(20, 20);
        let facility = data().id(building);
        let a = w.spawn(facility, 0, pos);
        w.spawn(facility, 0, pos);
        let unfinished = w.spawn(facility, 0, pos);
        w.get_mut(unfinished).unwrap().complete = false;
        let id = w.spawn(data().id(unit), 0, pos);
        let wrong = w.spawn(data().id(other), 0, pos);
        let enemy = w.spawn(data().id(unit), 1, pos);
        let far = w.spawn(data().id(unit), 0, pos + FVec::new(Fx::from_int(15), Fx::ZERO));
        for u in [id, wrong, enemy, far] {
            let e = w.get_mut(u).unwrap(); e.hp = 10; e.weapon_cd = [999; 3];
        }
        w.tick = 20;
        step(&mut w);
        assert_eq!(w.get(id).unwrap().hp, 10 + 2 * data().def(facility).data.heal, "{building}");
        for u in [wrong, enemy, far] { assert_eq!(w.get(u).unwrap().hp, 10, "wrong target healed at {building}"); }
        w.get_mut(id).unwrap().pos = pos;
        let max = w.max_hp(w.get(id).unwrap());
        w.get_mut(id).unwrap().hp = max - 1;
        w.tick = 40; step(&mut w);
        assert_eq!(w.get(id).unwrap().hp, max);
        assert!(w.get(a).unwrap().complete);
    }
}

#[test]
fn interceptors_are_paid_capped_consumed_and_need_radar() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let pos = FVec::tile_center(20, 20);
    let abm = w.spawn(data().id("abm_site"), 0, pos);
    assert_eq!(w.get(abm).unwrap().ammo, 0);
    let ammo = data().def(data().id("abm_ammo")).data.cost.arr();
    let nuclear = data().def(data().id("icbm")).data.cost.arr();
    assert_eq!(ammo.map(|v| v * 2), nuclear);
    w.players[0].res = [10000; 5];
    train(&mut w, abm, "abm_ammo", 10);
    assert_eq!(w.get(abm).unwrap().production.len(), 3);
    assert_eq!(w.players[0].res, ammo.map(|v| 10000 - 3 * v));
    // Zero-pop rounds can finish even while the empire is over its population cap.
    w.players[0].pop = 9000; w.players[0].pop_cap = 0;
    for _ in 0..3 { finish(&mut w, abm, "abm_ammo"); }
    assert_eq!(w.get(abm).unwrap().ammo, 3);
    assert!(!w.entities.iter().any(|e| e.alive && e.def == data().id("abm_ammo")));
    let before = w.players[0].res;
    train(&mut w, abm, "abm_ammo", 1);
    assert_eq!(w.players[0].res, before);
    let missile = w.spawn(data().id("conventional_icbm"), 1, pos);
    w.players[0].radar = false;
    ee_sim::combat::fire_ready(&mut w, slot_of(abm), missile);
    assert_eq!(w.get(abm).unwrap().ammo, 3);
    w.players[0].radar = true;
    for remaining in (0..3).rev() {
        w.get_mut(abm).unwrap().weapon_cd = [0; 3];
        ee_sim::combat::fire_ready(&mut w, slot_of(abm), missile);
        assert_eq!(w.get(abm).unwrap().ammo, remaining);
    }
    let shots = w.projectiles.len();
    w.get_mut(abm).unwrap().weapon_cd = [0; 3];
    ee_sim::combat::fire_ready(&mut w, slot_of(abm), missile);
    assert_eq!(w.projectiles.len(), shots, "empty ABM cannot fire");
    train(&mut w, abm, "abm_ammo", 1);
    finish(&mut w, abm, "abm_ammo");
    assert_eq!(w.get(abm).unwrap().ammo, 1);
}

#[test]
fn conventional_payloads_launch_in_order_and_never_make_nuclear_waves() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    w.players[0].res = [20000; 5];
    let pos = FVec::tile_center(20, 20);
    let silo = w.spawn(data().id("missile_silo"), 0, pos);
    for key in ["conventional_icbm", "cluster_icbm"] { train(&mut w, silo, key, 1); finish(&mut w, silo, key); }
    let rounds = w.get(silo).unwrap().cargo.clone();
    assert_eq!(rounds.len(), 2);
    let victim = w.spawn(data().id("fortress"), 1, pos);
    let initial = w.get(victim).unwrap().hp;
    for (i, key) in ["conventional_icbm", "cluster_icbm"].iter().enumerate() {
        let d = data().def(data().id(key));
        assert!(d.data.cost.total() < data().def(data().id("abm_ammo")).data.cost.total());
        assert_eq!(w.get(rounds[i]).unwrap().def, d.id);
        w.apply_command(&Command { player: 0, kind: CommandKind::Launch { building: silo, at: pos } });
        step(&mut w);
        assert!(w.get(rounds[i]).is_none());
        assert!(w.shockwaves.is_empty());
    }
    assert!(w.get(victim).unwrap().hp < initial);
    assert!(w.get(silo).unwrap().cargo.is_empty());
}

#[test]
fn coastal_guns_hit_ships_harder_without_boosting_land_damage() {
    let d = data();
    for armor in [ArmorClass::Infantry, ArmorClass::Heavy, ArmorClass::Building] {
        assert_eq!(d.mult(DamageType::CoastalGun, armor), d.mult(DamageType::Cannon, armor));
    }
    assert!(d.mult(DamageType::CoastalGun, ArmorClass::Ship) > 2 * d.mult(DamageType::Cannon, ArmorClass::Ship));
    for key in ["guard_tower", "fortress"] {
        let tower = d.def(d.id(key));
        assert_eq!(tower.weapons[1].dmg_type, DamageType::CoastalGun);
        assert!(ee_sim::combat::weapon_hits(&tower.weapons[2], d.def(d.id("submarine"))));
    }
}

fn worker_world() -> World {
    let mut cfg = MatchConfig::skirmish(5, 2); cfg.reveal = true;
    let mut w = World::new(cfg);
    // Controlled archipelago: broad land halves, a water channel, a wall within the left island.
    for e in &mut w.entities { if e.def == data().id("citizen") { e.order = Order::Move { to: e.pos, attack_move: false }; } }
    for y in 0..w.map.h { for x in 0..w.map.w {
        let i = w.map.idx(x, y);
        let land = x > 1 && y > 1 && x < w.map.w - 2 && y < w.map.h - 2 && x != 30;
        w.map.base_pass[i] = if land { 1 } else { 0 };
        w.map.pass[i] = if land && x != 20 { 1 } else { 0 };
    }}
    w.map.compute_land_ids();
    w
}

#[test]
fn idle_batches_prioritize_viewed_island_without_mixing_workers() {
    let mut w = worker_world();
    let left = w.spawn(data().id("citizen"), 0, FVec::tile_center(29, 10));
    let right = w.spawn(data().id("citizen"), 0, FVec::tile_center(40, 10));
    // Left citizen is closer, but across the water from the camera.
    let ids = w.idle_citizens_near(0, FVec::tile_center(31, 10), 8);
    assert_eq!(ids, vec![right]);
    assert_eq!(w.idle_citizens_near(0, FVec::tile_center(29, 10), 8), vec![left]);
    w.get_mut(left).unwrap().queue.push(Order::Gather { node: 0 });
    assert!(!w.idle_citizens_near(0, FVec::tile_center(29, 10), 8).contains(&left));
}

#[test]
fn mine_dispatch_respects_vacancies_islands_walls_and_busy_citizens() {
    let mut w = worker_world();
    let near = FVec::tile_center(15, 10);
    let reachable = w.spawn_static(data().id("gold_mine"), 255, (12, 10), true);
    w.spawn_static(data().id("iron_mine"), 255, (23, 10), true); // beyond wall
    w.spawn_static(data().id("stone_mine"), 255, (32, 10), true); // another island
    let workers: Vec<_> = (0..12).map(|_| w.spawn(data().id("citizen"), 0, near)).collect();
    let remote = w.spawn(data().id("citizen"), 0, FVec::tile_center(35, 10));
    w.get_mut(workers[0]).unwrap().order = Order::Gather { node: reachable };
    assert_eq!(w.staff_mines(0, near, 8), 7);
    assert_eq!(w.staff_mines(0, near, 8), 0, "repeated dispatch cannot overfill or cross a wall");
    assert_eq!(w.get(remote).unwrap().order, Order::Idle);
    assert_eq!(workers.iter().filter(|&&id| w.get(id).unwrap().order == Order::Gather { node: reachable }).count(), 8);
    assert!(!w.map.passable(20, 10, Layer::Land));
}

#[test]
fn attacked_submarine_gets_idle_helpers_without_stealing_busy_units() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let pos = FVec::tile_center(20, 20);
    let caller = w.spawn(data().id("submarine"), 0, pos);
    let enemy = w.spawn(data().id("frigate"), 1, pos + FVec::new(Fx::from_int(8), Fx::ZERO));
    let helpers: Vec<_> = (0..14).map(|_| w.spawn(data().id("submarine"), 0, pos)).collect();
    let busy = w.spawn(data().id("submarine"), 0, pos);
    w.get_mut(busy).unwrap().order = Order::Move { to: pos, attack_move: false };
    w.rebuild_caches();
    w.tick = 50;
    ee_sim::combat::apply_damage(&mut w, caller, 10, DamageType::Torpedo, 1, enemy);
    assert_eq!(helpers.iter().filter(|&&id| w.get(id).unwrap().order == Order::Attack { target: enemy }).count(), 12);
    assert!(matches!(w.get(busy).unwrap().order, Order::Move { .. }));
    assert_eq!(w.get(helpers[0]).unwrap().help_tick, w.tick);
    let heli = data().def(data().id("asw_helicopter"));
    assert!(ee_sim::combat::weapon_hits(&heli.weapons[0], data().def(data().id("submarine"))));
    for key in ["tank", "fighter", "frigate"] { assert!(!ee_sim::combat::weapon_hits(&heli.weapons[0], data().def(data().id(key)))); }
}

#[test]
fn repair_auras_also_stack_on_aircraft_in_hangars() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let pos = FVec::tile_center(20, 20);
    let a = w.spawn(data().id("airport"), 0, pos);
    w.spawn(data().id("airport"), 0, pos);
    let plane = w.spawn(data().id("helicopter"), 0, pos);
    w.get_mut(plane).unwrap().inside = a;
    w.get_mut(plane).unwrap().hp = 10;
    w.get_mut(a).unwrap().cargo.push(plane);
    let hangar_repair = w.max_hp(w.get(plane).unwrap()) / 40 + 1;
    w.tick = 20;
    step(&mut w);
    assert_eq!(w.get(plane).unwrap().hp, 10 + hangar_repair + 24);
}

#[test]
fn enemy_kills_pay_loot_once_but_friendly_and_natural_losses_do_not() {
    let mut w = World::new(MatchConfig::skirmish(5, 3));
    let pos = FVec::tile_center(20, 20);
    let before = w.players[0].res;
    let mut expected = before;
    for key in ["rifleman", "tank", "battleship", "fortress", "icbm"] {
        let def = data().id(key);
        let victim = w.spawn(def, 1, pos);
        let reward = data().def(def).data.cost.arr().map(|v| v / 4);
        for i in 0..5 { expected[i] += reward[i]; }
        w.kill(victim, 0);
        w.kill(victim, 0); // stale duplicate damage cannot award again
        assert_eq!(w.players[0].res, expected);
    }
    assert_eq!(w.players[0].stats.looted, std::array::from_fn(|i| (expected[i] - before[i]) as i64));
    w.set_diplomacy(0, 2, true); w.set_diplomacy(2, 0, true);
    for (owner, killer, key) in [(0, 0, "tank"), (2, 0, "tank"), (1, 255, "tank"), (255, 0, "tree")] {
        let victim = w.spawn(data().id(key), owner, pos);
        w.kill(victim, killer);
        assert_eq!(w.players[0].res, expected);
    }
    let saved = ron::to_string(&w).unwrap();
    let loaded: World = ron::from_str(&saved).unwrap();
    assert_eq!(loaded.players[0].stats.looted, w.players[0].stats.looted);
    assert_eq!(loaded.checksum(), w.checksum());
}

#[test]
fn transport_cargo_loot_is_awarded_at_the_wreck() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let pos = FVec::tile_center(20, 20);
    let ship = w.spawn(data().id("transport"), 1, pos);
    let passenger = w.spawn(data().id("tank"), 1, FVec::tile_center(60, 60));
    w.get_mut(passenger).unwrap().inside = ship;
    w.get_mut(ship).unwrap().cargo.push(passenger);
    w.events.clear();
    w.kill(ship, 0);
    let rewards: Vec<_> = w.events.iter().filter_map(|e| match e {
        ee_sim::world::SimEvent::Loot { pos, .. } => Some(*pos), _ => None,
    }).collect();
    assert_eq!(rewards, vec![pos, pos]);
}

#[test]
fn carpet_bomber_one_shots_upgraded_infantry_across_its_wider_area() {
    let mut w = World::new(MatchConfig::skirmish(5, 2));
    let pos = FVec::tile_center(20, 20);
    let bomber = w.spawn(data().id("bomber"), 0, pos);
    let wp = &data().def(data().id("bomber")).weapons[0];
    assert_eq!(wp.splash, Fx::from_ratio(550, 100));
    let center = w.spawn(data().id("rifleman"), 1, pos);
    let mut foot = vec![];
    for key in ["citizen", "rifleman", "machine_gunner", "bazooka", "stinger", "sniper", "mortar", "medic"] {
        let def = data().id(key);
        w.players[1].mods[def as usize].hp_pct = 50;
        let id = w.spawn(def, 1, pos + FVec::new(Fx::from_int(5), Fx::ZERO));
        let max = w.max_hp(w.get(id).unwrap());
        w.get_mut(id).unwrap().hp = max;
        foot.push(id);
    }
    let outside = w.spawn(data().id("rifleman"), 1, pos + FVec::new(Fx::from_int(9), Fx::ZERO));
    w.rebuild_caches();
    ee_sim::combat::fire_ready(&mut w, slot_of(bomber), center);
    assert_eq!(w.get(bomber).unwrap().ammo, 0);
    step(&mut w);
    assert!(foot.iter().all(|&id| w.get(id).is_none()));
    assert!(w.get(outside).is_some());
    assert!(w.events.iter().any(|e| matches!(e, ee_sim::world::SimEvent::Impact { source_def, .. } if *source_def == data().id("bomber"))));
    assert!(w.events.iter().any(|e| matches!(e, ee_sim::world::SimEvent::Loot { owner: 0, .. })));
}
