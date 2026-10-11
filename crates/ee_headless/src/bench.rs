//! Profile simulation ticks from a copy of a real match, without launching Godot.
//! `cargo run --release -p ee_headless -- bench-save match.eesave 200`
//! Loading is excluded from timings. Like the game's loader, this resumes saved
//! world/AI state; it does not replay human inputs made after the snapshot.
use serde::Deserialize;
use std::time::Instant;

#[derive(Deserialize)]
struct Snapshot {
    data_hash: u64,
    world: ee_sim::world::World,
    ais: Vec<String>,
    replay: ee_net::Replay,
}

pub fn run(path: &str, ticks: u32) {
    assert!(ticks > 0, "ticks must be positive");
    let text = std::fs::read_to_string(path).expect("read save");
    let saved: Snapshot = ron::from_str(&text).expect("parse save");
    assert_eq!(saved.data_hash, ee_sim::data().hash, "save uses different game data");
    let mut session = ee_net::Session::single_player(saved.replay.config.expect("match config"));
    session.input_delay = saved.replay.input_delay;
    session.world = saved.world;
    session.world.rebuild_caches();
    for state in saved.ais {
        session.add_controller(Box::new(ee_ai::Ai::from_state(&state).expect("AI state")));
    }
    let mut times = Vec::with_capacity(ticks as usize);
    let mut worst_tick = 0;
    let mut worst = 0.0_f64;
    let start = session.world.tick;
    for _ in 0..ticks {
        let t = Instant::now();
        assert!(session.step_once());
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        if ms > worst {
            worst = ms;
            worst_tick = session.world.tick - 1;
        }
        times.push(ms);
    }
    let total: f64 = times.iter().sum();
    times.sort_by(f64::total_cmp);
    println!("ticks {start}..{}: total {total:.2} ms, mean {:.2} ms, p95 {:.2} ms, max {worst:.2} ms at {worst_tick}, checksum {:016x}",
        session.world.tick, total / ticks as f64, times[(times.len() - 1) * 95 / 100], session.world.checksum());
}
