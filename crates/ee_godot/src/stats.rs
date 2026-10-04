//! Match statistics for the end screen: a score in the spirit of Empire Earth and a
//! per-player history sampled during play (kept in save files).
use ee_sim::world::{data, World};
use serde::{Deserialize, Serialize};

/// Metrics sampled per player, in this order.
pub const METRICS: [&str; 8] = ["score", "population", "military", "citizens", "buildings", "gathered", "kills", "technologies"];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Sample {
    pub tick: u32,
    /// per player, METRICS order
    pub values: Vec<[f32; 8]>,
}

/// (military, economy, technology) score parts.
pub fn score_parts(w: &World, p: u8) -> (i64, i64, i64) {
    let pl = &w.players[p as usize];
    let military = pl.stats.kills as i64 * 10 + pl.stats.razed as i64 * 50;
    let economy = pl.stats.gathered.iter().sum::<i64>() / 40 + pl.stats.built as i64 * 5;
    let tech = pl.techs.iter().filter(|t| **t).count() as i64 * 150;
    (military, economy, tech)
}

pub fn sample(w: &World) -> Sample {
    let mut values = Vec::new();
    for pl in &w.players {
        let (m, e, t) = score_parts(w, pl.id);
        let (mut army, mut cits, mut bld) = (0, 0, 0);
        for e in &w.entities {
            if !e.alive || e.owner != pl.id {
                continue;
            }
            let d = data().def(e.def);
            if d.is_building() {
                bld += 1;
            } else if d.data.key == "citizen" {
                cits += 1;
            } else if d.is_unit() && d.gather_rate.iter().all(|g| *g == 0) {
                army += 1;
            }
        }
        values.push([
            (m + e + t) as f32,
            pl.pop as f32,
            army as f32,
            cits as f32,
            bld as f32,
            pl.stats.gathered.iter().sum::<i64>() as f32,
            pl.stats.kills as f32,
            pl.techs.iter().filter(|t| **t).count() as f32,
        ]);
    }
    Sample { tick: w.tick, values }
}
