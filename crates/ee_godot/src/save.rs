//! Save games and replay files.
//!
//! A save is a full snapshot (world + AI brains + the replay so far), so loading is
//! instant and the replay of a resumed game still covers the whole match.
//! Replays are the match setup plus every command; `ee_headless replay` re-simulates
//! them with fresh AIs to analyse their decisions.
use ee_ai::Ai;
use ee_net::{Replay, Session};
use ee_sim::world::World;
use serde::{Deserialize, Serialize};

pub const SAVE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct SaveGame {
    pub version: u32,
    pub data_hash: u64,
    pub world: World,
    /// serialized AI controllers (same order as `replay.ais`)
    pub ais: Vec<String>,
    pub replay: Replay,
    pub reveal: bool,
}

/// Same layout as `SaveGame`, borrowing the live world instead of copying it.
#[derive(Serialize)]
struct SaveOut<'a> {
    version: u32,
    data_hash: u64,
    world: &'a World,
    ais: Vec<String>,
    replay: Replay,
    reveal: bool,
}

pub fn save(session: &Session, reveal: bool) -> Result<String, String> {
    let ais: Vec<String> = session.controller_states().into_iter().map(|s| s.unwrap_or_default()).collect();
    let g = SaveOut {
        version: SAVE_VERSION,
        data_hash: ee_sim::data().hash,
        world: &session.world,
        ais,
        replay: session.make_replay(),
        reveal,
    };
    ron::to_string(&g).map_err(|e| e.to_string())
}

/// Rebuild a session from a save file. Returns (session, reveal).
pub fn load(text: &str) -> Result<(Session, bool), String> {
    let g: SaveGame = ron::from_str(text).map_err(|e| format!("not a save file: {e}"))?;
    if g.version != SAVE_VERSION {
        return Err(format!("save version {} is not supported", g.version));
    }
    if g.data_hash != ee_sim::data().hash {
        return Err("this save was made with different game data".into());
    }
    let cfg = g.replay.config.clone().ok_or("save has no match config")?;
    let mut session = Session::single_player(cfg);
    session.input_delay = g.replay.input_delay;
    session.world = g.world;
    session.world.rebuild_caches();
    session.replay = g.replay.ticks.clone();
    session.checksums = g.replay.checksums.clone();
    session.ai_setup = g.replay.ais.clone();
    for state in &g.ais {
        let ai = Ai::from_state(state).ok_or("corrupt AI state")?;
        session.add_controller(Box::new(ai));
    }
    Ok((session, g.reveal))
}

pub fn replay_text(session: &Session) -> Result<String, String> {
    ron::to_string(&session.make_replay()).map_err(|e| e.to_string())
}
