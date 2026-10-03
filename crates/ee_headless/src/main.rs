use ee_sim::map::Terrain;
use ee_sim::mapgen::{generate, MapParams};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("map") => {
            let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let players: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2);
            let out = args.get(4).cloned().unwrap_or("map.ppm".into());
            let g = generate(&MapParams { seed, players, size: 1, resources: 100 });
            let m = &g.map;
            let mut img = vec![0u8; (m.w * m.h * 3) as usize];
            for y in 0..m.h {
                for x in 0..m.w {
                    let h = m.tile_height(x, y);
                    let c: [u8; 3] = match m.terrain_at(x, y) {
                        Terrain::DeepWater => [20, 50, 110],
                        Terrain::ShallowWater => [40, 110, 160],
                        Terrain::Beach => [220, 200, 140],
                        Terrain::Grass => [90, 150, 60],
                        Terrain::Meadow => [120, 170, 70],
                        Terrain::Forest => [30, 90, 30],
                        Terrain::Dirt => [140, 120, 70],
                        Terrain::Rock => [130, 120, 110],
                        Terrain::Mountain => [90, 85, 80],
                    };
                    let shade = (h.clamp(0, 600) / 12) as i32;
                    let i = ((y * m.w + x) * 3) as usize;
                    for k in 0..3 {
                        img[i + k] = (c[k] as i32 + shade).clamp(0, 255) as u8;
                    }
                }
            }
            for o in &g.objects {
                let col: [u8; 3] = match o.key {
                    "tree" => [10, 60, 10],
                    "gold_mine" => [255, 215, 0],
                    "stone_mine" => [200, 200, 200],
                    "iron_mine" => [160, 60, 40],
                    "berries" => [200, 0, 120],
                    "fish" => [0, 255, 255],
                    "capitol" => [255, 0, 0],
                    "citizen" => [255, 255, 255],
                    _ => [255, 0, 255],
                };
                let (w, h) = if o.key.ends_with("_mine") { (2, 2) } else if o.key == "capitol" { (4, 4) } else { (1, 1) };
                for y in o.y..o.y + h {
                    for x in o.x..o.x + w {
                        if m.in_bounds(x, y) {
                            let i = ((y * m.w + x) * 3) as usize;
                            img[i..i + 3].copy_from_slice(&col);
                        }
                    }
                }
            }
            let mut f = std::fs::File::create(&out).unwrap();
            write!(f, "P6\n{} {}\n255\n", m.w, m.h).unwrap();
            f.write_all(&img).unwrap();
            println!("wrote {out} ({}x{}, {} objects)", m.w, m.h, g.objects.len());
        }
        Some("match") => {
            let seed: u64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
            let players: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(2);
            let minutes: u32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(40);
            let diff: i32 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(2);
            let pop: i32 = args.get(6).and_then(|s| s.parse().ok()).unwrap_or(300);
            run_match(seed, players, minutes, diff, pop);
        }
        _ => eprintln!("usage: ee_headless map <seed> <players> <out.ppm> | match <seed> <players> <minutes> <difficulty> [pop_limit]"),
    }
}

fn run_match(seed: u64, players: usize, minutes: u32, diff: i32, pop_limit: i32) {
    use ee_ai::{Ai, Difficulty};
    use ee_net::Session;
    use ee_sim::world::{data, MatchConfig};
    let mut cfg = MatchConfig::skirmish(seed, players);
    cfg.pop_limit = pop_limit;
    for p in cfg.players.iter_mut() {
        p.is_ai = true;
    }
    let mut s = Session::new(cfg, vec![], Box::new(ee_net::LocalTransport));
    for p in 0..players {
        s.add_controller(Box::new(Ai::new(p as u8, Difficulty::from_index(diff), seed)));
    }
    let t0 = std::time::Instant::now();
    let total = minutes * 60 * 20;
    let mut max_step = std::time::Duration::ZERO;
    for t in 0..total {
        let st = std::time::Instant::now();
        s.step_once();
        max_step = max_step.max(st.elapsed());
        if t % (20 * 60 * 2) == 0 || s.world.game_over {
            let w = &s.world;
            let units = w.entities.iter().filter(|e| e.alive && e.owner != 255 && data().def(e.def).is_unit()).count();
            print!("[{:>3}m] units {:4} |", t / 1200, units);
            for p in &w.players {
                let mut cnt = std::collections::BTreeMap::new();
                for e in &w.entities {
                    if e.alive && e.owner == p.id {
                        *cnt.entry(data().def(e.def).data.class).or_insert(0) += 1;
                    }
                }
                print!(" P{} pop {}/{} res {:?} {:?} k{} l{} |", p.id, p.pop, p.pop_cap, p.res, cnt, p.stats.kills, p.stats.lost);
            }
            println!();
            // diag for P0
            let mut g = [0; 5];
            let mut idle = 0;
            let mut bk = std::collections::BTreeMap::new();
            for e in &w.entities {
                if !e.alive || e.owner != 0 { continue; }
                let dd = data().def(e.def);
                if dd.is_building() { *bk.entry(dd.data.key.clone()).or_insert(0) += 1; }
                if dd.data.class == ee_sim::defs::Class::Citizen {
                    match e.order {
                        ee_sim::entity::Order::Gather { node } => {
                            if let Some(n) = w.get(node) { if let Some(r) = data().def(n.def).data.resource { g[r as usize] += 1; } }
                        }
                        ee_sim::entity::Order::ReturnCargo => g[(e.carry_res as usize).min(4)] += 1,
                        ee_sim::entity::Order::Idle => idle += 1,
                        _ => {}
                    }
                }
            }
            for line in s.controller_debug() { println!("      {line}"); }
            // units on enemy islands
            for p in &w.players {
                let others: Vec<(i32,i32)> = w.starts.iter().enumerate().filter(|(i,_)| *i as u8 != p.id).map(|(_, s)| *s).collect();
                let n = w.entities.iter().filter(|e| e.alive && e.owner == p.id && e.inside == 0 && data().def(e.def).is_unit()
                    && others.iter().any(|o| e.pos.within(ee_sim::fixed::FVec::tile_center(o.0, o.1), ee_sim::fixed::Fx::from_int(35)))).count();
                print!("      P{} units near enemy bases: {}", p.id, n);
            }
            println!();
            for p in &w.players {
                let sett = w.entities.iter().filter(|e| e.alive && e.owner == p.id && data().def(e.def).data.key == "settlement").count();
                let tow = w.entities.iter().filter(|e| e.alive && e.owner == p.id && data().def(e.def).data.key == "guard_tower").count();
                let sam = w.entities.iter().filter(|e| e.alive && e.owner == p.id && data().def(e.def).data.key == "aa_site").count();
                print!("      P{} towncenters {} towers {} sams {}", p.id, sett, tow, sam);
            }
            println!();
            println!("      P0 gatherers f/w/s/g/i {:?} idle {} buildings {:?}", g, idle, bk);
        }
        if s.world.game_over {
            println!("GAME OVER at {}m{}s winner team {:?}", t / 1200, (t / 20) % 60, s.world.winner_team);
            break;
        }
    }
    println!("sim time {:?} for {} ticks, max step {:?}, checksum {:016x}", t0.elapsed(), s.world.tick, max_step, s.world.checksum());
}
